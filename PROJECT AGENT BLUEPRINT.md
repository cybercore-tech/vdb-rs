# PROJECT AGENT BLUEPRINT — vdb-rs

Embedded, persistent, Rust-native vector database using LMDB as the KV substrate
and memory-mapped files for vector payload + ANN index storage.

---

## 1. PROJECT IDENTITY

**Project Name:**  
vdb-rs

**Project Type:**  
Single Rust library crate (no workspace yet)

**Current Version:**  
0.1.0 (pre-alpha)

**Repository:**  
`<repository URL or local path>`

**Primary Language:**  
Rust

**Framework / Runtime:**  
No async runtime; embedded/synchronous. Optional `tokio` feature may be added later.

**Target Platforms:**  
Linux, macOS, Windows

**Rust Edition:**  
2021

**MSRV:**  
None yet — will be set at 1.70+ during Phase 2.

**Cargo workspace:**  
Single crate, not a workspace (scalable to one later).

---

## 2. PROJECT VISION

### One-Sentence Description

`vdb-rs` is an embedded, crash-safe, Rust-native vector database that stores
metadata and WAL entries in `LMDB`, stores f32 vector blobs in mmap'd files,
and indexes them with an on-disk HNSW graph — all exposed through a small,
typed collection/query API where LMDB is an implementation detail, not a
public dependency.

### Long-Term Capabilities

- Single-binary embedded usage (no server process).

- ACID transactions over collection metadata, vector metadata, and WAL.

- Zero-copy vector reads via per-collection `mmap` data files.

- ANN search (HNSW by default, IVF/Flat as future optional backends).

- Metadata filtering DSL (JSON/MsgPack blobs over LMDB).

- Automatic crash recovery via WAL replay.

- Optional quantized/compressed vector payloads.

- Optional hybrid dense + sparse scoring.

---

## 3. CORE PROBLEM

Developers embedding vector search in Rust applications are forced to:

- Spin up / manage a separate server (Qdrant, Weaviate, Milvus).

- Bridge to Python or gRPC, losing native performance and ergonomics.

- Or hand-roll their own storage + ANN code, fighting correctness alone.

`vdb-rs` gives them: `VectorDb::open(path)` → `create_collection` →
`upsert_vector` → `query(...).filter(...).execute()?` with everything
durably persisted on disk and zero external runtime.

### Current alternatives are insufficient because:

- Python-first vector DBs require a running server and native bindings are
  brittle.

- Pure in-memory Rust stores lose data on restart and don't handle scale.

- Embedded KV stores (sled, rocksdb) expose low-level APIs; none bundle an
  ANN index + storage layout out of the box.

### Primary user experience:

```rust
let db = VectorDb::open("data/my.vdb")?;
let coll = db.create_collection("docs", CollectionConfig {
    dim: 768,
    metric: Metric::Cosine,
    index: IndexConfig::hnsw(),
})?;
let id = coll.upsert_vector(&embedding, json!({
    "title": "Rust Vector DB",
    "tags": ["rust", "db", "vector"],
}))?;
let results = coll.query(&query_embedding, 10)
    .filter("tags CONTAINS 'rust'")
    .execute()?;
```

---

## 4. USERS

**Primary Users:**  
Intermediate-to-advanced Rust developers building AI agents, RAG systems,
or recommendation engines that need local-first vector search.

**Secondary Users:**  
Library authors wrapping `vdb-rs` into higher-level Rust agent frameworks.

**Expected Skill Level:**  
Intermediate to advanced. Users must understand Rust ownership/lifetimes and
basic vector search concepts (metrics, ANN).

---

## 5. CORE REQUIREMENTS

### Required Features

| #   | Feature                                                                                                          |
| --- | ---------------------------------------------------------------------------------------------------------------- |
| 1   | `LMDB` KV engine backing with `collections`, `vectors`, `metadata`, `logs` tables.                               |
| 2   | Per-collection `mmap` vector data file (`.vectors`) with binary header + contiguous f32 blocks, 32-byte aligned. |
| 3   | Per-collection `mmap` HNSW index file (`.index`) with binary header + node/levels layout.                        |
| 4   | `KvEngine` trait abstraction so LMDB is hidden from the public API.                                              |
| 5   | `Collection` CRUD: `create`, `delete`, `load`, `list_collections`.                                               |
| 6   | `upsert_vector(vector, metadata)` + `delete_vector(id)` per collection.                                          |
| 7   | Query builder: `query(&embedding, k).filter(expr).execute()` returning `(id, score, metadata)` tuples.           |
| 8   | WAL-based crash recovery: replay `logs` table on `open` if dirty flag is set.                                    |
| 9   | Metadata stored as serialized JSON/MsgPack blobs in LMDB `metadata` table.                                       |
| 10  | Compile-time / runtime validation: dimension mismatch, duplicate vector IDs, filter expr syntax.                 |

