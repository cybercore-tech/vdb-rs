//! `KvEngine`: the LMDB (via `heed`) backed KV substrate.
//!
//! See `docs/adr/0001-use-lmdb-as-kv-substrate.md` for why LMDB was chosen,
//! and `docs/storage_layout.md` for the named-database (table) layout this
//! wraps: collection configs, vector locations, metadata, payloads, recovery,
//! allocation state and caller-managed audit logs.

use std::path::Path;

use byteorder::BigEndian;
use heed::types::{SerdeJson, Str, U64};
use heed::{Database, Env, EnvOpenOptions};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Configuration for one collection, stored in the `collections` table.
///
/// `metric` is stored as a plain string label (e.g. `"cosine"`), not the
/// `metrics` feature's [`crate::metric::Metric`] enum — `storage` doesn't
/// depend on `metrics`, so the two features stay independently
/// isolation-testable. When `metrics` is also enabled, use
/// [`CollectionConfig::metric`] to parse it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionConfig {
    /// Vector dimensionality for every vector in this collection.
    pub dim: u32,
    /// Distance/similarity metric, stored as its lowercase name.
    pub metric: String,
    /// Last database-wide allocation plus one for this collection.
    /// Informational only; the state table is the allocation authority.
    pub next_id: u64,
    /// Unique incarnation, preventing deleted handles from accessing replacements.
    #[serde(default)]
    pub generation: u64,
    /// Mutation revision used to invalidate derived indexes.
    #[serde(default)]
    pub revision: u64,
}

#[cfg(feature = "metrics")]
impl CollectionConfig {
    /// Parse `self.metric` into the typed [`crate::metric::Metric`] enum,
    /// if it's a recognized name.
    pub fn metric(&self) -> Option<crate::metric::Metric> {
        match self.metric.as_str() {
            "cosine" => Some(crate::metric::Metric::Cosine),
            "euclidean" => Some(crate::metric::Metric::Euclidean),
            "dot_product" => Some(crate::metric::Metric::DotProduct),
            _ => None,
        }
    }
}

/// Where one vector's data lives, stored in the `vectors` table, keyed by
/// vector ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VectorMeta {
    /// Name of the collection this vector belongs to.
    pub collection: String,
    /// Byte offset into that collection's `.vectors` file.
    pub offset: u64,
    /// Vector dimensionality (redundant with the file header — kept here
    /// so callers can validate without opening the file).
    pub dim: u32,
}

/// The kind of change an audit [`Operation`] records.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationType {
    /// A vector was inserted or replaced.
    Upsert,
    /// A vector was deleted.
    Delete,
}

/// Legacy caller-managed audit entry in the `logs` table.
/// Database recovery uses the transactional `pending` and `payloads` tables;
/// these audit entries are not replay instructions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Operation {
    /// What kind of change this entry records.
    pub op_type: OperationType,
    /// The vector ID this operation applies to.
    pub vector_id: u64,
    /// Unix timestamp (seconds) the operation was logged at.
    pub timestamp: u64,
}

/// A durable instruction to rebuild or remove a collection's derived files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum Recovery {
    Rebuild,
    Remove,
}

/// Exclusively opened LMDB substrate. Prefer `VectorDb` for transactional writes.
pub struct KvEngine {
    env: Env,
    collections: Database<Str, SerdeJson<CollectionConfig>>,
    vectors: Database<U64<BigEndian>, SerdeJson<VectorMeta>>,
    metadata: Database<U64<BigEndian>, SerdeJson<serde_json::Value>>,
    logs: Database<U64<BigEndian>, SerdeJson<Operation>>,
    state: Database<Str, U64<BigEndian>>,
    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    payloads: Database<U64<BigEndian>, SerdeJson<Vec<f32>>>,
    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    pending: Database<Str, SerdeJson<Recovery>>,
    // Declared after env so the directory remains locked through LMDB close.
    _lock: std::fs::File,
}

