# AGENT_HANDOFF.md — vdb-rs

# Identity

**Project:** vdb-rs  
**Version:** 0.1.0  
**Branch:** main  
**Current Commit:** see `git log -1 --oneline` in this repository

# Handoff Summary

Phase 1 complete: real Cargo crate scaffolded at
`/home/raven/Devspace/rust/vdb-rs` (moved here from the blueprint at
`/home/raven/Projects/agent-blueprints/rust-vdb-blueprint`, which stays as
the reusable source template). KV substrate changed from redb to **LMDB
via `heed`** — see `docs/adr/0001-use-lmdb-as-kv-substrate.md` for why.
`KvEngine`, `VectorFile`, `IndexFile`, `Metric`, `Filter` are all real,
unit-tested, placeholder-scope implementations. Full validation gate green
including all 7 feature-isolation combinations from `PROJECT_SPEC.md`.

# Current Phase

Phase 1 — Core architecture (complete)

# Current Milestone

Phase 1 scaffold

# Current Task

```text
TASK:
None open. Awaiting direction for Phase 2.

GOAL (next, not yet started):
Storage layout — add real vectors/metadata/logs LMDB tables, CollectionConfig/
VectorMeta/Operation types, wire VectorFile writing.

REQUIREMENTS:
- Follow docs/storage_layout.md's table shapes.
- Keep KvEngine's public surface additive (VectorDb doesn't exist yet, so
  no compat risk yet, but keep the pattern established in src/kv.rs).

DO NOT CHANGE:
- .vectors/.index magic bytes (VDB1/IDX1) without an ADR.
- The KV substrate (LMDB) without a new ADR.

ACCEPTANCE CRITERIA:
- Full validation gate green (see AGENTS.md's "Standard full validation gate").
- New tables unit-tested the same way `collections` is in src/kv.rs.

VALIDATION:
./scripts/validate.sh
./scripts/validate-features.sh
```

# Work Completed This Session

- Fixed a real doc-drift bug found across 4 files in the blueprint (stale
  `quantization`/`index-ivf`/`async`-only Feature Matrix that didn't match
  the actual isolation-tested feature set) — fixed in
  `PROJECT_SPEC.md`, `AGENTS.md`, `PROJECT_STATE.md`, and
  `PROJECT AGENT BLUEPRINT.md`.
- Reconsidered the KV substrate: redb → LMDB (via `heed`), specifically
  because redb is pre-1.0 with an unsettled format, which was judged too
  risky a foundation for a project making its own file-format stability
  promises. Rewrote ADR-0001, and swept every reference across
  `PROJECT_SPEC.md`, `AGENTS.md`, `PROJECT_STATE.md`,
  `PROJECT AGENT BLUEPRINT.md`, `docs/arch.md`, `docs/storage_layout.md`,
  `docs/query_pipeline.md`, `docs/adr/0002-hnsw-on-disk-layout.md`.
- Verified `heed` 0.22.1's real API (`EnvOpenOptions`, `Database`,
  `RoTxn`/`RwTxn`, `types::{Str,SerdeJson,...}`) against docs.rs before
  writing code against it — not assumed from memory.
- Scaffolded the real crate at `/home/raven/Devspace/rust/vdb-rs`:
  `cargo init --lib --name vdb`, full `Cargo.toml` with the `[features]`
  table matching `PROJECT_SPEC.md`'s Feature Matrix.
- Wrote `src/{error,kv,vector,index,metric,query}.rs` — all real,
  compiling, unit-tested code (not stub `todo!()`s): `KvEngine` opens a
  real LMDB env and round-trips a collection config; `VectorFile`/
  `IndexFile` mmap and validate real file headers; `Metric` computes real
  cosine/euclidean/dot distances; `Filter` matches real JSON metadata.
- Ran the full validation gate + all 7 project-critical feature
  combinations + `./scripts/validate-features.sh` — all green.
- **Found and fixed a real feature-isolation bug**: `--features storage`
  alone failed to compile (`serde_json` wasn't reachable without also
  enabling `serde-query`) — `kv.rs` needs it directly for heed's
  `SerdeJson` codec. Fixed in `Cargo.toml`; Feature Matrix in
  `PROJECT_SPEC.md` updated to document this as a real, gate-verified
  dependency edge.

# Files Changed