### Optional Features

- Quantization (`f16`, `i8` PQ) behind a `quantization` Cargo feature.

- IVF / Flat index backends behind an `index-ivf` feature.

- `tokio` async API behind a `async` feature (sync remains default).

- Sparse vector support for hybrid BM25 + dense scoring.

- Vector payload compression (`zstd`, `lz4`) in the `.vectors` file.

---

## 6. NON-GOALS

- Distributed / networked multi-node clustering.

- GPU or CUDA acceleration.

- HTTP server or gRPC API.

- SQL query engine or full-text index.

- Any non-`LMDB` KV backend (trait stays; impls are optional extras, not swaps).

- Schema migration tooling across incompatible storage layouts.

- Auto-sharding or rebalancing across disks.

---

## 7. TECHNICAL STACK

### Languages

- Rust (2021 edition)

### Frameworks

- None required at MVP. `tokio` optional for async feature.

### Major Dependencies

| Dependency                | Purpose                                                                   |
| ------------------------- | ------------------------------------------------------------------------- |
| `LMDB`                    | ACID embedded KV store for collections, vectors meta, user metadata, WAL. |
| `memmap2`                 | Cross-platform `mmap` for `.vectors` and `.index` files.                  |
| `serde` + `serde_json`    | Serialization of `CollectionConfig`, `VectorMeta`, metadata blobs.        |
| `thiserror`               | Typed error types.                                                        |
| `half`                    | Optional: `f16` quantization.                                             |
| `bincode` or custom codec | Serialization for vector metadata table values (decided Phase 2).         |

### Build System

Cargo

### Package Manager

cargo

---

## 8. ENVIRONMENT

```text
OS: Arch Linux
Shell: Bash
Terminal: Foot
Editor: Emacs
Architecture: x86_64
Rust: Stable
Build system: Cargo
```

---

## 9. REPOSITORY STRUCTURE

```text
vdb-rs/
├── Cargo.toml
├── Cargo.lock
├── CHANGELOG.md
├── README.md
├── LICENSE
├── PROJECT_STATE.md
├── AGENT_HANDOFF.md
├── AGENTS.md
├── PROJECT_SPEC.md
├── docs/
│   ├── arch.md
│   ├── storage_layout.md
│   ├── query_pipeline.md
│   └── adr/
│       ├── 0001-use-lmdb-as-kv-substrate.md
│       └── 0002-hnsw-on-disk-layout.md
├── scripts/
│   ├── validate.sh
│   ├── validate-features.sh
│   ├── validate-package.sh
│   ├── monitor-gh-run.sh
│   └── show-gh-failure.sh
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── kv/
│   │   ├── mod.rs
│   │   └── lmdb.rs
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── collection.rs
│   │   ├── vectors.rs
│   │   ├── metadata.rs
│   │   └── log.rs
│   ├── vector/
│   │   ├── mod.rs
│   │   ├── file.rs
│   │   └── layout.rs
│   ├── index/
│   │   ├── mod.rs
│   │   ├── hnsw.rs
│   │   └── file.rs
│   ├── collection.rs
│   ├── db.rs
│   ├── query.rs
│   └── metrics.rs
└── tests/
    ├── storage.rs
    ├── index.rs
    ├── query.rs
    └── crash_recovery.rs
```

### Key directories

| Path                             | Purpose                                                      |
| -------------------------------- | ------------------------------------------------------------ |
| `src/kv/`                        | `KvEngine` trait + LMDB implementation.                      |
| `src/storage/`                   | Redb-backed tables: collections, vector meta, metadata, WAL. |
| `src/vector/`                    | mmap vector data file (.vectors) read/write.                 |
| `src/index/`                     | HNSW index on-disk graph + mmap traversal.                   |
| `src/db.rs`, `src/collection.rs` | Public `VectorDb` / `Collection` API.                        |
| `src/query.rs`                   | Query builder + filter DSL.                                  |
| `docs/adr/`                      | Architecture Decision Records.                               |
| `scripts/`                       | Validation + CI helper scripts.                              |

---

## 10. ARCHITECTURAL PRINCIPLES

1. Clear module boundaries — KV layer, storage tables, vector files, index,
   and public API never cross-contaminate.

