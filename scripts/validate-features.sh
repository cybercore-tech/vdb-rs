#!/usr/bin/env bash
set -euo pipefail

echo "==> workspace check with no default features"
cargo check --locked \
  --workspace \
  --no-default-features

echo "==> workspace test with no default features"
cargo test --locked \
  --workspace \
  --no-default-features

echo ""
echo "BASE FEATURE-ISOLATION GATE: GREEN"

echo ""
echo "==> vdb-rs single-feature isolation checks"

for feature in storage index-hnsw metrics serde-query; do
  echo "  -> checking: --no-default-features --features ${feature}"
  cargo check --locked -p vdb --no-default-features --features "${feature}"
done

echo ""
echo "==> vdb-rs critical feature-pair combinations"

echo "  -> storage + index-hnsw"
cargo check --locked -p vdb --no-default-features --features storage,index-hnsw

echo "  -> storage + index-hnsw + metrics"
cargo check --locked -p vdb --no-default-features --features storage,index-hnsw,metrics

echo "  -> storage + serde-query"
cargo check --locked -p vdb --no-default-features --features storage,serde-query

echo "  -> storage + metrics + serde-query (db.rs: VectorDb/Collection/QueryBuilder)"
cargo check --locked -p vdb --no-default-features --features storage,metrics,serde-query

echo ""
echo "FEATURE-ISOLATION GATE: GREEN"
