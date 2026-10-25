# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
# AI Generated
"""Merge several JSONL run files into one. A result is identified by (name, params, n); the merge
is refused if any result appears more than once, within a file or across files, so no
measurement is silently duplicated or shadowed.

    uv run scripts/merge_runs.py runs/a.jsonl runs/b.jsonl -o runs/merged.jsonl
"""

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("files", nargs="+")
parser.add_argument("-o", "--output", required=True)
parser.add_argument("-f", "--force", action="store_true", help="overwrite the output if it exists")
args = parser.parse_args()

output = Path(args.output)
if any(output.resolve() == Path(path).resolve() for path in args.files):
    raise SystemExit("output must not be one of the inputs")
if output.exists() and not args.force:
    raise SystemExit(f"{output} exists; pass --force to overwrite")

# (name, params, n) -> [(file, line number)]
seen = defaultdict(list)
lines = []
for path in args.files:
    with open(path) as f:
        for lineno, line in enumerate(f, start=1):
            if not line.strip():
                continue
            r = json.loads(line)
            key = (r["name"], tuple(sorted(r.get("params", {}).items())), r["n"])
            seen[key].append((path, lineno))
            lines.append(line if line.endswith("\n") else line + "\n")

duplicates = {key: places for key, places in seen.items() if len(places) > 1}
if duplicates:
    for (name, params, n), places in sorted(duplicates.items()):
        label = name + ("<" + ", ".join(f"{k}={v}" for k, v in params) + ">" if params else "")
        where = ", ".join(f"{path}:{lineno}" for path, lineno in places)
        print(f"duplicate {label} n={n}: {where}", file=sys.stderr)
    raise SystemExit(f"refusing to merge: {len(duplicates)} result(s) appear more than once")

with open(output, "w") as f:
    f.writelines(lines)
print(f"wrote {len(lines)} results from {len(args.files)} files to {output}")
