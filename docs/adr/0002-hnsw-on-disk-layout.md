# ADR 0002: Derived HNSW snapshots

Status: accepted for alpha.

Store validated multi-layer graphs in checksummed, versioned index files with
an offset directory and variable-length adjacency lists. Persist collection
generation and revision so obsolete graphs cannot be used after mutations or
recreation. Queries traverse mmap edges and score the separate vector mmap.

Rebuild lazily after mutation, publish by fsync + atomic rename + directory sync,
and retain exact search as a recall baseline and filtered-query path. This makes
recovery simpler at the expense of first-query latency and temporary construction
memory. Incremental maintenance is the next performance milestone.
