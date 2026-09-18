//! `KvEngine`: the LMDB (via `heed`) backed KV substrate.
//!
//! See `docs/adr/0001-use-lmdb-as-kv-substrate.md` for why LMDB was chosen,
//! and `docs/storage_layout.md` for the named-database (table) layout this
//! wraps: `collections`, `vectors`, `metadata`, `logs`.

use std::path::Path;

use byteorder::NativeEndian;
use heed::types::{SerdeJson, Str, U64};
use heed::{Database, Env, EnvOpenOptions};
use serde::{Deserialize, Serialize};

use crate::error::Result;

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

/// The kind of change a WAL [`Operation`] records.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationType {
    /// A vector was inserted or replaced.
    Upsert,
    /// A vector was deleted.
    Delete,
}

/// One WAL entry, stored in the `logs` table, keyed by a caller-supplied
/// monotonic sequence number. Replayed on `open` if a previous session
/// left a dirty flag set — replay itself is Phase 6, not yet implemented;
/// [`KvEngine::replay_log`] only returns the recorded entries so far.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Operation {
    /// What kind of change this entry records.
    pub op_type: OperationType,
    /// The vector ID this operation applies to.
    pub vector_id: u64,
    /// Unix timestamp (seconds) the operation was logged at.
    pub timestamp: u64,
}

/// One open LMDB environment, holding all four of `vdb`'s named
/// sub-databases (see `docs/storage_layout.md`).
pub struct KvEngine {
    #[allow(dead_code)]
    env: Env,
    collections: Database<Str, SerdeJson<CollectionConfig>>,
    vectors: Database<U64<NativeEndian>, SerdeJson<VectorMeta>>,
    metadata: Database<U64<NativeEndian>, SerdeJson<serde_json::Value>>,
    logs: Database<U64<NativeEndian>, SerdeJson<Operation>>,
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
    /// enforced at the `VectorDb` layer once that lands.
    pub fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path)?;

        // Safety: see the invariants documented above.
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(1024 * 1024 * 1024) // 1 GiB; revisit growth policy later.
                .max_dbs(4) // collections, vectors, metadata, logs
                .open(path)?
        };

        let mut wtxn = env.write_txn()?;
        let collections = env.create_database(&mut wtxn, Some("collections"))?;
        let vectors = env.create_database(&mut wtxn, Some("vectors"))?;
        let metadata = env.create_database(&mut wtxn, Some("metadata"))?;
        let logs = env.create_database(&mut wtxn, Some("logs"))?;
        wtxn.commit()?;

        Ok(Self {
            env,
            collections,
            vectors,
            metadata,
            logs,
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

    /// Append one WAL entry, keyed by a caller-supplied monotonic sequence number.
    pub fn append_log(&self, seq: u64, op: &Operation) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.logs.put(&mut wtxn, &seq, op)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Return every logged WAL entry in sequence order.
    ///
    /// Actually replaying them into the mmap files on crash recovery is
    /// Phase 6 — this just hands back what's recorded.
    pub fn replay_log(&self) -> Result<Vec<(u64, Operation)>> {
        let rtxn = self.env.read_txn()?;
        let entries: Result<Vec<(u64, Operation)>> =
            self.logs.iter(&rtxn)?.map(|entry| Ok(entry?)).collect();
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_all_four_tables() {
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
        };
        assert_eq!(config.metric(), Some(crate::metric::Metric::Euclidean));

        let unknown = CollectionConfig {
            dim: 3,
            metric: "manhattan".into(),
        };
        assert_eq!(unknown.metric(), None);
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
