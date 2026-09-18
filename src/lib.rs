//! `vdb`: an embedded, crash-safe, Rust-native vector database.
//!
//! LMDB (via `heed`, see `docs/adr/0001-use-lmdb-as-kv-substrate.md`) holds
//! structured metadata and the WAL; per-collection files hold vector
//! payloads (`.vectors`) and the HNSW index (`.index`), both mmap'd for
//! zero-copy reads. See `docs/arch.md` for the full layered architecture.
#![warn(missing_docs)]

pub mod error;

/// KV substrate (LMDB via `heed`) — see `docs/adr/0001-use-lmdb-as-kv-substrate.md`.
#[cfg(feature = "storage")]
pub mod kv;

/// Per-collection mmap'd `.vectors` file — see `docs/storage_layout.md`.
#[cfg(feature = "storage")]
pub mod vector;

/// Per-collection mmap'd HNSW `.index` file — see `docs/adr/0002-hnsw-on-disk-layout.md`.
#[cfg(feature = "index-hnsw")]
pub mod index;

/// Distance/similarity metrics used for vector search and scoring.
#[cfg(feature = "metrics")]
pub mod metric;

/// Query builder and metadata filter DSL.
#[cfg(feature = "serde-query")]
pub mod query;

/// `VectorDb`/`Collection`/`QueryBuilder` — the public API. Needs
/// `storage` (KvEngine + VectorFile), `metrics` (Metric), and
/// `serde-query` (Filter) all together.
#[cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]
pub mod db;

pub use error::{Error, Result};

#[cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]
pub use db::{Collection, QueryBuilder, ScoredVector, VectorDb};
#[cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]
pub use metric::Metric;
#[cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]
pub use query::Filter;
