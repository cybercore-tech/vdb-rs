# ADR 0001: LMDB via heed

Status: accepted.

Use LMDB for the transactional authority because it provides mature local
single-writer transactions, durable commits and mmap reads. Rust calls it through
heed and its C LMDB dependency. The database takes its own advisory directory lock
to keep vector-file and index lifecycle changes coherent with LMDB.

The alpha accepts serialized public operations, a fixed configured map size, and
an additional LMDB vector-payload copy for recovery. Do not use a network filesystem
or mutate files with tools outside the library while it is open.
