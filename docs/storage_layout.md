# Storage Layout

## LMDB Tables

| Table        | Key     | Value                              |
|-------------|---------|------------------------------------|
| `collections` | `String` | `CollectionConfig { dim, metric, index_params, ... }` |
| `vectors`     | `u64`   | `VectorMeta { collection_id, offset, len, dim }`     |
| `metadata`    | `u64`   | JSON/MsgPack blob                                  |
| `logs`        | `u64`   | `Operation { op_type, id, timestamp, ... }`        |

## On-Disk Files (per collection)

### `collection_name.vectors`

```
[Header: magic "VDB1", dim: u32, count: u64]
[Data: contiguous f32[d] arrays, 32-byte aligned]
```

- `VectorMeta.offset` points into this file.
- mmap'd for zero-copy reads.

### `collection_name.index`

```
[Header: magic "IDX1", type: u8, M: u32, ef_construction: u32, dim: u32]
[Nodes: fixed-size HNSW node records]
[Levels: variable-length neighbor lists]
```

- Node IDs reference vector IDs (foreign key into `vectors` table).

## Crash Recovery

- LMDB's internal WAL ensures table consistency.
- `logs` table tracks unflushed vector/index appends.
- On `open`: if dirty flag is set, replay `logs` → append to mmap files.

## File Versioning

Both `.vectors` and `.index` start with 4-byte magic identifiers (`VDB1` / `IDX1`).
Reader code checks magic on open; incompatible versions trigger an explicit error.
