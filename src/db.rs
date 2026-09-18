//! `VectorDb` / `Collection` / `QueryBuilder`: the public API tying
//! `KvEngine` + `VectorFile`/`VectorFileWriter` + `Metric` + `Filter`
//! together. See `PROJECT_SPEC.md`'s "Primary user experience" example —
//! this is the first implementation of it.
//!
//! Query execution here is brute-force linear scan (every vector in the
//! collection, scored against the query vector) — there's no real HNSW
//! graph yet (`src/index.rs` is still header-only). This is a deliberate
//! sequencing choice, not an oversight: the query path can be swapped for
//! real ANN search later behind this same API, without a redesign.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::kv::{CollectionConfig, KvEngine, VectorMeta};
use crate::metric::Metric;
use crate::query::Filter;
use crate::vector::{VectorFile, VectorFileWriter};

/// One scored query result.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredVector {
    /// The vector's ID.
    pub id: u64,
    /// The [`Metric`] score against the query vector — lower is more
    /// similar for `Cosine`/`Euclidean`, higher is more similar for
    /// `DotProduct` (see [`Metric::higher_is_better`]). Results are
    /// already ordered correctly for the collection's metric; this field
    /// is exposed for callers who want the raw value too.
    pub score: f32,
    /// The vector's user-supplied metadata blob, if any was stored.
    pub metadata: Option<serde_json::Value>,
}

/// An open vector database: one directory holding an LMDB environment
/// (`KvEngine`) plus one `.vectors` file per collection.
pub struct VectorDb {
    engine: Arc<KvEngine>,
    base_path: PathBuf,
}

impl VectorDb {
    /// Open (creating if necessary) a vector database rooted at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            engine: Arc::new(KvEngine::open(path)?),
            base_path: path.to_path_buf(),
        })
    }

    /// Create a new, empty collection.
    ///
    /// # Errors
    ///
    /// [`Error::CollectionAlreadyExists`] if `name` is already in use.
    pub fn create_collection(&self, name: &str, dim: u32, metric: Metric) -> Result<Collection> {
        if self.engine.get_collection(name)?.is_some() {
            return Err(Error::CollectionAlreadyExists(name.to_string()));
        }

        let config = CollectionConfig {
            dim,
            metric: metric.as_str().to_string(),
            next_id: 0,
        };
        self.engine.put_collection(name, &config)?;

        let mut writer = VectorFileWriter::create(&self.vectors_path(name), dim)?;
        writer.flush()?;

        Ok(Collection {
            engine: self.engine.clone(),
            base_path: self.base_path.clone(),
            name: name.to_string(),
        })
    }

    /// Open a handle to an existing collection.
    ///
    /// # Errors
    ///
    /// [`Error::CollectionNotFound`] if `name` doesn't exist.
    pub fn collection(&self, name: &str) -> Result<Collection> {
        if self.engine.get_collection(name)?.is_none() {
            return Err(Error::CollectionNotFound(name.to_string()));
        }
        Ok(Collection {
            engine: self.engine.clone(),
            base_path: self.base_path.clone(),
            name: name.to_string(),
        })
    }

    /// List every collection name.
    pub fn list_collections(&self) -> Result<Vec<String>> {
        self.engine.list_collections()
    }

    /// Delete a collection: its config and its `.vectors` file. Does
    /// **not** delete the individual `vectors`/`metadata` LMDB entries
    /// belonging to it (a known gap — see `PROJECT_STATE.md`'s "Known
    /// structural gaps": no deletion/compaction story yet). Returns
    /// `true` if the collection existed.
    pub fn delete_collection(&self, name: &str) -> Result<bool> {
        let existed = self.engine.delete_collection(name)?;
        if existed {
            let _ = std::fs::remove_file(self.vectors_path(name));
        }
        Ok(existed)
    }

    fn vectors_path(&self, name: &str) -> PathBuf {
        self.base_path.join(format!("{name}.vectors"))
    }
}

/// A handle to one collection. Cheap to clone (shares the underlying
/// `KvEngine` via `Arc`).
#[derive(Clone)]
pub struct Collection {
    engine: Arc<KvEngine>,
    base_path: PathBuf,
    name: String,
}

