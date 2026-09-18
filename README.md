# 🧬 vdb-rs

`Rust` · `LMDB` · `mmap` · `HNSW` · embedded

[![CI](https://github.com/darkstardevx/vdb-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/darkstardevx/vdb-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#-license)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](#-msrv)

**An embedded, crash-safe, Rust-native vector database — no server, no
Python bindings, just a library you link into your own binary.**

`LMDB` (via [`heed`](https://docs.rs/heed)) holds structured metadata and
the WAL. Vector payloads live in per-collection, mmap'd files. The whole
thing is invisible behind a small typed API — `VectorDb::open`,
`create_collection`, `upsert_vector`, `query(...).filter(...)` — see
[what you can do with it today](#-what-you-can-actually-do-with-it-today).

> ⚠️ **Pre-alpha. Not on crates.io.** Phases 1–3 and 5 of 8 are done —
> `VectorDb`/`Collection`/`QueryBuilder` are real and tested, including
> filtered k-NN queries. What's missing: a real ANN index (queries are
> brute-force linear scan today, not HNSW — see [Status](#-status)) and
> WAL crash-recovery replay. Built Phase 5 ahead of Phase 4's HNSW
> graph work on purpose — see [Roadmap](#️-roadmap) for why.

---

## 🤔 Why this exists

There isn't a direct Rust equivalent to what this aims to be:

| Existing option | The gap |
|---|---|
| [`qdrant`](https://qdrant.tech) | Server-first; embedded mode is a secondary use case, not the design center |
| [`lancedb`](https://lancedb.com) | Columnar-format-first (Arrow/Lance) — a different shape of tool |
| [`usearch`](https://docs.rs/usearch), [`hnsw_rs`](https://docs.rs/hnsw_rs), [`instant-distance`](https://docs.rs/instant-distance) | Just the ANN algorithm — no persistence, no crash safety, no WAL |
| SQLite + [`sqlite-vec`](https://github.com/asg017/sqlite-vec) | Real, but a C extension, not Rust-native |

The bet here: embedded + ACID metadata + mmap'd vectors + real crash
safety, in one small Rust crate, is a real gap — not "sqlite-vec but
worse," a different design point entirely.

## 🗄️ Why LMDB, not redb/rocksdb/sled

Originally spec'd with [redb](https://github.com/cberner/redb). Switched
to **LMDB via `heed`** during Phase 1 scaffolding — full reasoning in
[`docs/adr/0001-use-lmdb-as-kv-substrate.md`](docs/adr/0001-use-lmdb-as-kv-substrate.md),
short version:

- **Format-frozen since ~2011** — redb is pre-1.0 with an unsettled
  on-disk format, too risky a foundation for a project making its own
  file-format stability promises.
- **mmap-native** — reads are zero-copy straight off the page cache,
  matching the `.vectors`/`.index` design already in place, instead of a
  second, different storage paradigm sitting next to it.
- **Battle-tested at real scale** in unglamorous production systems
  (OpenLDAP, Meilisearch) for a long time.

Traded away: LMDB is a C library (FFI via `heed`), not pure Rust like
redb — accepted deliberately for the format/API maturity.

## 🏗️ Architecture

```
┌───────────────────────────────────────────┐
│  Public API — VectorDb / Collection /      │
│  QueryBuilder (src/db.rs)                  │
│  create_collection, upsert_vector,         │
│  query().filter().execute()                │
├───────────────────────────────────────────┤
│           KvEngine (src/kv.rs)             │
│   LMDB via heed — collections/vectors/     │
│   metadata/logs named databases            │
├───────────────────────────────────────────┤
│  Per-collection mmap files                 │
│   • collection.vectors  (src/vector.rs)    │
│   • collection.index    (src/index.rs,     │
│     header-only — no real HNSW graph yet)  │
└───────────────────────────────────────────┘
```

## ✅ Status

What's real and tested right now (40 tests: 38 unit + 2 integration):

- [x] **`VectorDb` / `Collection` / `QueryBuilder`** (`src/db.rs`) —
      create/open/list/delete collections, `upsert_vector`/
      `delete_vector`, filtered k-NN `query()`
- [x] **`KvEngine`** (`src/kv.rs`) — all four LMDB tables (`collections`,
      `vectors`, `metadata`, `logs`), typed `CollectionConfig`/
      `VectorMeta`/`Operation`, atomic per-collection ID allocation
- [x] **`VectorFile` / `VectorFileWriter`** (`src/vector.rs`) — real
      `.vectors` read/write, 32-byte-aligned per the spec
- [x] **`Metric`** (`src/metric.rs`) — cosine, euclidean, dot product,
      correct ranking direction for each (dot product is a similarity —
      higher is better — not a distance; genuinely easy to get backwards)
- [x] **`Filter`** (`src/query.rs`) — single `field CONTAINS value`
      predicate; full DSL (`AND`/`OR`) is later
- [ ] **`IndexFile`** (`src/index.rs`) — `.index` header read only (magic,
      `M`, `ef_construction`, `dim`); no real HNSW graph data yet, so
      `QueryBuilder::execute` is brute-force linear scan, not real ANN

What doesn't exist yet:

- [ ] Real HNSW graph traversal (Phase 4) — this is *the* thing that
      makes queries scale; brute-force scan works but is O(n) per query
- [ ] Full filter DSL (`AND`/`OR`, comparison operators, not just one
      `CONTAINS` predicate)
- [ ] WAL crash-recovery replay — `KvEngine::replay_log` returns recorded
      entries but doesn't re-apply them yet (Phase 6)
- [ ] `quantization` / `index-ivf` / `async` — later-phase, off by
      default, not isolation-tested

## 🔧 What you can actually do with it today

The real public API — verified as a real, passing integration test, not
illustrative pseudocode (see
[`tests/vector_db_api.rs`](tests/vector_db_api.rs)):

```rust,ignore
use vdb::{Filter, Metric, VectorDb};

let dir = tempfile::tempdir()?;
let db = VectorDb::open(dir.path())?;

let docs = db.create_collection("docs", 3, Metric::Cosine)?;

docs.upsert_vector(&[1.0, 0.0, 0.0], serde_json::json!({ "tags": ["rust"] }))?;
docs.upsert_vector(&[0.0, 1.0, 0.0], serde_json::json!({ "tags": ["python"] }))?;

let results = docs
    .query(&[1.0, 0.0, 0.0], 10)
    .filter(Filter { field: "tags".into(), contains: "rust".into() })
    .execute()?;
```

The lower-level primitives (`KvEngine`, `VectorFile`/`VectorFileWriter`
directly) are still there and usable too — see
[`tests/basic_round_trip.rs`](tests/basic_round_trip.rs) — but the API
above is what you actually want.

## 🚩 Feature flags

| Feature | Default | Requires | Purpose |
|---|---:|---|---|
| `storage` | on | `heed`, `memmap2`, `serde`, `serde_json`, `byteorder` | LMDB KV substrate + `.vectors` file read/write |
| `index-hnsw` | on | `storage` | mmap'd `.index` file (header only so far) |
| `metrics` | on | — | `Metric`: cosine, euclidean, dot product |
| `serde-query` | on | `serde`, `serde_json` | `Filter` (de)serialization |
| `quantization` | off | `half` | f16/PQ vector compression — later phase |
| `index-ivf` | off | — | IVF index backend, alternative to HNSW — later phase |
| `async` | off | `tokio` | Async API — later phase |

<details>
<summary>Every project-critical feature combination is isolation-tested</summary>

```bash
cargo check --workspace --no-default-features
cargo check -p vdb --no-default-features --features storage
cargo check -p vdb --no-default-features --features index-hnsw
cargo check -p vdb --no-default-features --features metrics
cargo check -p vdb --no-default-features --features serde-query
cargo check -p vdb --no-default-features --features storage,index-hnsw
cargo check -p vdb --no-default-features --features storage,index-hnsw,metrics
cargo check -p vdb --no-default-features --features storage,serde-query
cargo check -p vdb --no-default-features --features storage,metrics,serde-query
```

Both `storage`'s `serde`/`serde_json` and `byteorder` dependencies were
found to be *required*, not optional, by actually running these checks in
isolation — see the Phase 1/2 commit messages in `git log`.

</details>

## 📦 Installation

Not published to crates.io yet. Depend on it by git:

```toml
[dependencies]
vdb = { git = "https://github.com/darkstardevx/vdb-rs" }
```

## 🎯 MSRV

`1.88`, declared in `Cargo.toml` — **actually verified**, not just
asserted: `cargo +1.88.0 check`/`test` (real, pinned toolchain, not
`clippy::incompatible_msrv`'s static analysis) both pass.

Originally declared `1.70` with edition `2021` at Phase 1, based on the
blueprint's own placeholder ("targeting 1.70+"), never built against a
real toolchain. A `cargo +1.70.0 check` turned up two real problems
static analysis alone wouldn't catch: a proc-macro dependency (`quote`)
needing 1.71+, and — more fundamentally — a transitive dependency
(`tempfile` → `getrandom`) whose own `Cargo.toml` declares `edition =
"2024"`, which a pre-2024-edition `cargo` can't even parse. `1.70` was
never achievable with this dependency tree; `1.88` (matching the
`darkstardevx/gateflow` precedent, which hit the same
`unsigned_is_multiple_of`-class stabilization issue) is the first
verified-working pin. Re-verify before trusting this:

```bash
rustup toolchain install 1.88.0
cargo +1.88.0 check --workspace --all-targets --all-features
cargo +1.88.0 test --workspace --all-features --lib --tests
```

## 🧪 Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --lib --tests
cargo test --doc --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
./scripts/validate-features.sh
```

Or all at once:

```bash
./scripts/validate.sh
```

## 🗺️ Roadmap

```text
Phase 1 — Core architecture (KvEngine, LMDB layer, Cargo scaffolding)           ✅
Phase 2 — Storage layout (collections/vectors/metadata/logs tables)            ✅
Phase 3 — Vector file + mmap layer (.vectors file read/write)                  ✅
Phase 5 — Collection/query API + filter DSL (VectorDb/Collection/QueryBuilder) ✅
Phase 4 — HNSW index + mmap traversal (.index file)                            ⏳ next
Phase 6 — WAL / crash recovery replay
Phase 7 — Examples + integration tests
Phase 8 — Release hardening (MSRV, packaging, CI)
```

Built out of the blueprint's original order on purpose: Phase 5 (the
actual usable `VectorDb`/`Collection`/`QueryBuilder` API) landed before
Phase 4 (real HNSW). Reasoning: the API can ship correctly today with
brute-force linear-scan search using the `Metric` primitive that already
existed — real ANN search slots in behind the same interface later
without a redesign. Doing the API first also forced settling
vector-ID allocation (now: engine-allocated, monotonic per collection)
before Phase 4 added even more surface area depending on that scheme.

## 📄 License

Dual-licensed under either of

- MIT license
- Apache License, Version 2.0

at your option.
