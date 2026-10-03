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
qualifying neighbors. Clean inserts/deletions retain the cached and persisted
snapshot. At the next ANN query, an earlier revision of the same generation and
dimension seeds a mutable graph by copying nodes, vectors and links. Existing
live IDs must have matching offsets. Only missing IDs undergo HNSW insertion;
deleted nodes retain routing links. The refreshed graph is atomically published
at the current revision. Opening a corrupt/incompatible graph or recovery instead
builds from live vectors. Compaction discards snapshots before changing offsets.

A cached live-ID set reconstructed from LMDB excludes deleted nodes from results.
The layer-zero beam is widened by the number of deleted nodes before filtering,
so deletion cannot simply consume the requested result slots. Heavy deletion
increases search cost; compaction reclaims the graph and vector file. Refresh is
incremental construction, but still O(graph size) in hydration/snapshot I/O and
uses temporary vector copies. An append journal or mutable overlay is deferred.

Single insertion and batch insertion share the same implementation. A batch
validates all rows, allocates consecutive IDs in one logical transaction, advances
revision once, appends all blocks, syncs once and checkpoints once. Transaction
failure rolls back every row and ID allocation; post-commit I/O failure retains
the entire logical batch for recovery. Empty batches have no mutation.

Constraints: local little-endian hosts, local POSIX filesystems, serialized API
operations, fixed configured LMDB capacity, no external modification. Low-level
primitives are intentionally outside the public API recovery contract.
