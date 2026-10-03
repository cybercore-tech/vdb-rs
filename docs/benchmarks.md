# Synthetic benchmark baseline

Measured locally on 2026-10-02 with Rust 1.98.1, optimized release build, Linux,
and synchronized individual inserts. Command:

```sh
cargo run --release --example benchmark -- 1000 32 50
```

Deterministic random vectors, cosine metric, k=10, ef_search=128.

| Measurement | Result |
|---|---:|
| Vector count / dimensions | 1,000 / 32 |
| Query count | 50 |
| Recall@10 versus exact search | 100% |
| Ingestion | 14.732 s (67.9 vectors/s) |
| First graph build plus query | 0.891 s |
| Warm ANN p50 / p95 | 0.434 / 0.529 ms |
| Exact p50 / p95 | 0.903 / 0.955 ms |
| Database directory logical file sizes | 1,111,692 bytes |
| Linux process peak RSS | 4,740 KiB |

These are small synthetic observations, not production promises. Individual
fsyncs dominate ingestion on this machine. The initial build is separate from
warmed query latency; this baseline predates batch and incremental maintenance,
so each mutation triggered another full build on the next ANN query. The peak RSS figure covers ingestion/construction/queries in one process.
The benchmark must be repeated with larger, real-world embeddings and selective
filters before setting capacity or latency targets. CI also tests recall across
cosine, Euclidean and dot product and runs a small release benchmark smoke test.

## Larger synthetic case

Final implementation, same machine and compiler:

```sh
cargo run --release --example benchmark -- 5000 128 50
```

| Measurement | Result |
|---|---:|
| Vector count / dimensions | 5,000 / 128 |
| Recall@10 | 95.2% |
| Ingestion | 101.366 s (49.3 vectors/s) |
| First graph build plus query | 37.588 s |
| Warm ANN p50 / p95 | 3.621 / 5.946 ms |
| Exact p50 / p95 | 8.549 / 11.534 ms |
| Directory logical file sizes | 25,344,856 bytes |
| Linux peak RSS | 31,084 KiB |

This exposes the alpha's cost of synchronous ingestion and full graph rebuilds.
Warm ANN is faster than exact on this workload, but first-query latency is large.
Higher dimensions and count lower recall at a fixed candidate budget; applications
must tune ef_search against their own embeddings. Measurements overlapped local
validation/security-tool compilation and are observations, not controlled lab runs.
The 1,000-vector baseline preceded the final f64 metric-accumulation hardening;
rerun the harness for comparisons against the exact checked-out revision.

## Batch ingestion and incremental refresh

Measured locally on 2026-10-02, same Linux machine and Rust 1.98.1 release build.
The fourth argument is batch size; `just benchmark 1000 32 20 128` uses the same
harness. Input generation is outside ingestion timing. These observations ran
alongside some validation work and are not controlled capacity measurements.

```sh
cargo run --locked --release --example benchmark -- 1000 32 20 1
cargo run --locked --release --example benchmark -- 1000 32 20 128
cargo run --locked --release --example benchmark -- 2000 64 20 128
```

| Measurement | 1,000 × 32, batch 1 | 1,000 × 32, batch 128 | 2,000 × 64, batch 128 |
|---|---:|---:|---:|
| Ingestion seconds | 16.042 | 0.134 | 0.264 |
| Vectors per second | 62.3 | 7,485.4 | 7,584.0 |
| Initial build + query seconds | 1.085 | 1.183 | 5.112 |
| Warm ANN p50 / p95 ms | 0.387 / 0.571 | 0.523 / 0.632 | 1.271 / 1.609 |
| Exact p50 / p95 ms | 0.676 / 0.902 | 0.873 / 0.994 | 2.361 / 2.466 |
| Initial recall@10 | 100% | 100% | 100% |
| Mixed vectors inserted per round | 1 | 16 | 16 |
| Mixed write/delete/query p50 / p95 ms | 21.800 / 24.175 | 45.079 / 60.468 | 91.574 / 106.180 |
| Mixed recall@10 | 100% | 100% | 100% |
| Reopen first query ms | 2.759 | 2.801 | 7.049 |
| Final live-only full rebuild + query seconds | 1.139 | 1.173 | 5.266 |
| Linux peak RSS KiB | 7,704 | 8,096 | 11,384 |

Each mixed round performs one batch insert, deletes one original ID, and executes
an ANN query that refreshes the existing graph. Five rounds run after initial
recall/latency sampling. Mixed timing includes commits, file sync, graph hydration,
new-node construction, full snapshot publication and the query. Exact reference
queries used for mixed recall are outside that timer. The full rebuild comparison
removes the derived graph after closing all handles and reconstructs from the same
final live dataset. It excludes retained deleted routing nodes. Reopen timing
loads an already-current persisted snapshot, rather than replaying dirty mutations;
regressions separately cover reopening with an older snapshot and missing new nodes.

Batching amortizes durable commits/fsyncs; it does not remove them. Incremental
refresh avoids reconstructing every old node but still copies the graph vectors
and writes a complete snapshot. Deleted routing nodes accumulate until compaction
and widen the query beam. Real embedding datasets, larger scales and varied
write/query/deletion ratios remain necessary before setting production targets.
