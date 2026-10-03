# vdb-rs

[![CI](https://github.com/cybercore-tech/vdb-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/cybercore-tech/vdb-rs/actions/workflows/ci.yml)
[![Security](https://github.com/cybercore-tech/vdb-rs/actions/workflows/security.yml/badge.svg)](https://github.com/cybercore-tech/vdb-rs/actions/workflows/security.yml)

An embedded Rust vector database with LMDB transactions, durable recovery,
checksummed mmap vector files, and a persisted HNSW graph. No server required.

**Alpha: `0.1.0-alpha.1`, Rust 1.88+, Linux and macOS on local filesystems.**
The [signed GitHub alpha release](https://github.com/cybercore-tech/vdb-rs/releases/tag/v0.1.0-alpha.1)
is available, and [`cybercore-vdb`](https://crates.io/crates/cybercore-vdb) is published
on crates.io. Install it as `vdb` using the dependency alias below.
The previous pre-alpha on-disk format is incompatible;
export old databases with their original revision before upgrading. This release
rejects populated legacy databases rather than interpreting them incorrectly.

```toml
[dependencies]
vdb = { package = "cybercore-vdb", version = "=0.1.0-alpha.1" }
serde_json = "1"
tempfile = "3" # only needed for the temporary-directory example
```

```rust
use vdb::{Metric, VectorDb};
fn main() -> Result<(), Box<dyn std::error::Error>> {
let temporary = tempfile::tempdir()?;
let db = VectorDb::open(temporary.path())?;
let docs = db.create_collection("docs", 3, Metric::Cosine)?;
let id = docs.upsert_vector(&[1.0, 0.0, 0.0], serde_json::json!({"tags": ["rust"]}))?;
let neighbors = docs.query(&[1.0, 0.0, 0.0], 10).ef_search(128).execute()?;
assert_eq!(neighbors[0].id, id);
Ok(())
}
```

```rust
use vdb::{Metric, VectorDb};
fn main() -> Result<(), Box<dyn std::error::Error>> {
let temporary = tempfile::tempdir()?;
let db = VectorDb::open(temporary.path())?;
let docs = db.create_collection("docs", 3, Metric::Cosine)?;
let ids = docs.upsert_batch(&[
    (vec![1.0, 0.0, 0.0], serde_json::json!({"title": "first"})),
    (vec![0.0, 1.0, 0.0], serde_json::json!({"title": "second"})),
])?;
assert_eq!(ids.len(), 2);
Ok(())
}
```

Run `cargo run --example basic` for insertion, metadata filtering and a real
close/reopen. The library-level rustdoc example is tested too.

## Current behavior

- Database-wide monotonic vector IDs prevent cross-collection overwrites.
- Collection generations reject stale handles after delete/recreate.
- Deletion checks ownership and collection deletion removes all associated records.
- A directory lock excludes other engines/processes. Cloned database and collection
  handles share a mutex; reads and writes have coherent, serialized snapshots.
- Single inserts and `upsert_batch` atomically commit vector payloads, locations, metadata, ID allocation,
  collection revision and a recovery marker to LMDB. The vector file is then
  synchronized and the marker checkpointed. Reopening replays pending work.
- A batch returns IDs in input order, validates every row before mutation, and uses
  one logical transaction, one vector-file sync and one checkpoint. Empty batches
  are no-ops. LMDB capacity errors roll back the entire batch.
- Interrupted creation, single/batch insertion, compaction and collection deletion are recoverable.
  Process-kill tests cover the boundary before and after vector file synchronization.
- Vectors are finite `f32`s with dimension 1–65,536. Names are ASCII letters/digits,
  `_` and `-`, starting with a letter or digit (maximum 128 bytes).
- Cosine, Euclidean and dot-product search have the appropriate ranking direction.
  A zero-norm vector has cosine distance 1, so it is not treated as a perfect match.
- Unfiltered queries use a multi-layer HNSW graph with mmap traversal. `exact()`
  selects the reference scan. `ef_search()` sets the ANN candidate budget.
- `Filter { field, contains }` matches a string in a metadata array. Filtered queries
  use the exact path, preserving completeness even with selective filters.
- `compact()` reclaims deleted vector-file space and invalidates the derived index.

## Alpha limits

- `upsert_vector` is a legacy name: it **always inserts** a new vector. Updating an
  existing ID is not implemented.
- HNSW uses M=16, ef_construction=128 and deterministic geometric levels. The first
  unfiltered query builds/publishes a graph. After clean mutations, the next ANN
  query restores the previous topology and constructs only new nodes, including
  after reopening. Deleted nodes remain as routing links but cannot be results;
  `compact()` removes them. Recovery and corrupt/incompatible indexes rebuild.
- Incremental refresh still scans collection locations, copies graph vectors into
  RAM, and rewrites the full snapshot. Deletes widen the candidate beam, so heavy
  deletion increases query cost until compaction. No parallel reads yet.
- Size batches to fit available memory and the LMDB map. Writes remain serialized.
- Durable recovery keeps an additional copy of vector values in LMDB. The map defaults
  to 1 GiB; configure `DbOptions { map_size }` before opening for larger databases.
  Automatic resizing and vector quantization are not implemented.
- An I/O error **after** the LMDB commit can mean a write committed even though the
  call returned an error. The next operation or reopening repairs pending work.
  Do not blindly retry an insertion after such an error.
- File checksums detect accidental damage, not malicious tampering. A corrupt derived
  index is rebuilt; a corrupt clean vector file returns an error. `compact()` can
  rebuild vector files from intact LMDB payloads. LMDB corruption is not repaired.
- Recovery assumes a local filesystem that honors LMDB commits, file synchronization
  and atomic rename. External file mutation and network filesystems are unsupported.
- The low-level `kv`, `vector` and `index` modules are expert primitives. Direct
  mutations bypass the public API's revision/recovery protocol.
- `async`, `quantization` and `index-ivf` remain reserved flags with no implementation.
  Full filter expressions are future work.

## Benchmarks

```bash
cargo run --release --example benchmark -- 1000 32 50 128
# Individual inserts for comparison: use batch size 1.
```

This deterministic harness measures synchronized ingestion, initial graph build,
warmed ANN and exact p50/p95 latency, recall@10, mixed write/delete/query rounds,
reopen latency, a full rebuild comparison, file sizes and Linux peak RSS.
See [measured results](docs/benchmarks.md). Small synthetic results do not establish
production-scale capacity or throughput.

## Local-note semantic search

A [working evaluation and search example](docs/semantic-search.md) uses local
technical notes, real CPU embeddings, source/snippet metadata and a BM25 keyword
baseline. The local pilot covered 122 nonempty documents and 426 chunks: sampled
ANN recall@10 was 100%, and an expected source appeared in the top five for 10 of
11 questions. BM25 ranked expected answers higher overall (MRR@5 0.720 versus
0.568); hybrid ranking and Cyberdesk integration remain follow-up work.

It verifies batch ingestion, incremental indexing, whole-note deletion, reindexing
and reopening. Private notes stay local; CI uses frozen public embeddings.
After preparing and evaluating your corpus using the guide:

```sh
just semantic-search       # readable titles, sources, distances and snippets
just semantic-search-json  # JSON for automation
just semantic-report       # saved evaluation; also writes a Markdown copy
```

Default artifacts are under `dist/semantic-search/`: `report.json`, `report.md`
and `db/`. Search does not generate an evaluation report. Use
`realpath dist/semantic-search/report.md` to get its full local path. See the guide
for custom directories and embedding another question against the cached model.

## Development and release

```bash
./scripts/validate.sh
./scripts/validate-features.sh
cargo run --example basic
./scripts/validate-package.sh
# All release gates:
./scripts/release-gates release
```

CI covers Linux/macOS, default/all features, formatting, Clippy, strict rustdoc,
feature isolation, examples, offline semantic-search regressions, Python helper
checks, packaging and the locked Rust 1.88 build. Weekly
security checks audit dependencies and check licenses/sources; Dependabot maintains
Cargo and Actions dependencies. Version tags run release gates and produce a
**draft** GitHub release containing the verified `.crate`, SHA256 manifest and an
OIDC-backed cosign signature. No automatic crates.io publication is configured.

[Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) ·
[Architecture](docs/arch.md) · [Recovery protocol](docs/recovery.md) ·
[Storage layout](docs/storage_layout.md) · [Roadmap](docs/roadmap.md)

Dual licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
