//! Serialized public database API with durable recovery and derived ANN indexes.
use crate::error::{Error, Result};
use crate::kv::{CollectionConfig, KvEngine, Recovery};
use crate::metric::Metric;
use crate::query::Filter;
use crate::vector::{VectorFile, VectorFileWriter, validate_dimension};
#[cfg(feature = "index-hnsw")]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// One query result, sorted by the collection's metric.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredVector {
    /// Database-wide vector ID.
    pub id: u64,
    /// Distance (cosine/L2) or similarity (dot product).
    pub score: f32,
    /// Stored user metadata.
    pub metadata: Option<serde_json::Value>,
}

#[derive(Default)]
struct State {
    #[cfg(feature = "index-hnsw")]
    indexes: HashMap<String, crate::index::IndexFile>,
}

struct Shared {
    engine: KvEngine,
    base_path: PathBuf,
    gate: Mutex<State>,
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 128
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(Error::InvalidInput("collection names must start with an ASCII letter or digit and contain only letters, digits, _ or - (max 128 bytes)".into()));
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

impl Shared {
    fn vectors_path(&self, name: &str) -> PathBuf {
        self.base_path.join(format!("{name}.vectors"))
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        let mut state = self.gate.lock().map_err(|_| Error::Poisoned)?;
        self.recover(&mut state)?;
        Ok(state)
    }

    fn recover(&self, state: &mut State) -> Result<()> {
        for (name, op) in self.engine.pending()? {
            validate_name(&name)
                .map_err(|_| Error::Corrupt("invalid collection name in recovery log".into()))?;
            #[cfg(feature = "index-hnsw")]
            state.indexes.remove(&name);
            #[cfg(not(feature = "index-hnsw"))]
            let _ = &state;
            remove_if_exists(&self.base_path.join(format!("{name}.index")))?;
            let temporary = self.base_path.join(format!("{name}.vectors.tmp"));
            remove_if_exists(&temporary)?;
            let mut offsets = Vec::new();
            match op {
                Recovery::Remove => remove_if_exists(&self.vectors_path(&name))?,
                Recovery::Rebuild => {
                    let config = self
                        .engine
                        .get_collection(&name)?
                        .ok_or_else(|| Error::Corrupt("recovery collection missing".into()))?;
                    let mut writer = VectorFileWriter::create(&temporary, config.dim)?;
                    for (id, _) in self.engine.list_vector_metas(&name)? {
                        let vector = self.engine.payload(id)?;
                        offsets.push((id, writer.append(&vector)?));
                    }
                    writer.flush()?;
                    drop(writer);
                    std::fs::rename(&temporary, self.vectors_path(&name))?;
                }
            }
            sync_directory(&self.base_path)?;
            self.engine.checkpoint(&name, &offsets)?;
        }
        Ok(())
    }
}

/// LMDB capacity configuration. Set before opening a directory.
#[derive(Debug, Clone, Copy)]
pub struct DbOptions {
    /// Maximum LMDB map size in bytes (at least 1 MiB); default 1 GiB.
    /// Includes durable vector copies and metadata. Automatic growth is not supported.
    pub map_size: usize,
}
impl Default for DbOptions {
    fn default() -> Self {
        Self {
            map_size: 1024 * 1024 * 1024,
        }
    }
}

/// One exclusively opened database directory on a local filesystem.
/// Cloned handles share a lock; operations are serialized for coherent snapshots.
#[derive(Clone)]
pub struct VectorDb {
    shared: Arc<Shared>,
}

impl VectorDb {
    /// Open or create a database and replay pending materialization records.
    /// Another engine opening the same directory receives `DatabaseLocked`.
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_options(path, DbOptions::default())
    }

    /// Open with an explicit LMDB capacity. All handles must close before resizing.
    pub fn open_with_options(path: &Path, options: DbOptions) -> Result<Self> {
        let engine = KvEngine::open_with_map_size(path, options.map_size)?;
        let shared = Arc::new(Shared {
            engine,
            base_path: std::fs::canonicalize(path)?,
            gate: Mutex::new(State::default()),
        });
        {
            let _guard = shared.lock()?;
        }
        Ok(Self { shared })
    }

    /// Create a named collection with a positive dimension and metric.
    pub fn create_collection(&self, name: &str, dim: u32, metric: Metric) -> Result<Collection> {
        validate_name(name)?;
        validate_dimension(dim)?;
        let mut state = self.shared.lock()?;
        let config = self.shared.engine.create(name, dim, metric.as_str())?;
        self.shared.recover(&mut state)?;
        Ok(Collection {
            shared: self.shared.clone(),
            name: name.into(),
            generation: config.generation,
        })
    }

