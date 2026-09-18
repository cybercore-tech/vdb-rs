# ADR-0002: HNSW on-disk layout

- **Status:** Proposed
- **Date:** 2026-09-17
- **Deciders:** vdb-rs architecture blueprint

## Context

The ANN index needs an on-disk graph format that can be mmap'd for fast
traversal without loading the entire structure into memory.

Options considered: store HNSW entirely in LMDB, store in a custom binary file,
store as Cap'n Proto / FlatBuffers.

## Decision

Use a **custom binary file format** per collection (`collection_name.index`)
with a fixed header + mmap-accessible node/levels blocks.

Reasons:

- LMDB stores small records; an HNSW graph's adjacency lists are larger and
  benefit from sequential mmap access.
- Custom binary format gives full control over alignment, padding, and
  versioning.
- Mirrors the `.vectors` file design for consistency.

## Layout

```
[Header: magic "IDX1" (4B), index_type (1B), M/u32, ef_construction/u32, dim/u32]
[Nodes block: fixed-size Node records]
  - id: u64
  - level: u8
  - offset into Levels block
[Levels block: variable-length per-level neighbor lists]
```

## Consequences

- Index files are versioned via magic bytes; breaking changes need ADR.
- Node references resolve to vector IDs, which map to offsets via `VectorMeta`
  in LMDB.
- Traversal is pointer-chasing through mmap'd bytes — no heap allocation per
  hop.

## Alternatives considered

- **Store HNSW in LMDB:** Simpler code but poor cache locality and higher
  storage overhead per edge. Rejected.
- **Cap'n Proto / FlatBuffers:** Adds a complex dependency; binary format
  overhead not justified for this use case. Rejected.
