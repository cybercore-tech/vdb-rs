# PROJECT_SPEC.md — vdb-rs

# Project Identity

**Project Name:** vdb-rs  
**Current Version:** 0.1.0  
**Repository:** `<URL/path>`  
**Rust Edition:** 2021  
**MSRV:** TBD (targeting 1.70+)  
**Workspace:** Single crate  

## Workspace Members

```text
vdb
```

## Package Roles

| Package | Type | Purpose | Publish? |
|---|---|---|---|
| `vdb` | library | Embedded persistent vector DB | yes |

# Vision

## One-Sentence Description

An embedded, crash-safe, Rust-native vector database using LMDB for structured metadata and WAL, with mmap'd files for vector payloads and HNSW index.

## Long-Term Capabilities

- Single-binary embedded usage (no server)
- ACID transactions over metadata + WAL
- Zero-copy vector reads via mmap
- HNSW ANN search (optional IVF/Flat backends)
- Metadata filtering DSL
- Automatic crash recovery
- Optional quantization + async API

# Core Requirements

- [x] LMDB KV engine backing (collections, vectors, metadata, logs tables)
- [x] Per-collection mmap vector data file (.vectors)
- [x] Per-collection mmap HNSW index file (.index)
- [x] KvEngine trait abstraction
- [x] Collection CRUD
- [x] upsert_vector + delete_vector
- [x] Query builder with filter DSL
- [x] WAL-based crash recovery
- [x] Metadata stored as JSON/MsgPack blobs
- [x] Compile/runtime validation (dims, duplicates, filter syntax)

# Non-Goals

- Distributed multi-node clustering
- GPU/CUDA acceleration
- HTTP/gRPC server
- SQL engine
- Non-LMDB KV backends
- Schema migration tooling
- Auto-sharding

# Architectural Principles

- Stable public APIs where practical
- Strong typing (CollectionId, VectorId, Metric)
- Useful errors with context
- Deterministic behavior
- Explicit feature boundaries
- Backwards-compatible persistence
- Tests for public behavior
- Rustdoc kept warning-free
- Packaging treated as correctness

# Feature Matrix

Matches the crate's real `Cargo.toml` `[features]` table (Phase 1 scaffold,
2026-09-17).

| Feature | Package | Default | Requires | Purpose |
|---|---|---:|---|---|
| `storage` | vdb | yes | `heed`, `memmap2`, `serde`, `serde_json` | LMDB-backed KV storage for metadata/WAL + mmap'd `.vectors` files (ADR-0001). Pulls in `serde`/`serde_json` directly (not just via `serde-query`) because `KvEngine` uses heed's `SerdeJson` codec to store collection configs — a real cross-feature dependency found during Phase 1 scaffolding, not inferred. |
| `index-hnsw` | vdb | yes | `storage` | HNSW ANN index, mmap'd `.index` files (ADR-0002) — the primary/current index backend |
| `metrics` | vdb | yes | — | Distance/similarity metric implementations (the `Metric` type — cosine, euclidean, dot product) used by vector search |
| `serde-query` | vdb | yes | `serde`, `serde_json` | (De)serialization for the query builder's filter DSL (Core Requirement) |
| `quantization` | vdb | no | `half` | f16/PQ vector compression — Long-Term Capability, not yet isolation-tested |
| `index-ivf` | vdb | no | — | IVF index backend, alternative to `index-hnsw` — Long-Term Capability, not yet isolation-tested |
| `async` | vdb | no | `tokio` | Async VectorDb API — Long-Term Capability, not yet isolation-tested |

## Required Feature-Isolation Checks

```bash
cargo check --workspace --no-default-features
cargo test --workspace --no-default-features

# Project-critical combinations (validated via scripts/validate-features.sh):
cargo check -p vdb --no-default-features --features storage
cargo check -p vdb --no-default-features --features index-hnsw
cargo check -p vdb --no-default-features --features metrics
cargo check -p vdb --no-default-features --features serde-query
cargo check -p vdb --no-default-features --features storage,index-hnsw
cargo check -p vdb --no-default-features --features storage,index-hnsw,metrics
cargo check -p vdb --no-default-features --features storage,serde-query
```

# Public API Guarantees

Protected surfaces:

```text
VectorDb::open, create_collection, list_collections, delete_collection
Collection::upsert_vector, delete_vector, query
QueryBuilder::filter, with_ef, execute
CollectionConfig, IndexConfig, Metric
QueryResult, ScoredVector
```

Compatibility expectations:

- On-disk `.vectors` and `.index` formats are versioned; breaking changes require ADR + migration.
- Public types derive `Serialize` where practical.
- `KvEngine` trait is `pub` but the LMDB impl is an internal module.

# Persistence / Serialization

Formats used:

