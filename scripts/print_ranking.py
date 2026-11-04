# /// script
# requires-python = ">=3.11"
# ///
# AI Generated
"""Rank structures by a timing metric (default: batched query time) on a fixed-n run with random ranges.

Structures using more than --threshold bits per element are listed separately as excluded.

    uv run scripts/print_ranking.py rank_random
    uv run scripts/print_ranking.py rank_random --threshold 64 --stat median
    uv run scripts/print_ranking.py rank_random --metric query_latency
"""

import argparse
import json
import sys

from bench_runs import existing_run

METRICS = ["query_batch_time", "query_time", "query_latency", "build_time"]

parser = argparse.ArgumentParser()
parser.add_argument("file")
parser.add_argument("--threshold", type=float, default=40, help="max bits per element (default 40)")
parser.add_argument("--metric", choices=METRICS, default="query_time", help="field to rank by (default query_time)")
parser.add_argument("--stat", choices=["mean", "median"], default="mean")
parser.add_argument("--dist", default="fully_random", help="range distribution to keep (default fully_random)")
parser.add_argument("--n", type=float, help="n to use if the run contains several")
args = parser.parse_args()

with open(existing_run(args.file)) as f:
    records = [json.loads(line) for line in f if line.strip()]
records = [r for r in records if r["distribution"] == args.dist]
if not records:
    sys.exit(f"no records with distribution {args.dist!r}")

ns = sorted({r["n"] for r in records})
if args.n is not None:
    records = [r for r in records if r["n"] == int(args.n)]
    if not records:
        sys.exit(f"no records with n={int(args.n)}; available: {ns}")
elif len(ns) > 1:
    sys.exit(f"run contains several n ({ns}); pick one with --n")
n = records[0]["n"]

missing = [r for r in records if r.get(args.metric) is None]
if missing:
    print(f"warning: {len(missing)} record(s) have no {args.metric}; skipping them", file=sys.stderr)
    records = [r for r in records if r.get(args.metric) is not None]
    if not records:
        sys.exit(f"no records with {args.metric}")


def label(r):
    name = r["name"].rsplit("::", 1)[-1].removesuffix("Family")
    if r["params"]:
        name += "<" + ", ".join(f"{k}={v}" for k, v in r["params"].items()) + ">"
    return name

rows = [(label(r), r[args.metric][args.stat] * 1e9, r["memory_bytes"] * 8 / n) for r in records]
ranked = sorted((row for row in rows if row[2] <= args.threshold), key=lambda row: row[1])
excluded = sorted((row for row in rows if row[2] > args.threshold), key=lambda row: row[2])

width = max(len(row[0]) for row in rows)
print(f"n = {n}, distribution = {args.dist}, {args.stat} {args.metric}, threshold = {args.threshold:g} bits/elem\n")

print("Ranking")
for i, (name, ns_, bits) in enumerate(ranked, 1):
    print(f"{i:>3}. {name:<{width}}  {ns_:8.2f} ns  {bits:8.2f} bits/elem")
if not ranked:
    print("  (none)")

print("\nExcluded")
for name, ns_, bits in excluded:
    print(f"     {name:<{width}}  {ns_:8.2f} ns  {bits:8.2f} bits/elem")
if not excluded:
    print("  (none)")
