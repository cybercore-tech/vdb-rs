# Alpha milestone and follow-up work

Implemented: collection isolation/lifecycle, serialized writes and snapshots,
exclusive directory locking, transactional recovery payloads and redo markers,
idempotent file rebuild/checkpoint, mmap HNSW traversal, exact filtered search,
compaction, finite-vector/name/file validation, process-kill regressions, examples,
atomic batch ingestion, incremental HNSW insertion with deleted routing nodes,
mixed query/write benchmarks, real local-note semantic evaluation with a keyword
baseline and offline embedding regression fixture, recall benchmark, CI/MSRV/package/security/release tooling and format documentation.

Next priorities:

1. Expand beyond the 122-document/426-chunk local-note pilot with larger real
   embedding corpora and more independent relevance judgments. Evaluate hybrid
   BM25/vector ranking: BM25 ranked known answers higher overall in the pilot.
2. Integrate the standalone search example with Cyberdesk: background embeddings,
   source hashes and chunk-ID manifests, real note tags, edit/delete reconciliation,
   stale-version handling and links back to the original Markdown. Application
   edits are not yet atomic; vdb-rs still inserts under new IDs.
3. Reduce incremental refresh hydration and full snapshot writes. Measure a mutable
   overlay/append journal, bounded checkpoints and compaction thresholds for
   retained deleted nodes. Collection-scoped location iteration and bounded top-k
   exact selection should address shared-table scans and full sorting.
4. Explicit update-by-ID and retry/idempotency semantics, richer filter expressions,
   and collection statistics.
5. Automatic map growth, concurrent read snapshots, and leaner durable vector
   payload encoding (currently JSON copies in LMDB).
6. Quantization, IVF and async APIs only after measured need and dedicated coverage.

No crates.io publication is part of this milestone. Registry-name availability
and maintainer ownership must be established before registry publishing.