impl KvEngine {
    /// Open (creating if necessary) an LMDB environment rooted at `path`.
    ///
    /// # Safety invariants
    ///
    /// `heed::EnvOpenOptions::open` is `unsafe`: the caller must not open
    /// the same environment directory from multiple processes with
    /// mismatched `map_size`/`max_dbs`, must not hold transactions across a
    /// process abort, and must not point it at a remote filesystem. `path`
    /// is expected to be owned exclusively by one `KvEngine` at a time —
    /// enforced by an advisory directory lock.
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_map_size(path, 1024 * 1024 * 1024)
    }

    pub(crate) fn open_with_map_size(path: &Path, map_size: usize) -> Result<Self> {
        if map_size < 1024 * 1024 {
            return Err(Error::InvalidInput(
                "LMDB map size must be at least 1 MiB".into(),
            ));
        }
        std::fs::create_dir_all(path)?;

        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.join("vdb.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock).map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                Error::DatabaseLocked(path.display().to_string())
            } else {
                Error::Io(e)
            }
        })?;
        // Safety: the advisory lock excludes other engines using this API.
        // Callers must keep the directory on a local filesystem and must not
        // modify LMDB or mapped files externally while this engine is open.
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(map_size)
                .max_dbs(7) // collections, vectors, metadata, logs, state, payloads, pending
                .open(path)?
        };

        let mut wtxn = env.write_txn()?;
        let collections = env.create_database(&mut wtxn, Some("collections"))?;
        let vectors = env.create_database(&mut wtxn, Some("vectors"))?;
        let metadata = env.create_database(&mut wtxn, Some("metadata"))?;
        let logs = env.create_database(&mut wtxn, Some("logs"))?;
        let state: Database<Str, U64<BigEndian>> = env.create_database(&mut wtxn, Some("state"))?;
        let _payloads: Database<U64<BigEndian>, SerdeJson<Vec<f32>>> =
            env.create_database(&mut wtxn, Some("payloads"))?;
        let _pending: Database<Str, SerdeJson<Recovery>> =
            env.create_database(&mut wtxn, Some("pending"))?;
        match state.get(&wtxn, "format")? {
            None => {
                if !collections.is_empty(&wtxn)?
                    || !vectors.is_empty(&wtxn)?
                    || !metadata.is_empty(&wtxn)?
                    || !logs.is_empty(&wtxn)?
                {
                    return Err(Error::Corrupt(
                        "legacy pre-alpha database; export with its original revision first".into(),
                    ));
                }
                state.put(&mut wtxn, "format", &2)?;
            }
            Some(2) => {}
            Some(version) => {
                return Err(Error::Corrupt(format!(
                    "unsupported database format {version}"
                )));
            }
        }
        wtxn.commit()?;

        Ok(Self {
            env,
            collections,
            vectors,
            metadata,
            logs,
            state,
            #[cfg(all(feature = "metrics", feature = "serde-query"))]
            payloads: _payloads,
            #[cfg(all(feature = "metrics", feature = "serde-query"))]
            pending: _pending,
            _lock: lock,
        })
    }

    /// Fetch a collection's stored config by name, if it exists.
    pub fn get_collection(&self, name: &str) -> Result<Option<CollectionConfig>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.collections.get(&rtxn, name)?)
    }

    /// Insert or replace a collection's stored config.
    pub fn put_collection(&self, name: &str, config: &CollectionConfig) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.collections.put(&mut wtxn, name, config)?;
        wtxn.commit()?;
        Ok(())
    }

    /// List every collection name, in key (lexicographic) order.
    pub fn list_collections(&self) -> Result<Vec<String>> {
        let rtxn = self.env.read_txn()?;
        let names: Result<Vec<String>> = self
            .collections
            .iter(&rtxn)?
            .map(|entry| Ok(entry?.0.to_string()))
            .collect();
        names
    }

    /// Remove a collection's stored config. Returns `true` if it existed.
    ///
    /// Does not touch the `vectors`/`metadata` entries belonging to it, or
    /// its `.vectors` file — callers (`VectorDb::delete_collection`) are
    /// responsible for that cleanup.
    pub fn delete_collection(&self, name: &str) -> Result<bool> {
        let mut wtxn = self.env.write_txn()?;
        let existed = self.collections.delete(&mut wtxn, name)?;
        wtxn.commit()?;
        Ok(existed)
    }

    /// Atomically allocate a database-wide ID for `collection` (read the
    /// database-wide `next_id`, increment it, write it back, all in one LMDB
    /// transaction) and return the allocated ID.
    pub fn allocate_vector_id(&self, collection: &str) -> Result<u64> {
        let mut wtxn = self.env.write_txn()?;
        let mut config = self
            .collections
            .get(&wtxn, collection)?
            .ok_or_else(|| crate::error::Error::CollectionNotFound(collection.to_string()))?;
        let id = self.state.get(&wtxn, "next_id")?.unwrap_or(0);
        let next = id
            .checked_add(1)
            .ok_or_else(|| Error::Corrupt("ID space exhausted".into()))?;
        config.next_id = next;
        self.state.put(&mut wtxn, "next_id", &next)?;
        self.collections.put(&mut wtxn, collection, &config)?;
        wtxn.commit()?;
        Ok(id)
    }

    /// Fetch a vector's stored location/metadata by ID, if it exists.
    pub fn get_vector_meta(&self, id: u64) -> Result<Option<VectorMeta>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.vectors.get(&rtxn, &id)?)
    }

    /// Insert or replace a vector's location/metadata.
    pub fn put_vector_meta(&self, id: u64, meta: &VectorMeta) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.vectors.put(&mut wtxn, &id, meta)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Remove a vector's stored location/metadata. Returns `true` if it existed.
    pub fn delete_vector_meta(&self, id: u64) -> Result<bool> {
        let mut wtxn = self.env.write_txn()?;
        let existed = self.vectors.delete(&mut wtxn, &id)?;
        wtxn.commit()?;
        Ok(existed)
    }

    /// List every `(id, VectorMeta)` belonging to `collection`.
    ///
    /// Full scan of the `vectors` table, filtered client-side — the table
    /// has no secondary index by collection yet. Fine for the current
    /// brute-force query path; revisit if this becomes a bottleneck.
    pub fn list_vector_metas(&self, collection: &str) -> Result<Vec<(u64, VectorMeta)>> {
        let rtxn = self.env.read_txn()?;
        let metas: Result<Vec<(u64, VectorMeta)>> = self
            .vectors
            .iter(&rtxn)?
            .map(|entry| Ok(entry?))
            .filter(|entry: &Result<(u64, VectorMeta)>| {
                entry
                    .as_ref()
                    .map(|(_, m)| m.collection == collection)
                    .unwrap_or(true)
            })
            .collect();
        metas
    }

    /// Fetch a vector's user-supplied metadata blob by ID, if it exists.
    pub fn get_metadata(&self, id: u64) -> Result<Option<serde_json::Value>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.metadata.get(&rtxn, &id)?)
    }

    /// Insert or replace a vector's user-supplied metadata blob.
    pub fn put_metadata(&self, id: u64, value: &serde_json::Value) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.metadata.put(&mut wtxn, &id, value)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Remove a vector's user-supplied metadata blob. Returns `true` if it existed.
    pub fn delete_metadata(&self, id: u64) -> Result<bool> {
        let mut wtxn = self.env.write_txn()?;
        let existed = self.metadata.delete(&mut wtxn, &id)?;
        wtxn.commit()?;
        Ok(existed)
    }

    /// Append one caller-managed audit entry, keyed by a caller-supplied monotonic sequence number.
    pub fn append_log(&self, seq: u64, op: &Operation) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.logs.put(&mut wtxn, &seq, op)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Return every caller-managed audit entry in sequence order.
    ///
    /// This is an audit-log reader; public API recovery uses pending markers.
    pub fn replay_log(&self) -> Result<Vec<(u64, Operation)>> {
        let rtxn = self.env.read_txn()?;
        let entries: Result<Vec<(u64, Operation)>> =
            self.logs.iter(&rtxn)?.map(|entry| Ok(entry?)).collect();
        entries
    }
    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    pub(crate) fn create(&self, name: &str, dim: u32, metric: &str) -> Result<CollectionConfig> {
        let mut txn = self.env.write_txn()?;
        if self.collections.get(&txn, name)?.is_some() {
            return Err(Error::CollectionAlreadyExists(name.into()));
        }
        let generation = self
            .state
            .get(&txn, "generation")?
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::Corrupt("generation exhausted".into()))?;
        let config = CollectionConfig {
            dim,
            metric: metric.into(),
            next_id: 0,
            generation,
            revision: 0,
        };
        self.state.put(&mut txn, "generation", &generation)?;
        self.collections.put(&mut txn, name, &config)?;
        self.pending.put(&mut txn, name, &Recovery::Rebuild)?;
        txn.commit()?;
        Ok(config)
    }

    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    #[cfg(test)]
    pub(crate) fn insert(
        &self,
        collection: &str,
        vector: &[f32],
        metadata: &serde_json::Value,
        offset: u64,
    ) -> Result<u64> {
        Ok(self.insert_batch(collection, &[(vector, metadata)], offset)?[0])
    }

    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    pub(crate) fn insert_batch(
        &self,
        collection: &str,
        rows: &[(&[f32], &serde_json::Value)],
        offset: u64,
    ) -> Result<Vec<u64>> {
        let mut txn = self.env.write_txn()?;
        let mut config = self
            .collections
            .get(&txn, collection)?
            .ok_or_else(|| Error::CollectionNotFound(collection.into()))?;
        let first = self.state.get(&txn, "next_id")?.unwrap_or(0);
        let next = first
            .checked_add(rows.len() as u64)
            .ok_or_else(|| Error::Corrupt("ID space exhausted".into()))?;
        config.next_id = next;
        config.revision = config
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::Corrupt("revision exhausted".into()))?;
        self.state.put(&mut txn, "next_id", &next)?;
        self.collections.put(&mut txn, collection, &config)?;
        let stride = crate::vector::block_len(config.dim) as u64;
        let mut ids = Vec::with_capacity(rows.len());
        for (i, (vector, metadata)) in rows.iter().enumerate() {
            let id = first + i as u64;
            let offset = (i as u64)
                .checked_mul(stride)
                .and_then(|n| offset.checked_add(n))
                .ok_or_else(|| Error::Corrupt("vector offset exhausted".into()))?;
            self.vectors.put(
                &mut txn,
                &id,
                &VectorMeta {
                    collection: collection.into(),
                    offset,
                    dim: config.dim,
                },
            )?;
            self.metadata.put(&mut txn, &id, metadata)?;
            self.payloads.put(&mut txn, &id, &vector.to_vec())?;
            ids.push(id);
        }
        self.pending.put(&mut txn, collection, &Recovery::Rebuild)?;
        txn.commit()?;
        Ok(ids)
    }

    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    pub(crate) fn remove_vector(&self, collection: &str, id: u64) -> Result<bool> {
        let mut txn = self.env.write_txn()?;
        let Some(meta) = self.vectors.get(&txn, &id)? else {
            return Ok(false);
        };
        if meta.collection != collection {
            return Ok(false);
        }
        let mut config = self
            .collections
            .get(&txn, collection)?
            .ok_or_else(|| Error::CollectionNotFound(collection.into()))?;
        config.revision = config
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::Corrupt("revision exhausted".into()))?;
        self.collections.put(&mut txn, collection, &config)?;
        self.vectors.delete(&mut txn, &id)?;
        self.metadata.delete(&mut txn, &id)?;
        self.payloads.delete(&mut txn, &id)?;
        txn.commit()?;
        Ok(true)
    }

    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    pub(crate) fn remove_collection(&self, collection: &str) -> Result<bool> {
        let mut txn = self.env.write_txn()?;
        if !self.collections.delete(&mut txn, collection)? {
            return Ok(false);
        }
        let ids: Vec<u64> = self
            .vectors
            .iter(&txn)?
            .filter_map(|r| match r {
                Ok((id, meta)) if meta.collection == collection => Some(Ok(id)),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<std::result::Result<_, _>>()?;
        for id in ids {
            self.vectors.delete(&mut txn, &id)?;
            self.metadata.delete(&mut txn, &id)?;
            self.payloads.delete(&mut txn, &id)?;
        }
        self.pending.put(&mut txn, collection, &Recovery::Remove)?;
        txn.commit()?;
        Ok(true)
    }

    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    pub(crate) fn pending(&self) -> Result<Vec<(String, Recovery)>> {
        let txn = self.env.read_txn()?;
        self.pending
            .iter(&txn)?
            .map(|r| {
                let (name, op) = r?;
                Ok((name.to_owned(), op))
            })
            .collect()
    }

    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    pub(crate) fn payload(&self, id: u64) -> Result<Vec<f32>> {
        let txn = self.env.read_txn()?;
        self.payloads
            .get(&txn, &id)?
            .ok_or_else(|| Error::Corrupt(format!("missing recovery payload for vector {id}")))
    }

    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    pub(crate) fn mark_rebuild(&self, collection: &str) -> Result<()> {
        let mut txn = self.env.write_txn()?;
        let mut config = self
            .collections
            .get(&txn, collection)?
            .ok_or_else(|| Error::CollectionNotFound(collection.into()))?;
        config.revision = config
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::Corrupt("revision exhausted".into()))?;
        self.collections.put(&mut txn, collection, &config)?;
        self.pending.put(&mut txn, collection, &Recovery::Rebuild)?;
        txn.commit()?;
        Ok(())
    }

    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    pub(crate) fn checkpoint(&self, collection: &str, offsets: &[(u64, u64)]) -> Result<()> {
        let mut txn = self.env.write_txn()?;
        for &(id, offset) in offsets {
            let mut meta = self
                .vectors
                .get(&txn, &id)?
                .ok_or_else(|| Error::Corrupt("missing vector at checkpoint".into()))?;
            meta.offset = offset;
            self.vectors.put(&mut txn, &id, &meta)?;
        }
        self.pending.delete(&mut txn, collection)?;
        txn.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_initializes_storage_tables() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        assert!(engine.get_collection("docs").unwrap().is_none());
        assert!(engine.get_vector_meta(1).unwrap().is_none());
        assert!(engine.get_metadata(1).unwrap().is_none());
        assert_eq!(engine.replay_log().unwrap(), Vec::new());
    }

    #[test]
    fn put_then_get_round_trips_a_collection_config() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        let config = CollectionConfig {
            dim: 768,
            metric: "cosine".into(),
            next_id: 0,
            generation: 0,
            revision: 0,
        };

        engine.put_collection("docs", &config).unwrap();

        assert_eq!(engine.get_collection("docs").unwrap(), Some(config));
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn collection_config_metric_parses_a_recognized_name() {
        let config = CollectionConfig {
            dim: 3,
            metric: "euclidean".into(),
            next_id: 0,
            generation: 0,
            revision: 0,
        };
        assert_eq!(config.metric(), Some(crate::metric::Metric::Euclidean));

        let unknown = CollectionConfig {
            dim: 3,
            metric: "manhattan".into(),
            next_id: 0,
            generation: 0,
            revision: 0,
        };
        assert_eq!(unknown.metric(), None);
    }

    #[test]
    fn list_collections_returns_every_created_collection_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        engine
            .put_collection(
                "b",
                &CollectionConfig {
                    dim: 1,
                    metric: "cosine".into(),
                    next_id: 0,
                    generation: 0,
                    revision: 0,
                },
            )
            .unwrap();
        engine
            .put_collection(
                "a",
                &CollectionConfig {
                    dim: 1,
                    metric: "cosine".into(),
                    next_id: 0,
                    generation: 0,
                    revision: 0,
                },
            )
            .unwrap();

        assert_eq!(engine.list_collections().unwrap(), vec!["a", "b"]);
    }

    #[test]
    fn delete_collection_reports_whether_it_existed() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        engine
            .put_collection(
                "docs",
                &CollectionConfig {
                    dim: 1,
                    metric: "cosine".into(),
                    next_id: 0,
                    generation: 0,
                    revision: 0,
                },
            )
            .unwrap();

        assert!(engine.delete_collection("docs").unwrap());
        assert!(!engine.delete_collection("docs").unwrap());
        assert!(engine.get_collection("docs").unwrap().is_none());
    }

    #[test]
    fn allocate_vector_id_increments_monotonically() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        engine
            .put_collection(
                "docs",
                &CollectionConfig {
                    dim: 1,
                    metric: "cosine".into(),
                    next_id: 0,
                    generation: 0,
                    revision: 0,
                },
            )
            .unwrap();

        assert_eq!(engine.allocate_vector_id("docs").unwrap(), 0);
        assert_eq!(engine.allocate_vector_id("docs").unwrap(), 1);
        assert_eq!(engine.allocate_vector_id("docs").unwrap(), 2);
    }

    #[test]
    fn allocate_vector_id_fails_for_an_unknown_collection() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();

        let err = engine.allocate_vector_id("nope").unwrap_err();

        assert!(matches!(err, crate::error::Error::CollectionNotFound(name) if name == "nope"));
    }

    #[test]
    fn list_vector_metas_only_returns_the_requested_collection() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        let a = VectorMeta {
            collection: "a".into(),
            offset: 0,
            dim: 1,
        };
        let b = VectorMeta {
            collection: "b".into(),
            offset: 0,
            dim: 1,
        };
        engine.put_vector_meta(1, &a).unwrap();
        engine.put_vector_meta(2, &b).unwrap();
        engine.put_vector_meta(3, &a).unwrap();

        let metas = engine.list_vector_metas("a").unwrap();

        assert_eq!(metas, vec![(1, a.clone()), (3, a)]);
    }

    #[test]
    fn delete_metadata_reports_whether_it_existed() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        engine
            .put_metadata(1, &serde_json::json!({"a": 1}))
            .unwrap();

        assert!(engine.delete_metadata(1).unwrap());
        assert!(!engine.delete_metadata(1).unwrap());
        assert!(engine.get_metadata(1).unwrap().is_none());
    }

    #[test]
    fn put_then_get_round_trips_vector_meta() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        let meta = VectorMeta {
            collection: "docs".into(),
            offset: 32,
            dim: 768,
        };

        engine.put_vector_meta(1, &meta).unwrap();

        assert_eq!(engine.get_vector_meta(1).unwrap(), Some(meta));
    }

    #[test]
    fn delete_vector_meta_reports_whether_it_existed() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        let meta = VectorMeta {
            collection: "docs".into(),
            offset: 0,
            dim: 3,
        };
        engine.put_vector_meta(1, &meta).unwrap();

        assert!(engine.delete_vector_meta(1).unwrap());
        assert!(!engine.delete_vector_meta(1).unwrap());
        assert!(engine.get_vector_meta(1).unwrap().is_none());
    }

    #[test]
    fn put_then_get_round_trips_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        let value = serde_json::json!({ "title": "Rust Vector DB", "tags": ["rust", "db"] });

        engine.put_metadata(1, &value).unwrap();

        assert_eq!(engine.get_metadata(1).unwrap(), Some(value));
    }

    #[test]
    fn append_log_entries_replay_back_in_sequence_order() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        let op1 = Operation {
            op_type: OperationType::Upsert,
            vector_id: 1,
            timestamp: 100,
        };
        let op2 = Operation {
            op_type: OperationType::Delete,
            vector_id: 1,
            timestamp: 200,
        };

        engine.append_log(0, &op1).unwrap();
        engine.append_log(1, &op2).unwrap();

        assert_eq!(engine.replay_log().unwrap(), vec![(0, op1), (1, op2)]);
    }
}
