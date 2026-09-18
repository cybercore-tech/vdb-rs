# PROJECT_STATE.md — vdb-rs

# Identity

**Project:** vdb-rs  
**Version:** 0.1.0  
**Branch:** main  
**Edition:** 2021  
**MSRV:** 1.70 (declared in `Cargo.toml`, not yet CI-verified against an actual 1.70 toolchain)  

# Current Phase

Phase 1 — Core architecture: scaffolding complete and green

# Current Milestone

Phase 1 scaffold: `KvEngine` (LMDB/heed), `.vectors`/`.index` mmap readers,
`Metric`, `Filter` — all placeholder-but-real, unit-tested, full gate green

# Last Completed Milestone

Phase 1 scaffold

# Last Known Green Commit

```text
see `git log -1 --oneline` in this repository
```

# Current Work

None in progress — Phase 1 acceptance criteria met, awaiting direction on
Phase 2 (storage layout: real `vectors`/`metadata`/`logs` LMDB tables).

# Status

```text
GREEN — full validation gate passes (see Validation Status below)
```

# Recent Completed Work

- Repository relocated from the blueprint at
  `/home/raven/Projects/agent-blueprints/rust-vdb-blueprint` to its real
  location: `/home/raven/Devspace/rust/vdb-rs`.
- KV substrate decision changed from redb to **LMDB (via `heed`)** —
  redb rejected specifically for being pre-1.0/format-unsettled; see
  `docs/adr/0001-use-lmdb-as-kv-substrate.md`. All docs
  (`PROJECT_SPEC.md`, `AGENTS.md`, `PROJECT_STATE.md`,
  `PROJECT AGENT BLUEPRINT.md`, `docs/arch.md`, `docs/storage_layout.md`,
  `docs/query_pipeline.md`, `docs/adr/0002-*.md`) updated to match.
- `cargo init --lib --name vdb` run; real `Cargo.toml` with the full
  `[features]` table written.
- `src/error.rs` — `thiserror`-based `Error`/`Result`.
- `src/kv.rs` — `KvEngine` wrapping a real `heed::Env`; `collections`
  named-database wired up (get/put), unit-tested against a real LMDB
  environment in a tempdir.
- `src/vector.rs` — `VectorFile`: opens and mmaps a real `.vectors` file,
  validates magic bytes, reads `dim`/`count` from the header.
- `src/index.rs` — `IndexFile`: same pattern for `.index` (magic, `m`,
  `ef_construction`, `dim`).
- `src/metric.rs` — `Metric::{Cosine,Euclidean,DotProduct}::distance`,
  real math, unit-tested.
- `src/query.rs` — `Filter` (single `field CONTAINS value` predicate),
  serde round-trippable, unit-tested.
- **Real bug found and fixed during scaffolding**: `storage` feature
  compiled in isolation only by accident of dependency ordering — `kv.rs`
  needs `serde_json::Value` for heed's `SerdeJson` codec, but `serde_json`
  was only wired to the `serde-query` feature. `cargo check -p vdb
  --no-default-features --features storage` failed until `storage`'s
  feature declaration in `Cargo.toml` was corrected to depend on
  `dep:serde`/`dep:serde_json` directly. Feature Matrix in
  `PROJECT_SPEC.md` updated to record this as a real, verified
  cross-feature dependency, not an inferred one.

# Important Invariants

- LMDB is the sole KV substrate.
- Vector + index files are mmap'd per collection.
- Vector IDs are monotonically increasing u64 per collection.
- WAL is replayed on open if dirty flag is set.
- File formats are versioned; breaking changes need ADRs.

# Protected Behavior

- On-disk `.vectors` file format (magic `VDB1`)
- On-disk `.index` file (HNSW) layout (magic `IDX1`)
- LMDB named-database layout (`collections`, and `vectors`/`metadata`/`logs` once Phase 2 adds them)
- `KvEngine` trait/struct shape
- Public `VectorDb` / `Collection` / `QueryBuilder` API (not yet built — `KvEngine`/`VectorFile`/`IndexFile` are the only public surfaces so far)

# Feature State

