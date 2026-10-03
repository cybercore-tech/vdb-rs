# Storage format 2

A database directory contains `data.mdb`, `lock.mdb`, persistent `vdb.lock`, and
per-collection `NAME.vectors`/`NAME.index` files. Temporary rebuild files use `.tmp`.
Never delete the lock file while a database may be open: advisory locks attach to
its inode. IDs and LMDB counters use big-endian u64 codecs for numeric key ordering.

LMDB named databases:

| Table | Key | Value |
|---|---|---|
| collections | collection name | JSON dimension, metric, generation, revision, next_id |
| vectors | database-wide ID | JSON collection, vector offset, dimension |
| metadata | database-wide ID | JSON metadata |
| payloads | database-wide ID | JSON finite f32 recovery payload |
| pending | collection name | JSON Rebuild/Remove instruction |
| state | name | u64 format=2, next_id, generation |
| logs | caller-supplied sequence | legacy caller-managed audit entry; not recovery |

The collection's `next_id` is informational (last allocation plus one); `state`
is the allocation authority. Collections have independent immutable generations
and incrementing mutation revisions. Legacy populated databases without a format
marker are rejected. There is no automatic migration for pre-alpha format 1.

## Vector files

All integers and f32 bytes are little-endian. Header length is 32:
magic `VDB2` (4), dimension (4), appended-block count (8), FNV-1a checksum of
bytes 0..16 (8), zero reserved bytes (8). Each block contains `dim*4` payload
bytes, an eight-byte FNV-1a checksum of the payload, then zero padding to a
32-byte boundary. Block stride is `ceil((dim*4+8)/32)*32`. Deleted blocks remain
until compaction. Readers validate header, dimension, exact file length, block
alignment and per-vector checksum. Invalid ranges return an error, not a panic.
Zero-copy reads require a little-endian host and externally immutable mappings.

## HNSW files

Header length is 64: `IDX2` magic, type=0, reserved bytes, M, ef_construction,
dimension, highest level, generation, revision, node count, entry position, and
checksum. Empty entry is u64::MAX. The checksum is FNV-1a(header bytes 0..56)
XOR FNV-1a(all bytes after the 64-byte header).

An array of u64 node offsets follows. Each variable-length node holds u64 ID,
u64 vector offset, u32 level, then for every level a u32 degree and degree u64
neighbor positions. Node offsets are contiguous, edges must target valid nodes
present at that level, IDs and adjacency entries must be unique, and degrees
are bounded by M (2M on layer zero). Checksums and boundaries are verified before
traversal. The index is rebuildable; it is never the authority for committed data.
