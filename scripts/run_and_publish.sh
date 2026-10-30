#!/usr/bin/env bash
# Run a benchmark into the data repo, then commit and push the new run files.
#
#     scripts/run_and_publish.sh <label> main [--min-n N] [--max-n N]
#     scripts/run_and_publish.sh <label> ranges
#
# The run is named <date>_<time>_<label>_<host>, e.g. 2026-10-31_142501_sweep_bench1, so
# `main` writes <run>_small/_random/_loguniform.jsonl and `ranges` writes <run>_ranges.jsonl.
# Runs go to $BENCH_RUNS if set, otherwise to the data repo checked out at runs/.
set -euo pipefail

if [[ $# -lt 2 ]]; then
    sed -n '2,9s/^# \{0,1\}//p' "$0" >&2
    exit 2
fi
label=$1
command=$2
shift 2
if [[ ! $label =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "label may only contain letters, digits, '.', '_' and '-'" >&2
    exit 2
fi

CODE="$(cd "$(dirname "$0")/.." && pwd)"
RUNS="${BENCH_RUNS:-$CODE/runs}"
id="$(date +%F_%H%M%S)_${label}_$(hostname -s)"

cd "$CODE"
BENCH_RUNS="$RUNS" cargo run --release -- "$command" "$id" "$@"

cd "$RUNS"
git add -- "$id"_*.jsonl
git commit -qm "Run: $id"
for _ in 1 2 3 4 5; do
    if git pull --rebase -q && git push -q; then
        echo "Published $id"
        exit 0
    fi
    sleep $((RANDOM % 5 + 1))
done
echo "Push failed; run $id is committed locally in $RUNS" >&2
exit 1
