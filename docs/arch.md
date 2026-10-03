# Architecture

`VectorDb` and incarnation-bound `Collection` handles share one `Shared` object:
LMDB engine, canonical database directory and an operation mutex. A process-level
advisory lock stays held until the last handle closes. This deliberately favors
correctness over concurrency in the alpha.

LMDB is the durable authority. Each successful transaction contains all logical
state for its mutation. Vector payloads are retained in LMDB to make file recovery
independent of partially appended data. Checksummed vector files are materialized
copies used for zero-copy scoring. HNSW files are disposable derived snapshots.

Exact queries enumerate the requested collection, read mapped vectors, apply
metadata filters, score, and sort all qualifying candidates. This is O(database
records + collection size × dimension + matches log matches). It currently scans
the shared vector-location table to select the collection.

ANN queries cache validated mmap graph snapshots. A snapshot is reusable only if
its collection generation, revision and dimension match current configuration.
Construction uses a mutable multi-layer graph, deterministic geometric levels,
beam search and diversified neighbor selection. Publishing writes a temporary
file, fsyncs, renames and synchronizes the directory. Query traversal reads edges
from the mmap and vectors from the vector mmap. There is no exact fallback for
unfiltered ANN queries; `.exact()` explicitly selects the baseline.

Filtered queries use exact search because post-filtering ANN candidates can miss
qualifying neighbors. Inserts/deletions invalidate the cached graph; a persisted
old graph is rejected by revision. Compaction increments revision because offsets
change even if the logical set of IDs does not.

Constraints: local little-endian hosts, local POSIX filesystems, serialized API
operations, fixed configured LMDB capacity, no external modification. Low-level
primitives are intentionally outside the public API recovery contract.
