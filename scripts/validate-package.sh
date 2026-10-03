#!/usr/bin/env bash
set -euo pipefail

CRATE_NAME="cybercore-vdb"

echo "==> cargo package --locked -p ${CRATE_NAME} --list"
cargo package --locked -p "${CRATE_NAME}" --list

echo "==> cargo package --locked -p ${CRATE_NAME}"
cargo package --locked -p "${CRATE_NAME}"

echo ""
echo "PACKAGE GATE: GREEN"
