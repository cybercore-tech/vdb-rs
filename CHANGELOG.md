# Changelog

## Unreleased

No changes yet.

## 0.1.0-alpha.1

First public alpha, published as `cybercore-vdb` with the library name `vdb`.

- Finalize registry metadata and installation instructions; keep the repository
  name `vdb-rs` because both `vdb` and `vdb-rs` are already registered crates.

- Add atomic `upsert_batch` with ordered database-wide IDs, shared vector-file sync,
  full validation before mutation, transaction rollback and interrupted-batch recovery.
- Refresh HNSW incrementally from clean older snapshots, including after reopening;
  retain deleted routing nodes until compaction and exclude them from results.
- Measure batch ingestion and mixed write/delete/query workloads. Refresh still
  hydrates graph vectors and writes a complete snapshot.
- Add a real local-note semantic-search example with CPU embeddings, project tags,
  source/snippet metadata, exact-vector and BM25 comparisons, and note lifecycle checks.
- Add readable `just semantic-search` and `just semantic-report` output, a Markdown
  report copy, and `just semantic-search-json` for automation. Private artifacts stay local.
- Extend CI/MSRV/release gates with frozen public embeddings and Python helper checks;
  document pilot results and update the project site with current measurements.

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
