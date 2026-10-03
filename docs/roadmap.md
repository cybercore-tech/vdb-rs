# Alpha milestone and follow-up work

Implemented: collection isolation/lifecycle, serialized writes and snapshots,
exclusive directory locking, transactional recovery payloads and redo markers,
idempotent file rebuild/checkpoint, mmap HNSW traversal, exact filtered search,
compaction, finite-vector/name/file validation, process-kill regressions, examples,
recall benchmark, CI/MSRV/package/security/release tooling and format documentation.

Next priorities:

1. Batch ingestion and incremental HNSW maintenance, with large real embedding
   datasets and mixed query/write benchmarks. Current synchronous ingestion and
   lazy full graph rebuild are the principal performance limits.
2. Collection-scoped location iteration and bounded top-k exact selection;
   shared-table scans and full sorting still affect exact/filtered queries.
3. Explicit update-by-ID and retry/idempotency semantics, richer filter expressions,
   and collection statistics.
4. Automatic map growth, concurrent read snapshots, and leaner durable vector
   payload encoding (currently JSON copies in LMDB).
5. Quantization, IVF and async APIs only after measured need and dedicated coverage.

No crates.io publication is part of this milestone. Registry-name availability
and maintainer ownership must be established before registry publishing.