| Feature | Status | Isolation Tested? | Notes |
|---|---|---:|---|
| `storage` | scaffolded (KvEngine collections table only) | yes | `heed`/LMDB env open + get/put, unit-tested |
| `index-hnsw` | scaffolded (header-only IndexFile) | yes | no real graph traversal yet — Phase 4 |
| `metrics` | implemented | yes | cosine/euclidean/dot, unit-tested |
| `serde-query` | scaffolded (single predicate only) | yes | full DSL (AND/OR) is Phase 5 |
| `quantization` | not implemented | no | planned for later phase |
| `index-ivf` | not implemented | no | planned for later phase (alternative to index-hnsw) |
| `async` | not implemented | no | planned for later phase |

# Validation Status

| Gate | Status | Last Command/Notes |
|---|---|---|
| `cargo fmt --all --check` | PASS | 2026-09-17 |
| `git diff --check` | N/A | nothing staged with whitespace issues at scaffold time |
| all-target/all-feature check | PASS | `cargo check --workspace --all-targets --all-features` |
| clippy `-D warnings` | PASS | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| workspace tests | PASS | 11/11, `cargo test --workspace --all-features --lib --tests` |
| doc tests | PASS | 0 doctests present yet (no `///` code examples written) |
| rustdoc `-D warnings` | PASS | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features` |
| no-default-features | PASS | `cargo check --workspace --no-default-features` |
| critical feature isolation | PASS | all 7 combinations in `PROJECT_SPEC.md` + `./scripts/validate-features.sh` green |
| package list | UNKNOWN | not yet run — no `repository`/version tag pushed |
| cargo package | UNKNOWN | not yet run |
| GitHub Actions | UNKNOWN | no remote/CI configured yet |

# Current Failure

```text
None.
```

# Packaging State

```text
Publishable crates:
- vdb

Last package result:
Not yet packaged (no remote repository yet)

Known package issues:
None known
```

# Cargo.lock State

```text
Tracked by repository: yes (created by this session's cargo commands)
Current status: present, not yet committed
Reason if modified: initial dependency resolution
```

# CI State

```text
Latest run URL:
<n/a>

Run ID:
<n/a>

Status:
Not set up — no GitHub remote yet
```

# Current Git State

```text
Expected worktree:
Phase 1 scaffold: Cargo.toml, src/*.rs, Cargo.lock, plus the blueprint docs
copied in from rust-vdb-blueprint.

Known uncommitted changes:
see `git status --short` in this repository
```

# Recent Significant Commits

```text
See `git log --oneline` in this repository.
```

# Current Acceptance Criteria

- [x] `Cargo.toml` with LMDB (`heed`), `memmap2`, `serde`, `thiserror`
- [x] `src/lib.rs` compiles with placeholder modules
- [x] `docs/adr/` has ADR-0001 (LMDB) and ADR-0002 (HNSW layout)
- [x] `scripts/validate.sh` and `scripts/validate-features.sh` present and green

# Next Intended Work

Phase 2 — Storage layout: add the real `vectors`/`metadata`/`logs` LMDB
named databases to `KvEngine` (only `collections` exists so far), define
`CollectionConfig`/`VectorMeta`/`Operation` types, wire `VectorFile`
writing (currently read-only).

# Recommended Next Command

```bash
cd /home/raven/Devspace/rust/vdb-rs
./scripts/validate.sh
```

Then start Phase 2: add `vectors`/`metadata`/`logs` tables to `src/kv.rs`.

# Notes for Next Agent

- LMDB is the KV substrate; do NOT introduce SQLite/sled/redb.
- The storage layout (collections/vectors/metadata/logs) must match the blueprint — only `collections` exists so far.
- ADR-0001 (use LMDB via heed) and ADR-0002 (HNSW on-disk layout) are accepted/proposed respectively — read both before touching `src/kv.rs`, `src/vector.rs`, or `src/index.rs`.
- Feature flags: `storage`/`index-hnsw`/`metrics`/`serde-query` are the current, isolation-tested, default-on set (see `PROJECT_SPEC.md`'s Feature Matrix, now matching the real `Cargo.toml`). `quantization`/`index-ivf`/`async` are later-phase, off by default, not yet isolation-tested.
- `storage`'s `Cargo.toml` entry depends on `serde`/`serde_json` directly (not only via `serde-query`) — found the hard way via `cargo check --no-default-features --features storage` failing. Don't "clean up" that dependency edge without re-running the feature-isolation gate.
