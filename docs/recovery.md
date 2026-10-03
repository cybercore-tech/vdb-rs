# Durable recovery protocol

The alpha uses LMDB's durable transaction journal plus a transactional redo
instruction in `pending`, rather than an independent append-only application log.
Full committed vector payloads are retained in `payloads`. Recovery instructions
are keyed by collection and coalesce into `Rebuild` or `Remove`.

## Insert

1. Acquire the shared operation mutex and finish any pending recovery.
2. Validate the collection incarnation, dimension and finite vector values.
3. Open the clean append file and compute its next aligned offset.
4. In one LMDB transaction, allocate a globally unique ID, increment collection
   revision, write location, metadata and payload, and set `pending[name]=Rebuild`.
5. Append the vector and checksum, patch the header count/checksum, and fsync.
6. Delete the pending marker in another durable LMDB transaction.
7. Return the ID.

A crash before step 4 commits leaves no new logical vector. A crash after step 4
may commit an operation whose caller never received success. Recovery preserves
that vector. This is at-least-once outcome uncertainty, not a claim of exactly-once
client retry semantics. An error after the logical commit has the same uncertainty.

## Recovery and compaction

Under the operation mutex, `Rebuild` writes all committed live payloads to a new
checksummed `.vectors.tmp` file and records their new offsets in memory. It flushes
and fsyncs the file, drops its writer, atomically renames it over `.vectors`, and
fsyncs the directory. A single LMDB transaction publishes all new offsets and removes
the marker. The old index is discarded. If any step fails, the marker remains and
the next attempt repeats the process. A crash after rename but before checkpoint
is safe: no query is allowed until the rebuild/checkpoint finishes.

Explicit compaction marks a rebuild and increments revision before beginning
file work, using this same protocol.

## Collection lifecycle and deletion

Creation commits a fresh generation, configuration and `Rebuild` marker. Recovery
materializes the empty file before any handle is returned. Deleting a vector removes
location, metadata and payload and increments revision in one transaction. It does
not need a file marker because the orphaned append block is never referenced again.
Collection deletion atomically removes all of its logical records and sets `Remove`.
Recovery removes derived files, fsyncs the directory, then checkpoints the marker.
The database-wide ID allocator is never reset, including after delete/recreate.

Process-kill tests stop workers with pending creation, torn vector-file insertion,
fully synced insertion, deletion and partial compaction. Each database is reopened
twice to verify durable outcomes, coherent metadata and idempotent replay. These
checks cover process termination; they do not emulate controller caches or power
failure. Durability relies on LMDB defaults and the filesystem's fsync/rename contract.
