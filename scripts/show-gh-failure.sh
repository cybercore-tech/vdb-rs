#!/usr/bin/env bash
set -euo pipefail

if [ -z "${1:-}" ]; then
  echo "Usage: $0 <run-id>"
  exit 1
fi

RUN_ID="$1"

echo "==> failing jobs for run ${RUN_ID}"
gh run view "${RUN_ID}" --json jobs --jq '.jobs[] | select(.conclusion == "failure")'

echo ""
echo "==> failed log for run ${RUN_ID}"
gh run view "${RUN_ID}" --log-failed
