# Changelog

## 0.1.0-alpha.1

- Fix cross-collection ID collisions, foreign-ID deletion and delete/recreate failures.
- Add collection generations, serialized snapshots and exclusive directory locking.
- Atomically persist metadata/vector payloads with recoverable file materialization.
- Add idempotent rebuild/checkpoint recovery and compaction with process-kill tests.
- Implement multi-layer HNSW construction, persistence and mmap traversal; preserve
  exact search and exact filtered search.
- Introduce checksummed storage format 2; populated pre-alpha format 1 is rejected.
- Validate names, dimensions, finite vectors, file layouts and graph references.
- Add runnable examples, recall/performance baseline, licenses and architecture docs.
- Add Linux/macOS CI, locked MSRV checks, verified packaging, dependency security,
  Dependabot and signed draft crate releases under cybercore-tech/vdb-rs.
