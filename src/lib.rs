//! Embedded vector search with transactional LMDB recovery and mmap HNSW graphs.
//!
//! The alpha serializes public operations, supports atomic batches, and refreshes
//! HNSW incrementally after clean mutations.
//! See the repository README for durability assumptions and format compatibility.
//!
//! ```
//! # #[cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use vdb::{Metric, VectorDb};
//! let temporary = tempfile::tempdir()?;
//! let db = VectorDb::open(temporary.path())?;
//! let docs = db.create_collection("docs", 3, Metric::Cosine)?;
//! let id = docs.upsert_vector(&[1.0, 0.0, 0.0], serde_json::json!({"tags": ["rust"]}))?;
//! let neighbors = docs.query(&[1.0, 0.0, 0.0], 10).execute()?;
//! assert_eq!(neighbors[0].id, id);
//! # Ok(())
//! # }
//! # fn main() {
//! # #[cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]
//! # example().unwrap();
//! # }
//! ```
#![warn(missing_docs)]

pub mod error;

#[cfg(feature = "storage")]
mod checksum;

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
pub use db::{Collection, DbOptions, QueryBuilder, ScoredVector, VectorDb};
#[cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]
pub use metric::Metric;
#[cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]
pub use query::Filter;
