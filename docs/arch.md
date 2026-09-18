# vdb-rs Architecture Overview

This document describes the high-level architecture of `vdb-rs`.

## Layers

```
┌─────────────────────────────────────────┐
│         Public API (VectorDb)           │
│   create_collection, upsert_vector,     │
│   query().filter().execute()            │
├─────────────────────────────────────────┤
│          Collection / QueryBuilder      │
├─────────────────────────────────────────┤
│          KvEngine trait                 │
│   (abstraction — LMDB is the impl)      │
├─────────────────────────────────────────┤
│  Storage Tables (LMDB)                  │
│   • collections  (String → Config)      │
│   • vectors      (u64   → VectorMeta)   │
│   • metadata     (u64   → JSON/MsgPack) │
│   • logs         (u64   → Operation)    │
├─────────────────────────────────────────┤
│  mmap Files (per collection)            │
│   • *.vectors  (f32 blobs, aligned)     │
│   • *.index    (HNSW graph nodes)       │
└─────────────────────────────────────────┘
```

## LMDB Responsibilities

- ACID transactions over structured metadata.
- WAL replay on crash recovery.
- Single-writer/multi-reader safety.

## Application Responsibilities

- Vector data layout in mmap'd `.vectors` files.
- HNSW graph layout in mmap'd `.index` files.
- Query pipeline (preprocess → retrieve → filter → score).

## Data Flow: Insert

1. Append f32 vector to `collection.vectors` (mmap write + flush).
2. Write `VectorMeta { id, offset, len, dim }` to LMDB `vectors` table.
3. Add HNSW node to `collection.index` (mmap write).
4. Append `Operation` to LMDB `logs` table (WAL).
5. Mark collection dirty; commit LMDB transaction.

## Data Flow: Query

1. Preprocess query vector (normalize for cosine).
2. Traverse HNSW graph in `collection.index` (mmap).
3. Fetch candidate vectors via `VectorMeta.offset` → mmap read.
4. Apply metadata filters via LMDB `metadata` lookups.
5. Score + rerank; return top-k `(id, score, metadata)`.
