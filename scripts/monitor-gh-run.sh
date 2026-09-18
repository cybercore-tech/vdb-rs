#!/usr/bin/env bash
set -euo pipefail

if [ -z "${1:-}" ]; then
  echo "Usage: $0 <run-id>"
  exit 1
fi

RUN_ID="$1"

echo "==> gh run watch ${RUN_ID}"
gh run watch "${RUN_ID}" --exit-status

echo ""
echo "Run ${RUN_ID} completed."