```text
LMDB tables: collections (String -> CollectionConfig),
             vectors  (u64  -> VectorMeta),
             metadata (u64  -> JSON/MsgPack blob),
             logs     (u64  -> Operation)

.vectors file: binary header (magic "VDB1", dim u32, count u64) + contiguous aligned f32 arrays
.index file:   binary header (magic "IDX1", type, params) + HNSW nodes/levels
```

Compatibility policy:

- New optional fields get `#[serde(default)]`
- Enum variants are stable after 1.0
- File format versions are checked at open time

Canonicalization requirements:

- Vector blobs are written as native-endian f32, 32-byte aligned
- Metadata blobs use deterministic serialization (JSON canonical form or bincode v2)

# Integrity / Security Invariants

Project-specific invariants:

```text
LMDB is the sole KV substrate.
Vector payload + index files are mmap'd per collection.
Vector IDs (u64) are monotonically increasing within a collection.
WAL (logs table) is replayed on open if dirty flag is set.
On-disk file formats are versioned; breaking changes require ADR.
HNSW graph node references resolve to VectorMeta via LMDB.
Public serialization formats remain backwards compatible.
```

# CLI Contract

No CLI at MVP. May add a `vdb` CLI in Phase 7.

# Error Policy

Use `thiserror` structured error types with source chain + context.

# Dependency Policy

New dependencies require: concrete need, maintenance review, feature review,
MSRV review, platform review, security/transitive-cost review.

# Testing Strategy

Expected coverage:

```text
unit (src/ inline #[cfg(test)] modules)
integration (tests/*.rs)
regression (tests/crash_recovery.rs)
serde round-trip (types)
compatibility (file format versioning)
feature isolation (scripts/validate-features.sh)
compile tests (doctests)
```

# Full Validation Gate

```bash
cargo fmt --all --check

git diff --check

cargo check \
  --workspace \
  --all-targets \
  --all-features

cargo clippy \
  --workspace \
  --all-targets \
  --all-features \
  -- \
  -D warnings

cargo test \
  --workspace \
  --all-features \
  --lib \
  --tests

RUSTDOCFLAGS="-D warnings" \
cargo doc \
  --workspace \
  --no-deps \
  --all-features

# Doc tests explicitly (do not rely on implicit --all-targets coverage):
cargo test --doc --workspace --all-features
```

# Package Gate

For the publishable crate:

```bash
cargo package -p vdb --list
cargo package -p vdb
```

Package must succeed from clean, committed state.

# CI

Primary CI system: GitHub Actions

Workflow files:

```text
.github/workflows/ci.yml
```

CI should cover: fmt, check, clippy, tests, docs, feature isolation, package.

# Git Commit Standard

Detailed commits required:

```text
feat(scope): concise summary

Why:
<reason>

Implementation:
- detail
- detail

Tests:
- detail

Validation:
- command

Compatibility:
<notes>
```

Commit types: feat, fix, refactor, test, docs, build, ci, perf, security, chore.

# Changelog

Sections: Added, Changed, Deprecated, Removed, Fixed, Security.

# Release Checklist

```text
[ ] Full gate green
[ ] Feature isolation green
[ ] Package list inspected
[ ] cargo package green
[ ] Cargo.lock committed if tracked
[ ] CHANGELOG updated
[ ] Version updated
[ ] README/docs current
[ ] CI green
[ ] Worktree clean
[ ] Release explicitly authorized
```

# Project Phases

```text
Phase 1 — Core architecture (KvEngine trait, LMDB layer, Cargo scaffolding)
Phase 2 — Storage layout (collections/vectors/metadata/logs tables)
Phase 3 — Vector file + mmap layer (.vectors file read/write)
Phase 4 — HNSW index + mmap traversal (.index file)
Phase 5 — Collection/query API + filter DSL
Phase 6 — WAL / crash recovery replay
Phase 7 — Examples + integration tests
Phase 8 — Release hardening (MSRV, packaging, CI)
```

# Protected Areas

Do not change without ADR:

```text
.vectors file format (magic, header layout)
.index file format (HNSW node/levels layout)
LMDB table schemas (collections, vectors, metadata, logs)
KvEngine trait signature
Public VectorDb/Collection/QueryBuilder API
```

# Definition of Done

A milestone is complete only when relevant implementation, tests, docs, full
validation, feature isolation, packaging, Git state, and handoff/state updates
are complete.

# Raw Project Idea

> An embedded, persistent Rust vector database. LMDB holds all structured
> metadata (collections config, vector metadata, user metadata, WAL); mmap'd
> per-collection files hold the raw f32 payloads and the HNSW graph. Users
> call `VectorDb::open`, `create_collection`, `upsert_vector`, and
> `query(...).filter(...)` — LMDB and mmap are invisible. Optional IVF/Flat
> backends, quantization, and async later.