```text
Blueprint docs (redb -> LMDB sweep + feature-matrix fixes), in both
/home/raven/Projects/agent-blueprints/rust-vdb-blueprint/ (source template)
and copied into /home/raven/Devspace/rust/vdb-rs/ (the real project):
  AGENTS.md, PROJECT_SPEC.md, PROJECT_STATE.md, PROJECT AGENT BLUEPRINT.md,
  AGENT_HANDOFF.md, docs/arch.md, docs/query_pipeline.md,
  docs/storage_layout.md, docs/adr/0002-hnsw-on-disk-layout.md
  docs/adr/0001-use-redb-as-kv-substrate.md -> renamed and rewritten as
  docs/adr/0001-use-lmdb-as-kv-substrate.md

New, in /home/raven/Devspace/rust/vdb-rs/ only:
  Cargo.toml, Cargo.lock (generated)
  src/lib.rs, src/error.rs, src/kv.rs, src/vector.rs, src/index.rs,
  src/metric.rs, src/query.rs
```

# Commits Created

```text
See `git log --oneline` in this repository.
```

# Validation Completed

```text
PASS cargo fmt --all --check
PASS cargo check --workspace --all-targets --all-features
PASS cargo clippy --workspace --all-targets --all-features -- -D warnings
PASS cargo test --workspace --all-features --lib --tests   (11/11)
PASS cargo test --doc --workspace --all-features            (0 doctests)
PASS RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
PASS cargo check --workspace --no-default-features
PASS all 7 project-critical feature combinations (PROJECT_SPEC.md)
PASS ./scripts/validate-features.sh
```

# Current Failure

```text
None.
```

# Packaging Notes

```text
cargo package state: not yet run (no repository/version tag to package against)
Cargo.lock state: created, not yet committed
publishable crates: vdb
```

# CI Notes

```text
Latest run: <n/a>
Run ID: <n/a>
Current conclusion: no GitHub remote configured yet
```

# Important Architecture Notes

- LMDB (via `heed`) is the sole KV substrate — no alternative backends. See ADR-0001.
- `KvEngine` currently wraps only the `collections` named database; `vectors`/`metadata`/`logs` are Phase 2.
- Vector + index files use mmap with fixed binary headers (`VDB1`/`IDX1` magic) — both `VectorFile`/`IndexFile` are read-only so far; writing is Phase 2/4.
- HNSW nodes will reference vector IDs, which resolve to offsets via `VectorMeta` once that type exists (Phase 2).
- WAL will use a `logs` table with u64 sequence keys (Phase 2/6) — not yet implemented.

# Important Invariants

- LMDB is the sole KV substrate.
- Vector payload + index files are mmap'd per collection.
- Vector IDs (u64) are monotonically increasing within a collection.
- WAL is replayed on `open` if the dirty flag is set (not yet implemented).
- On-disk file formats are versioned; breaking changes require ADR.

# Protected Behavior

- `.vectors` file format (magic `VDB1`, header layout)
- `.index` file format (magic `IDX1`, HNSW node/levels layout)
- LMDB named-database layout (`collections` now; `vectors`/`metadata`/`logs` once added)
- `KvEngine` shape
- Public `VectorDb` / `Collection` / `QueryBuilder` API (not yet built)

# Known Traps

- Do not use `cargo package --allow-dirty` as the normal fix for dirty-package failures.
- Do not assume `--all-features` proves feature isolation — verified the hard way this session (see the `storage`/`serde_json` bug above).
- Do not omit `Cargo.lock` investigation when packaging reports it dirty.
- Do not weaken clippy or rustdoc warning gates.
- Do NOT introduce a second KV engine (SQLite, sled, redb) — the `KvEngine` trait exists for testability, not backend swapping.
- Do NOT store full vector blobs in LMDB — keep them in the mmap'd `.vectors` file with only metadata in LMDB.
- `EnvOpenOptions::open` is `unsafe` — read the safety comment already in `src/kv.rs::KvEngine::open` before changing it; don't drop the doc comment explaining the invariant.

# Workflow Preferences

- Provide complete file replacements as shell heredocs when requested.
- Keep commit messages detailed and descriptive.
- Prefer one logical commit per completed milestone.
- Run focused tests first, then full gate.
- Confirm CI after pushing milestone commits (once a remote exists).
- Record exact failing commands and root causes.

# Next Exact Step

```bash
cd /home/raven/Devspace/rust/vdb-rs
git log --oneline
./scripts/validate.sh
```

Then start Phase 2 (storage layout) per `PROJECT_STATE.md`'s "Next Intended Work".

# Recommended Resume Commands

```bash
cargo check --workspace --all-targets --all-features
cargo fmt --all --check
./scripts/validate.sh
./scripts/validate-features.sh
```

Then read:

```text
AGENTS.md
PROJECT_SPEC.md
PROJECT_STATE.md
AGENT_HANDOFF.md
docs/adr/0001-use-lmdb-as-kv-substrate.md
docs/adr/0002-hnsw-on-disk-layout.md
```