    /// Get a handle to an existing collection's current incarnation.
    pub fn collection(&self, name: &str) -> Result<Collection> {
        validate_name(name)?;
        let _state = self.shared.lock()?;
        let config = self
            .shared
            .engine
            .get_collection(name)?
            .ok_or_else(|| Error::CollectionNotFound(name.into()))?;
        Ok(Collection {
            shared: self.shared.clone(),
            name: name.into(),
            generation: config.generation,
        })
    }

    /// List collection names in lexicographic order.
    pub fn list_collections(&self) -> Result<Vec<String>> {
        let _state = self.shared.lock()?;
        self.shared.engine.list_collections()
    }

    /// Delete all collection records and its derived files.
    pub fn delete_collection(&self, name: &str) -> Result<bool> {
        validate_name(name)?;
        let mut state = self.shared.lock()?;
        let existed = self.shared.engine.remove_collection(name)?;
        self.shared.recover(&mut state)?;
        Ok(existed)
    }
}

/// A cheap, cloneable handle bound to one collection incarnation.
#[derive(Clone)]
pub struct Collection {
    shared: Arc<Shared>,
    name: String,
    generation: u64,
}

impl std::fmt::Debug for Collection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collection")
            .field("name", &self.name)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl Collection {
    /// Collection name.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn checked_config(&self) -> Result<CollectionConfig> {
        let config = self
            .shared
            .engine
            .get_collection(&self.name)?
            .ok_or_else(|| Error::CollectionNotFound(self.name.clone()))?;
        if config.generation != self.generation {
            return Err(Error::StaleCollection(self.name.clone()));
        }
        Ok(config)
    }

    /// Current config; fails for deleted/recreated collection handles.
    pub fn config(&self) -> Result<CollectionConfig> {
        let _state = self.shared.lock()?;
        self.checked_config()
    }

    /// Insert a vector and metadata, allocating a database-wide ID.
    /// This legacy name always inserts; it does not update an existing ID.
    /// LMDB commits the payload, metadata, location and recovery marker together.
    /// If file I/O subsequently fails, the operation may have committed; reopening
    /// or the next operation repairs its derived files. Do not blindly retry.
    pub fn upsert_vector(&self, vector: &[f32], metadata: serde_json::Value) -> Result<u64> {
        let mut state = self.shared.lock()?;
        let config = self.checked_config()?;
        validate_vector(vector, config.dim)?;
        let path = self.shared.vectors_path(&self.name);
        let mut writer = VectorFileWriter::open_append(&path)?;
        if writer.dim() != config.dim {
            return Err(Error::Corrupt(
                "collection/vector-file dimension mismatch".into(),
            ));
        }
        let offset = writer.next_offset();
        let id = self
            .shared
            .engine
            .insert(&self.name, vector, &metadata, offset)?;
        #[cfg(feature = "index-hnsw")]
        state.indexes.remove(&self.name);
        #[cfg(not(feature = "index-hnsw"))]
        let _ = &mut state;
        writer.append(vector)?;
        writer.flush()?;
        self.shared.engine.checkpoint(&self.name, &[])?;
        Ok(id)
    }

    /// Delete a vector belonging to this collection; foreign IDs return false.
    /// Space is reclaimed by `compact` or an interrupted-write rebuild.
    pub fn delete_vector(&self, id: u64) -> Result<bool> {
        let mut state = self.shared.lock()?;
        self.checked_config()?;
        let existed = self.shared.engine.remove_vector(&self.name, id)?;
        #[cfg(feature = "index-hnsw")]
        if existed {
            state.indexes.remove(&self.name);
        }
        #[cfg(not(feature = "index-hnsw"))]
        let _ = &mut state;
        Ok(existed)
    }

    /// Reclaim deleted vector space using the durable rebuild protocol.
    /// IDs and metadata are preserved; a cached HNSW index is invalidated.
    pub fn compact(&self) -> Result<()> {
        let mut state = self.shared.lock()?;
        self.checked_config()?;
        self.shared.engine.mark_rebuild(&self.name)?;
        self.shared.recover(&mut state)
    }

    /// Start a nearest-neighbor query. HNSW is used when enabled and unfiltered.
    /// Filtered queries use exact search to preserve filter completeness.
    pub fn query(&self, vector: &[f32], k: usize) -> QueryBuilder<'_> {
        QueryBuilder {
            collection: self,
            query_vector: vector.to_vec(),
            k,
            filter: None,
            exact: false,
            ef_search: 64,
        }
    }
}