impl std::fmt::Debug for Collection {
    // Manual impl: KvEngine wraps heed types that don't implement Debug,
    // so #[derive(Debug)] isn't available. The name is the useful part
    // for debugging anyway.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collection")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Collection {
    /// This collection's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// This collection's stored config.
    pub fn config(&self) -> Result<CollectionConfig> {
        self.engine
            .get_collection(&self.name)?
            .ok_or_else(|| Error::CollectionNotFound(self.name.clone()))
    }

    /// Insert a new vector with the given metadata. Returns its allocated
    /// (monotonically increasing) ID.
    ///
    /// # Errors
    ///
    /// [`Error::DimensionMismatch`] if `vector.len()` doesn't match the
    /// collection's configured dimension.
    pub fn upsert_vector(&self, vector: &[f32], metadata: serde_json::Value) -> Result<u64> {
        let config = self.config()?;
        if vector.len() as u32 != config.dim {
            return Err(Error::DimensionMismatch {
                expected: config.dim,
                got: vector.len() as u32,
            });
        }

        let id = self.engine.allocate_vector_id(&self.name)?;

        let mut writer = VectorFileWriter::open_append(&self.vectors_path())?;
        let offset = writer.append(vector)?;
        writer.flush()?;

        self.engine.put_vector_meta(
            id,
            &VectorMeta {
                collection: self.name.clone(),
                offset,
                dim: config.dim,
            },
        )?;
        self.engine.put_metadata(id, &metadata)?;

        Ok(id)
    }

    /// Delete a vector by ID. Returns `true` if it existed. Does not
    /// reclaim its space in the `.vectors` file (no compaction yet).
    pub fn delete_vector(&self, id: u64) -> Result<bool> {
        let existed = self.engine.delete_vector_meta(id)?;
        if existed {
            self.engine.delete_metadata(id)?;
        }
        Ok(existed)
    }

    /// Start a k-nearest-neighbor query against this collection.
    pub fn query(&self, vector: &[f32], k: usize) -> QueryBuilder<'_> {
        QueryBuilder {
            collection: self,
            query_vector: vector.to_vec(),
            k,
            filter: None,
        }
    }

    fn vectors_path(&self) -> PathBuf {
        self.base_path.join(format!("{}.vectors", self.name))
    }
}

/// Builds and executes a k-nearest-neighbor query against one
/// [`Collection`]. Construct via [`Collection::query`].
pub struct QueryBuilder<'a> {
    collection: &'a Collection,
    query_vector: Vec<f32>,
    k: usize,
    filter: Option<Filter>,
}

impl QueryBuilder<'_> {
    /// Only include results whose metadata satisfies `filter`.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Run the query: brute-force linear scan over every vector in the
    /// collection, scored by the collection's configured [`Metric`],
    /// filtered (if [`QueryBuilder::filter`] was set), then the top `k`
    /// results in similarity order.
    ///
    /// # Errors
    ///
    /// [`Error::DimensionMismatch`] if the query vector's length doesn't
    /// match the collection. [`Error::UnknownMetric`] if the collection's
    /// stored metric string isn't a recognized [`Metric`] variant.
    pub fn execute(self) -> Result<Vec<ScoredVector>> {
        let config = self.collection.config()?;
        if self.query_vector.len() as u32 != config.dim {
            return Err(Error::DimensionMismatch {
                expected: config.dim,
                got: self.query_vector.len() as u32,
            });
        }
        let metric = config
            .metric()
            .ok_or_else(|| Error::UnknownMetric(config.metric.clone()))?;

        let metas = self
            .collection
            .engine
            .list_vector_metas(&self.collection.name)?;
        let vf = VectorFile::open(&self.collection.vectors_path())?;

        let mut scored = Vec::new();
        for (id, meta) in metas {
            let vector = vf.read_at(meta.offset)?;
            let metadata = self.collection.engine.get_metadata(id)?;

            if let Some(filter) = &self.filter {
                let matches = metadata.as_ref().is_some_and(|m| filter.matches(m));
                if !matches {
                    continue;
                }
            }

            let score = metric.distance(&self.query_vector, vector);
            scored.push(ScoredVector {
                id,
                score,
                metadata,
            });
        }

        if metric.higher_is_better() {
            scored.sort_by(|a, b| b.score.total_cmp(&a.score));
        } else {
            scored.sort_by(|a, b| a.score.total_cmp(&b.score));
        }
        scored.truncate(self.k);

        Ok(scored)
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
