#!/usr/bin/env bash
set -euo pipefail

echo "==> cargo fmt"
cargo fmt --all --check

echo "==> git diff --check"
git diff --check

echo "==> cargo check --workspace --all-targets --all-features"
cargo check \
  --workspace \
  --all-targets \
  --all-features

echo "==> cargo clippy --workspace --all-targets --all-features -- -D warnings"
cargo clippy \
  --workspace \
  --all-targets \
  --all-features \
  -- \
  -D warnings

echo "==> cargo test --workspace --all-features (lib + integration)"
cargo test \
  --workspace \
  --all-features \
  --lib \
  --tests

echo "==> cargo test --doc --workspace --all-features"
cargo test \
  --doc \
  --workspace \
  --all-features

echo "==> rustdoc -D warnings"
RUSTDOCFLAGS="-D warnings" \
cargo doc \
  --workspace \
  --no-deps \
  --all-features

echo
echo "FULL RUST GATE: GREEN"