fn validate_vector(vector: &[f32], dim: u32) -> Result<()> {
    if vector.len() != dim as usize {
        return Err(Error::DimensionMismatch {
            expected: dim,
            got: u32::try_from(vector.len()).unwrap_or(u32::MAX),
        });
    }
    if vector.iter().any(|v| !v.is_finite()) {
        return Err(Error::InvalidInput(
            "vectors must contain finite values".into(),
        ));
    }
    Ok(())
}

/// Query options for exact or approximate nearest-neighbor search.
pub struct QueryBuilder<'a> {
    collection: &'a Collection,
    query_vector: Vec<f32>,
    k: usize,
    filter: Option<Filter>,
    exact: bool,
    ef_search: usize,
}

impl QueryBuilder<'_> {
    /// Restrict results to matching metadata; this selects exact search.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }
    /// Request the exact scan, useful as an ANN recall baseline.
    pub fn exact(mut self) -> Self {
        self.exact = true;
        self
    }
    /// HNSW candidate budget (at least k). Higher values trade latency for recall.
    pub fn ef_search(mut self, ef: usize) -> Self {
        self.ef_search = ef;
        self
    }
    /// Execute with a coherent snapshot of vectors and metadata.
    pub fn execute(self) -> Result<Vec<ScoredVector>> {
        let mut state = self.collection.shared.lock()?;
        let config = self.collection.checked_config()?;
        validate_vector(&self.query_vector, config.dim)?;
        if self.ef_search == 0 {
            return Err(Error::InvalidInput("ef_search must be positive".into()));
        }
        if self.k == 0 {
            return Ok(Vec::new());
        }
        let metric = config
            .metric()
            .ok_or_else(|| Error::UnknownMetric(config.metric.clone()))?;
        #[cfg(feature = "index-hnsw")]
        if !self.exact && self.filter.is_none() {
            return self.ann(&mut state, &config, metric);
        }
        #[cfg(not(feature = "index-hnsw"))]
        let _ = (&mut state, self.exact);
        let metas = self
            .collection
            .shared
            .engine
            .list_vector_metas(&self.collection.name)?;
        let vf = VectorFile::open(&self.collection.shared.vectors_path(&self.collection.name))?;
        if vf.dim() != config.dim {
            return Err(Error::Corrupt(
                "collection/vector-file dimension mismatch".into(),
            ));
        }
        let mut scored = Vec::new();
        for (id, meta) in metas {
            if meta.dim != config.dim {
                return Err(Error::Corrupt("vector-location dimension mismatch".into()));
            }
            let metadata = self.collection.shared.engine.get_metadata(id)?;
            if self
                .filter
                .as_ref()
                .is_some_and(|f| !metadata.as_ref().is_some_and(|m| f.matches(m)))
            {
                continue;
            }
            scored.push(ScoredVector {
                id,
                score: metric.distance(&self.query_vector, vf.read_at(meta.offset)?),
                metadata,
            });
        }
        scored.sort_by(|a, b| metric.compare(a.score, b.score).then(a.id.cmp(&b.id)));
        scored.truncate(self.k);
        Ok(scored)
    }
}