2. Small, stable public API — `VectorDb`, `Collection`, `QueryBuilder`,
   result types. Internal modules stay `pub(crate)`.

3. Strong typing — `CollectionId`, `VectorId`, `Metric`, `IndexType` as enums/
   newtypes, not bare `String`/`u64`.

4. Explicit error handling — `thiserror`-based error enum with rich context.

5. Minimal hidden global state — all state flows through `VectorDb` handle.

6. Deterministic behavior — no randomness in storage layout; HNSW seed is
   configurable for reproducibility.

7. Backwards compatibility where practical — on-disk format has a version byte;
   future layout changes require explicit migration.

8. Testable components — each layer (`kv`, `storage`, `vector`, `index`) has
   unit + integration tests.

9. Good diagnostics — errors identify the failing table, collection, or vector
   ID.

10. Documentation alongside implementation — rustdoc on every `pub` item;
    `docs/arch.md` for high-level flow.

---

## 11. SOURCE OF TRUTH

When information conflicts, use this priority:

1. Explicit current task instructions
2. `AGENTS.md`
3. `PROJECT_SPEC.md`
4. `PROJECT_STATE.md`
5. `AGENT_HANDOFF.md`
6. ADRs under `docs/adr/`
7. Passing tests
8. Public API contracts
9. User-facing docs (`README.md`, `docs/`)
10. Existing implementation
11. Recent Git history
12. Agent assumptions

Never silently change an established requirement.

---

## 12. AGENT ROLE

You are the **Senior project engineer + architecture assistant** for `vdb-rs`.

Your job is to:

- Understand the LMDB-backed storage layout before modifying it.
- Preserve working KV transaction behavior.
- Implement requested features completely and tested.
- Identify ANN index + mmap traversal design risks.
- Add tests at every layer.
- Maintain `docs/adr/`, `PROJECT_STATE.md`, `AGENT_HANDOFF.md`.
- Keep changes scoped to the current milestone.
- Leave the repo green after each session.

You are NOT expected to redesign the storage layout every time a new index type
is added.

---

## 13. OPERATING RULES

Before modifying code:

