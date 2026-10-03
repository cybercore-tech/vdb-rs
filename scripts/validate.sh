#!/usr/bin/env bash
set -euo pipefail

echo "==> cargo fmt"
cargo fmt --all --check

echo "==> git diff --check"
git diff --check

echo "==> cargo check --locked --workspace --all-targets --all-features"
cargo check --locked \
  --workspace \
  --all-targets \
  --all-features

echo "==> cargo clippy --locked --workspace --all-targets --all-features -- -D warnings"
cargo clippy --locked \
  --workspace \
  --all-targets \
  --all-features \
  -- \
  -D warnings

echo "==> cargo test --locked --workspace --all-features (lib + integration)"
cargo test --locked \
  --workspace \
  --all-features \
  --lib \
  --tests

echo "==> cargo test --locked --doc --workspace --all-features"
cargo test --locked \
  --doc \
  --workspace \
  --all-features

echo "==> rustdoc -D warnings"
RUSTDOCFLAGS="-D warnings" \
cargo doc --locked \
  --workspace \
  --no-deps \
  --all-features

echo
echo "FULL RUST GATE: GREEN"