#[cfg(feature = "index-hnsw")]
impl QueryBuilder<'_> {
    fn ann(
        &self,
        state: &mut State,
        config: &CollectionConfig,
        metric: Metric,
    ) -> Result<Vec<ScoredVector>> {
        let shared = &self.collection.shared;
        let name = &self.collection.name;
        let vf = VectorFile::open(&shared.vectors_path(name))?;
        if vf.dim() != config.dim {
            return Err(Error::Corrupt(
                "collection/vector-file dimension mismatch".into(),
            ));
        }
        if !state
            .indexes
            .get(name)
            .is_some_and(|idx| idx.matches(config.generation, config.revision, config.dim))
        {
            let path = shared.base_path.join(format!("{name}.index"));
            let existing = crate::index::IndexFile::open(&path)
                .ok()
                .filter(|idx| idx.matches(config.generation, config.revision, config.dim));
            let index = if let Some(index) = existing {
                index
            } else {
                let mut vectors = Vec::new();
                for (id, meta) in shared.engine.list_vector_metas(name)? {
                    vectors.push((id, meta.offset, vf.read_at(meta.offset)?.to_vec()));
                }
                let graph = crate::index::Hnsw::build(vectors, metric)?;
                let temporary = shared.base_path.join(format!("{name}.index.tmp"));
                graph.write(&temporary, config.dim, config.generation, config.revision)?;
                std::fs::rename(&temporary, &path)?;
                sync_directory(&shared.base_path)?;
                crate::index::IndexFile::open(&path)?
            };
            state.indexes.insert(name.clone(), index);
        }
        state.indexes[name]
            .search(&vf, &self.query_vector, self.k, self.ef_search, metric)?
            .into_iter()
            .map(|(id, score)| {
                Ok(ScoredVector {
                    id,
                    score,
                    metadata: shared.engine.get_metadata(id)?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use serde_json::json;

    #[test]
    fn create_then_open_an_existing_collection() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        db.create_collection("docs", 3, Metric::Cosine).unwrap();

        let coll = db.collection("docs").unwrap();

        assert_eq!(coll.name(), "docs");
        assert_eq!(coll.config().unwrap().dim, 3);
    }

    #[test]
    fn create_collection_rejects_a_duplicate_name() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        db.create_collection("docs", 3, Metric::Cosine).unwrap();

        let err = db.create_collection("docs", 3, Metric::Cosine).unwrap_err();

        assert!(matches!(err, Error::CollectionAlreadyExists(name) if name == "docs"));
    }

    #[test]
    fn collection_fails_for_an_unknown_name() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();

        let err = db.collection("nope").unwrap_err();

        assert!(matches!(err, Error::CollectionNotFound(name) if name == "nope"));
    }

    #[test]
    fn list_and_delete_collections() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        db.create_collection("a", 1, Metric::Cosine).unwrap();
        db.create_collection("b", 1, Metric::Cosine).unwrap();

        assert_eq!(db.list_collections().unwrap(), vec!["a", "b"]);
        assert!(db.delete_collection("a").unwrap());
        assert!(!db.delete_collection("a").unwrap());
        assert_eq!(db.list_collections().unwrap(), vec!["b"]);
    }

    #[test]
    fn upsert_allocates_sequential_ids_and_round_trips_the_vector() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        let coll = db.create_collection("docs", 3, Metric::Cosine).unwrap();

        let id_a = coll
            .upsert_vector(&[1.0, 0.0, 0.0], json!({"title": "a"}))
            .unwrap();
        let id_b = coll
            .upsert_vector(&[0.0, 1.0, 0.0], json!({"title": "b"}))
            .unwrap();

        assert_eq!(id_a, 0);
        assert_eq!(id_b, 1);
    }

    #[test]
    fn upsert_rejects_a_vector_of_the_wrong_dimension() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        let coll = db.create_collection("docs", 3, Metric::Cosine).unwrap();

        let err = coll.upsert_vector(&[1.0, 0.0], json!(null)).unwrap_err();

        assert!(matches!(
            err,
            Error::DimensionMismatch {
                expected: 3,
                got: 2
            }
        ));
    }

    #[test]
    fn delete_vector_reports_whether_it_existed() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        let coll = db.create_collection("docs", 3, Metric::Cosine).unwrap();
        let id = coll.upsert_vector(&[1.0, 0.0, 0.0], json!(null)).unwrap();

        assert!(coll.delete_vector(id).unwrap());
        assert!(!coll.delete_vector(id).unwrap());
    }

    #[test]
    fn query_returns_nearest_neighbors_in_similarity_order() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        let coll = db.create_collection("docs", 2, Metric::Euclidean).unwrap();

        let near = coll
            .upsert_vector(&[1.0, 1.0], json!({"tags": ["a"]}))
            .unwrap();
        let mid = coll
            .upsert_vector(&[5.0, 5.0], json!({"tags": ["a"]}))
            .unwrap();
        let far = coll
            .upsert_vector(&[10.0, 10.0], json!({"tags": ["a"]}))
            .unwrap();

        let results = coll.query(&[0.0, 0.0], 2).execute().unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, near);
        assert_eq!(results[1].id, mid);
        let _ = far;
    }

    #[test]
    fn query_with_dot_product_ranks_higher_scores_first() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        let coll = db.create_collection("docs", 2, Metric::DotProduct).unwrap();

        let low = coll.upsert_vector(&[1.0, 0.0], json!(null)).unwrap();
        let high = coll.upsert_vector(&[10.0, 0.0], json!(null)).unwrap();

        let results = coll.query(&[1.0, 0.0], 2).execute().unwrap();

        // Dot product: higher = more similar, so `high` must rank first —
        // a plain ascending sort would get this backwards.
        assert_eq!(results[0].id, high);
        assert_eq!(results[1].id, low);
    }

    #[test]
    fn query_filter_excludes_non_matching_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        let coll = db.create_collection("docs", 2, Metric::Euclidean).unwrap();

        let rust_id = coll
            .upsert_vector(&[0.0, 0.0], json!({"tags": ["rust"]}))
            .unwrap();
        coll.upsert_vector(&[0.1, 0.1], json!({"tags": ["python"]}))
            .unwrap();

        let results = coll
            .query(&[0.0, 0.0], 10)
            .filter(Filter {
                field: "tags".into(),
                contains: "rust".into(),
            })
            .execute()
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, rust_id);
    }

    #[test]
    fn query_rejects_a_vector_of_the_wrong_dimension() {
        let dir = tempfile::tempdir().unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        let coll = db.create_collection("docs", 3, Metric::Cosine).unwrap();

        let err = coll.query(&[1.0, 0.0], 5).execute().unwrap_err();

        assert!(matches!(
            err,
            Error::DimensionMismatch {
                expected: 3,
                got: 2
            }
        ));
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, Instant};

    #[test]
    fn crash_worker() {
        let Ok(path) = std::env::var("VDB_TEST_CRASH_PATH") else {
            return;
        };
        let stage = std::env::var("VDB_TEST_CRASH_STAGE").unwrap();
        let db = VectorDb::open(Path::new(&path)).unwrap();
        match stage.as_str() {
            "create" => {
                db.shared.engine.create("pending", 2, "euclidean").unwrap();
            }
            "delete" => {
                db.shared.engine.remove_collection("docs").unwrap();
            }
            "rebuild" => {
                db.shared.engine.mark_rebuild("docs").unwrap();
                let tmp = db.shared.base_path.join("docs.vectors.tmp");
                std::fs::write(tmp, b"partial snapshot").unwrap();
            }
            "insert" | "file" => {
                let c = db.collection("docs").unwrap();
                let mut writer =
                    VectorFileWriter::open_append(&db.shared.vectors_path("docs")).unwrap();
                db.shared
                    .engine
                    .insert(
                        "docs",
                        &[3., 4.],
                        &json!({"recovered":true}),
                        writer.next_offset(),
                    )
                    .unwrap();
                if stage == "file" {
                    writer.append(&[3., 4.]).unwrap();
                    writer.flush().unwrap();
                } else {
                    std::fs::write(db.shared.vectors_path("docs"), b"torn file header").unwrap();
                }
                drop(c);
            }
            "locked" => {}
            _ => panic!("unknown crash stage"),
        }
        std::fs::write(db.shared.base_path.join("ready"), b"ready").unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    #[test]
    fn killed_process_replays_committed_changes_idempotently() {
        for stage in ["create", "insert", "file", "delete", "rebuild", "locked"] {
            let dir = tempfile::tempdir().unwrap();
            {
                let db = VectorDb::open(dir.path()).unwrap();
                let c = db.create_collection("docs", 2, Metric::Euclidean).unwrap();
                c.upsert_vector(&[1., 0.], json!({"original":true}))
                    .unwrap();
                c.query(&[0., 0.], 1).execute().unwrap();
            }
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "db::recovery_tests::crash_worker", "--nocapture"])
                .env("VDB_TEST_CRASH_PATH", dir.path())
                .env("VDB_TEST_CRASH_STAGE", stage)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let start = Instant::now();
            while !dir.path().join("ready").exists() {
                if start.elapsed() > Duration::from_secs(15) || child.try_wait().unwrap().is_some()
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("crash worker failed at {stage}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(matches!(
                VectorDb::open(dir.path()),
                Err(Error::DatabaseLocked(_))
            ));
            child.kill().unwrap();
            child.wait().unwrap();
            for _ in 0..2 {
                let db = VectorDb::open(dir.path()).unwrap();
                assert!(db.shared.engine.pending().unwrap().is_empty());
                if stage == "delete" {
                    assert!(db.list_collections().unwrap().is_empty());
                } else {
                    let c = db.collection("docs").unwrap();
                    let results = c.query(&[0., 0.], 10).execute().unwrap();
                    assert_eq!(
                        results.len(),
                        if stage == "insert" || stage == "file" {
                            2
                        } else {
                            1
                        },
                        "stage={stage}"
                    );
                    if results.len() == 2 {
                        assert_eq!(results[1].score, 5.);
                        assert_eq!(results[1].metadata.as_ref().unwrap()["recovered"], true);
                    }
                    if stage == "create" {
                        assert!(
                            db.collection("pending")
                                .unwrap()
                                .query(&[0., 0.], 1)
                                .execute()
                                .unwrap()
                                .is_empty()
                        );
                    }
                }
            }
        }
    }
}
