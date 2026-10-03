# ADR 0002: Derived HNSW snapshots

Status: accepted for alpha.

Store validated multi-layer graphs in checksummed, versioned index files with
an offset directory and variable-length adjacency lists. Persist collection
generation and revision so obsolete graphs cannot be used after mutations or
recreation. Queries traverse mmap edges and score the separate vector mmap.

Refresh lazily after clean mutation. Hydrate the previous snapshot, insert only
new IDs with the existing construction algorithm, and retain deleted nodes as
routing links. Reconstruct live membership from LMDB and exclude deleted IDs
from results, widening the candidate budget by the number of deleted nodes.
Earlier revisions can seed refresh only within the same generation/dimension and
with unchanged live-vector offsets. This also supports clean reopen before refresh.

Publish by fsync + atomic rename + directory sync; recovery, compaction and
corrupt/incompatible snapshots reconstruct from live vectors. Exact search remains
the recall baseline and filtered-query path. The IDX2 format remains unchanged.

This trades an O(N) hydration/full snapshot write for avoiding O(N) HNSW node
construction on each mutation. Vector copies remain temporary, queries still
traverse mmap data, and dirty writes can coalesce until the next ANN query.
An append journal and mutable graph overlay require separate storage/recovery
coverage; evaluate those against larger real embedding workloads next.
