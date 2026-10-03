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
warmed query latency; each mutation triggers another full build on the next ANN
query. The peak RSS figure covers ingestion/construction/queries in one process.
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
