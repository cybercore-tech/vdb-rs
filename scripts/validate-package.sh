#!/usr/bin/env bash
set -euo pipefail

CRATE_NAME="vdb"

echo "==> cargo package -p ${CRATE_NAME} --list"
cargo package -p "${CRATE_NAME}" --list

echo "==> cargo package -p ${CRATE_NAME}"
cargo package -p "${CRATE_NAME}"

echo ""
echo "PACKAGE GATE: GREEN"
