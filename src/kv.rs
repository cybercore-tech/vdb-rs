//! `KvEngine`: the LMDB (via `heed`) backed KV substrate.
//!
//! See `docs/adr/0001-use-lmdb-as-kv-substrate.md` for why LMDB was chosen,
//! and `docs/storage_layout.md` for the named-database (table) layout this
//! wraps: `collections`, `vectors`, `metadata`, `logs`.

use std::path::Path;

use heed::types::{SerdeJson, Str};
use heed::{Database, Env, EnvOpenOptions};

use crate::error::Result;

/// One open LMDB environment, holding all of this database's named
/// sub-databases.
///
/// Phase 1 scaffolding: only the `collections` table is wired up so far.
/// `vectors`/`metadata`/`logs` land in Phase 2 (see `PROJECT_SPEC.md`'s
/// phase list).
pub struct KvEngine {
    #[allow(dead_code)]
    env: Env,
    collections: Database<Str, SerdeJson<serde_json::Value>>,
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
                .map_size(1024 * 1024 * 1024) // 1 GiB; revisit growth policy in Phase 2.
                .max_dbs(4) // collections, vectors, metadata, logs
                .open(path)?
        };

        let mut wtxn = env.write_txn()?;
        let collections = env.create_database(&mut wtxn, Some("collections"))?;
        wtxn.commit()?;

        Ok(Self { env, collections })
    }

    /// Fetch a collection's stored config by name, if it exists.
    pub fn get_collection(&self, name: &str) -> Result<Option<serde_json::Value>> {
        let rtxn = self.env.read_txn()?;
        Ok(self.collections.get(&rtxn, name)?)
    }

    /// Insert or replace a collection's stored config.
    pub fn put_collection(&self, name: &str, config: &serde_json::Value) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.collections.put(&mut wtxn, name, config)?;
        wtxn.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn open_creates_the_collections_database() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        assert!(engine.get_collection("docs").unwrap().is_none());
    }

    #[test]
    fn put_then_get_round_trips_a_collection_config() {
        let dir = tempfile::tempdir().unwrap();
        let engine = KvEngine::open(dir.path()).unwrap();
        let config = json!({ "dim": 768, "metric": "cosine" });

        engine.put_collection("docs", &config).unwrap();

        assert_eq!(engine.get_collection("docs").unwrap(), Some(config));
    }
}