1. Inspect relevant files (`src/db.rs`, `src/kv/lmdb.rs`, `src/storage/*`).
2. Understand existing LMDB table schema.
3. Locate existing tests (`tests/storage.rs`, `tests/index.rs`).
4. Determine affected public APIs (`VectorDb`, `Collection`).
5. Identify feature flags (`storage`, `index-hnsw`, `metrics`, `serde-query` —
   current, isolation-tested; `quantization`, `index-ivf`, `async` —
   later-phase, not yet isolation-tested; see PROJECT_SPEC.md's Feature Matrix).

Then make the smallest coherent change that fully solves the problem.

Do not:

- Invent APIs without checking the trait/table pattern.
- Duplicate LMDB table access logic.
- Remove working transaction boundaries.
- Weaken tests to make failures disappear.
- Add dependencies without checking `Cargo.toml` first.
- Perform unrelated refactors during feature work.

---

## 14. CHANGE SAFETY RULES

Existing working functionality is protected.

Before replacing the HNSW graph layout or LMDB schema:

- Why must the format change? (backward compat break needs ADR)
- What depends on it? (query pipeline, crash recovery, tests)
- What compatibility impact exists?
- What tests protect it?

Prefer additive changes: new table name, new index type, new file version byte.

Breaking changes require an ADR + changelog entry + migration plan.

---

## 15. IMPLEMENTATION WORKFLOW

### Phase A — Inspect

- `Cargo.toml`, `src/lib.rs`, `src/kv/lmdb.rs`, `src/storage/mod.rs`.
- Tests under `tests/`.
- `PROJECT_STATE.md`, `AGENT_HANDOFF.md`, `docs/adr/`.
- `git log --oneline -15`.

### Phase B — Design

Define: behavior, interfaces, data structures, failure modes, compat concerns.

### Phase C — Implement

Smallest complete implementation.

### Phase D — Test

Unit + integration + regression + edge-case.

### Phase E — Validate

Run the full validation gate.

### Phase F — Document

Update rustdoc + `docs/`.

### Phase G — Commit

Detailed commit with tests + validation summary.

---

## 16. DEVELOPMENT PHASES

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

Each phase contains small, independently verifiable milestones. Do not start a
later milestone until the current one is green.

---

## 17. TASK FORMAT

```text
TASK:
<what needs to be built>

GOAL:
<why it exists>

REQUIREMENTS:
- requirement
- requirement
- requirement

CONSTRAINTS:
- constraint
- constraint

DO NOT CHANGE:
- ...

ACCEPTANCE CRITERIA:
- observable result
- observable result

FOCUSED TESTS:
- ...

FULL VALIDATION:
<commands/tests that must pass>
```

---

## 18. DEFINITION OF DONE

A task is DONE only when:

- Implementation is complete.
- Code is formatted (`cargo fmt`).
- Compiler checks pass.
- Clippy passes (`-D warnings`).
- All tests pass.
- Regression tests exist where appropriate.
- Rustdoc passes (`-D warnings`).
- Feature isolation checks pass.
- Documentation is updated.
- Examples are updated.
- CHANGELOG is updated if user-visible.
- Repository is clean after commit.

---

## 19. VALIDATION GATE

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
  --all-features

RUSTDOCFLAGS="-D warnings" \
cargo doc \
  --workspace \
  --no-deps \
  --all-features
```

---

## 20. FEATURE ISOLATION

```bash
cargo check --workspace --no-default-features
cargo test  --workspace --no-default-features

# Project-critical combinations:
cargo check -p vdb --no-default-features
cargo test  -p vdb --no-default-features
```

Record required combinations in `PROJECT_SPEC.md`.

Current feature matrix (Default/Requires inferred — no `Cargo.toml` yet;
see PROJECT_SPEC.md's Feature Matrix for the authoritative version):

| Feature        | Default | Requires  | Purpose                                       |
| -------------- | ------- | --------- | ---------------------------------------------- |
| `storage`      | yes     | —         | LMDB KV storage for metadata/WAL + mmap'd `.vectors` |
| `index-hnsw`   | yes     | `storage` | HNSW ANN index, mmap'd `.index` files         |
| `metrics`      | yes     | —         | Distance/similarity metric implementations     |
| `serde-query`  | yes     | serde     | (De)serialization for the query builder's filter DSL |
| `quantization` | no      | —         | f16/PQ vector compression (later-phase, not yet isolation-tested) |
| `index-ivf`    | no      | —         | IVF index backend alternative to HNSW (later-phase, not yet isolation-tested) |
| `async`        | no      | tokio     | Async `VectorDb` API (later-phase, not yet isolation-tested) |

---

## 21. TESTING PHILOSOPHY

Tests prove behavior, not implementation.

Prefer:

- Public API tests (`VectorDb::create_collection`, `Collection::upsert_vector`).

- Regression tests (reopen DB → data persists; crash mid-write → WAL replay).

- State-transition tests (insert → delete → query → empty).

- Serialization round-trip tests (`CollectionConfig`, `VectorMeta`).

- Malformed-input tests (dimension mismatch, corrupt index file).

- Failure-mode tests (missing file, locked file, OOM on large vectors).

For a bug fix: reproduce → add regression test → verify failure → fix → verify pass.

---

## 22. ERROR HANDLING

Errors use `thiserror`, preserve source context, and never expose secrets.

Prefer:

```text
failed to read vector data file for collection "docs": invalid header magic
```

over:

```text
internal error
```

---

## 23. SECURITY RULES

Treat external input as untrusted.

Validate:

- Paths (no path traversal in collection names → file paths).
- Serialized data (LMDB table values, mmap'd index bytes).
- User-provided metadata blobs.
- Query filter expressions.

Never commit secrets, keys, or credentials.

---

## 24. PERFORMANCE RULES

Do not optimize prematurely. Avoid:

- Full-file loads of large vector blobs.
- Unbounded memory growth during query.
- Accidental O(n) scans without index fallback.
- Blocking on mmap faults without bounded prefetch.

Measure before redesigning.

---

## 25. DEPENDENCY POLICY

New deps require: concrete need, maintenance review, MSRV review, platform
review, transitive-cost review. Prefer `heed`, `memmap2`, `serde` only.

---

## 26. PUBLIC API POLICY

Protected surfaces:

- `VectorDb::open`, `create_collection`, `list_collections`, `delete_collection`
- `Collection::upsert_vector`, `delete_vector`, `query`
- `QueryBuilder::filter`, `with_ef`, `execute`
- `CollectionConfig`, `IndexConfig`, `Metric`
- `QueryResult`, `ScoredVector`
- On-disk file formats (`.vectors`, `.index`) — versioned, never silently changed.

---

## 27. DOCUMENTATION REQUIREMENTS

Public APIs include purpose, usage, parameters, returns, errors, examples.

Major architecture: `docs/arch.md`, `docs/storage_layout.md`, `docs/query_pipeline.md`.

ADR per significant decision.

---

## 28. OUTPUT FORMAT FOR AGENTS

## What Changed

## Files Changed

## Implementation Details

## Tests Added

## Validation

## Remaining Work

## Suggested Next Step

---

## 29. FILE DELIVERY FORMAT

Prefer shell-ready heredocs for complete file replacements.

---

## 30. COMMAND DELIVERY FORMAT

Commands should be directly pasteable.

---

## 31. GIT WORKFLOW

```bash
git status --short
git diff --check
```

One logical commit per milestone.

---

## 32. CHANGELOG POLICY

Sections: Added / Changed / Deprecated / Removed / Fixed / Security.

---

## 33. AGENT DECISION AUTHORITY

**May:** fix compile errors, add tests, improve localized error handling, update
directly affected docs, tiny refactors for correctness.

**Should not:** replace the storage substrate, rename public APIs, remove
features, change on-disk formats, break backwards compat.

---

## 34. ASSUMPTION POLICY

When info is missing: inspect repo → infer from architecture → choose least
disruptive reasonable interpretation. State significant assumptions.

---

## 35. FAILURE POLICY

```text
FAILED COMMAND:
<cmd>

ERROR:
<error>

ROOT CAUSE:
<analysis>

AFFECTED FILES:
<files>

NEXT FIX:
<correction>
```

---

## 36. CONTEXT PRESERVATION

Maintain `PROJECT_STATE.md` and `AGENT_HANDOFF.md` at repo root.

---

## 37. ARCHITECTURE DECISION RECORDS

Record under `docs/adr/`. Each ADR: Context, Decision, Alternatives,
Consequences, Status.

---

## 38. AGENT HANDOFF FILE

Maintain `AGENT_HANDOFF.md` at repo root.

---

## 39. PROJECT INVARIANTS

- `LMDB` is the sole KV substrate; no other KV backend will replace it.

- Vector payload + index files are mmap'd per collection; never fully in memory.

- Vector IDs (`u64`) are monotonically increasing within a collection.

- WAL (`logs` table) is replayed on `open` if the dirty flag is set.

- On-disk file formats are versioned; breaking changes require an ADR.

- HNSW graph node references resolve to `VectorMeta` via LMDB; never assume
  contiguous ID-to-offset mapping.

- Public serialization formats remain backwards compatible.

---

## 40. CURRENT PROJECT STATE

**Current Phase:** Phase 0 — Scaffold + blueprint complete

**Current Milestone:** Repository scaffolded with vdb-rs blueprint

**Last Completed Milestone:** Blueprint creation

**Last Known Green Commit:** `<uncommitted>`

**Current Known Issues:**

- No implementation yet.

**Next Intended Work:** Phase 1 — Core architecture (KvEngine trait + LMDB layer).

---

## 41. INITIAL AGENT INSTRUCTION

1. Read this blueprint.
2. `pwd`, `git status --short`, `git log --oneline -12`.
3. Read all `PROJECT_*`, `AGENT_*`, `AGENTS.md`.
4. Inspect `docs/adr/`.
5. Run baseline: `cargo check --all-features`.
6. Identify architecture boundaries.
7. Identify current milestone.
8. Do not redesign — continue from Phase 1.

---

## 42. PROJECT IDEA

> An embedded, persistent Rust vector database. LMDB holds all structured
> metadata (collections config, vector metadata, user metadata, WAL); mmap'd
> per-collection files hold the raw f32 payloads and the HNSW graph. Users
> call `VectorDb::open`, `create_collection`, `upsert_vector`, and
> `query(...).filter(...)` — LMDB and mmap are invisible. Optional IVF/Flat
> backends, quantization, and async later.

---

## 43. CURRENT TASK

```text
TASK:
Scaffold vdb-rs repository from blueprint.

GOAL:
Create directory structure + Cargo.toml + placeholder modules so Phase 1
can begin.

REQUIREMENTS:
- Create docs/adr/ + placeholder ADRs.
- Create scripts/validate.sh + validate-features.sh.
- Cargo.toml with deps: LMDB, memmap2, serde, thiserror.
- src/lib.rs + mod placeholders.

DO NOT CHANGE:
- Blueprint template structure.

ACCEPTANCE CRITERIA:
- cargo check --all-features passes.
- Cargo.toml has correct dependencies.
- docs/adr/ directory exists with placeholder ADRs.

VALIDATION:
- cargo check --workspace --all-targets --all-features
- cargo fmt --all --check
```

---

## 44. FINAL DIRECTIVE

Preserve what works. Build incrementally. Keep architecture coherent. Prefer
complete implementations over placeholders. Leave the repo correct, green,
documented, and easy for the next agent to continue.
