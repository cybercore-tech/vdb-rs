# ADR-0001: Use LMDB (via `heed`) as the KV substrate

- **Status:** Accepted
- **Date:** 2026-09-17
- **Deciders:** vdb-rs architecture blueprint

## Context

`vdb-rs` needs an ACID embedded key-value store for structured metadata:
collection configs, per-vector metadata, user metadata blobs, and the WAL.

Options considered: sled, rocksdb, sqlite, redb, LMDB.

## Decision

Use **LMDB**, via the `heed` crate (a mature, typed, safe Rust wrapper
maintained by Meilisearch and used in Meilisearch's own production storage
engine), as the sole KV substrate.

Reasons:

- On-disk format and C API essentially frozen since ~2011 — stable in a way
  few embedded stores can claim, which matters for a project making
  file-format compatibility promises.
- Copy-on-write B+ tree with crash-safe, ACID commits and hard
  single-writer/multi-reader guarantees enforced by the engine itself, not
  something `vdb-rs` has to arbitrate.
- Reads are backed directly by mmap — zero-copy, no deserialization step to
  reach a value — which matches the mmap-first design already used for
  `.vectors` and `.index`. LMDB's "named databases" (sub-DBs within one
  environment file) play the same logical-table role `collections` /
  `vectors` / `metadata` / `logs` need.
- Battle-tested at real scale in unrelated, unglamorous production systems
  (OpenLDAP, Meilisearch) for a long time, not a young project still
  settling its API/format.

## Consequences

- LMDB is a hard dependency; no other KV backend replaces it.
- Unlike redb, LMDB is a C library — `heed` links it via FFI. This is a
  real trade against the "no native dependencies to compile" ideal, and is
  accepted deliberately in exchange for format/API maturity. `heed` bundles
  and builds LMDB via its `-sys` crate, so this doesn't require a system
  package, but it does mean a C toolchain is needed at build time.
- The `KvEngine` trait abstracts LMDB so the public API does not expose it.
- LMDB's copy-on-write model means very write-heavy workloads with many
  small transactions can bloat the environment file until compacted/resized
  — acceptable here since `vdb-rs` writes are batched upserts, not
  high-frequency OLTP-style single-row writes. Worth revisiting via
  benchmark once Phase 6 (WAL/crash recovery) lands.
- Future backend swaps require a full ADR and migration story.

## Alternatives considered

- **sled:** Simpler API but weaker transaction model; no longer actively
  maintained.
- **rocksdb:** Excellent write-heavy (LSM) performance but requires a C++
  build toolchain — worse native-dependency cost than LMDB's plain-C
  footprint, with no compensating benefit for `vdb-rs`'s read-heavy,
  batched-write access pattern.
- **sqlite:** Overkill for KV; adds a SQL layer we don't need.
- **redb:** Pure Rust and a clean API, but young — no 1.0 yet, on-disk
  format and API surface not yet frozen. Rejected specifically because
  `vdb-rs` makes its own file-format stability promises and shouldn't build
  them on top of a substrate still settling its own.
- **LMDB (via `heed`):** Chosen.
