# /// script
# requires-python = ">=3.11"
# dependencies = ["matplotlib"]
# ///
# AI Generated
"""Plot build time, query time and memory against n for each structure in one or more JSONL files.

    uv run plot.py                                  # results.jsonl
    uv run plot.py --stat median                    # plot medians instead of means
    uv run plot.py results-abc123.jsonl results-def456.jsonl   # compare runs
"""

import argparse
import json
from collections import defaultdict
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.ticker as ticker

COLORS = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300"]
# With several files, each file gets its own line style; color stays tied to the structure.
LINESTYLES = ["-", "--", ":", "-."]

parser = argparse.ArgumentParser()
parser.add_argument("files", nargs="*", default=["results.jsonl"])
parser.add_argument("--stat", choices=["mean", "median"], default="mean")
parser.add_argument("-o", "--output", default="results.png")
args = parser.parse_args()

# (structure, file) -> [(n, build, build_std, query, query_std, memory)]
series = defaultdict(list)
for path in args.files:
    with open(path) as f:
        for line in f:
            r = json.loads(line)
            name = r["name"].rsplit("::", 1)[-1].removesuffix("Family")
            series[(name, path)].append(
                (
                    r["n"],
                    r["build_time"][args.stat], r["build_time"]["std_dev"],
                    r["query_time"][args.stat], r["query_time"]["std_dev"],
                    r["memory_bytes"],
                )
            )

names = sorted({name for name, _ in series})
color_of = {name: COLORS[i % len(COLORS)] for i, name in enumerate(names)}
style_of = {path: LINESTYLES[i % len(LINESTYLES)] for i, path in enumerate(args.files)}
multi_file = len(args.files) > 1

fig, (ax_build, ax_query, ax_mem) = plt.subplots(1, 3, figsize=(18, 5))
bands = {ax_build: [], ax_query: [], ax_mem: []}  # drawn after the lines, so they don't set the y-range
for (name, path), rows in sorted(series.items()):
    rows.sort()
    ns = [r[0] for r in rows]
    label = f"{name} ({Path(path).stem})" if multi_file else name
    style = dict(color=color_of[name], ls=style_of[path], lw=2, marker="o", ms=3, label=label)
    for ax, col, scale in [(ax_build, 1, 1e3), (ax_query, 3, 1e9)]:
        ys = [r[col] * scale for r in rows]
        ax.plot(ns, ys, **style)
        bands[ax].append((ns, ys, [r[col + 1] * scale for r in rows], color_of[name]))
    # Memory in units of the 8-byte u64 input: 1.0 means as large as the input array.
    ax_mem.plot(ns, [r[5] / (8 * r[0]) for r in rows], **style)

for ax, title, ylabel in [
    (ax_build, "Build time", f"{args.stat} ± std dev (ms)"),
    (ax_query, "Query time", f"{args.stat} ± std dev (ns / query)"),
    (ax_mem, "Memory", "bytes / (8 n)"),
]:
    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_title(title)
    ax.set_xlabel("n")
    ax.set_ylabel(ylabel)
    lo, hi = ax.get_ylim()
    # Shaded band: stat ± std_dev. On a log axis the lower edge can be <= 0, so clip it
    # to the bottom of the plot; a band touching the bottom means "spread >= the value".
    for ns, ys, sds, color in bands[ax]:
        lower = [max(y - sd, lo) for y, sd in zip(ys, sds)]
        upper = [y + sd for y, sd in zip(ys, sds)]
        ax.fill_between(ns, lower, upper, color=color, alpha=0.15, lw=0)
    ax.set_ylim(lo, hi)
    if hi / lo < 100:  # spans < 2 decades: also tick at 2x and 5x
        ax.yaxis.set_major_locator(ticker.LogLocator(subs=(1, 2, 5)))
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda y, _: f"{y:g}"))
    ax.yaxis.set_minor_formatter(ticker.NullFormatter())
    ax.grid(True, which="major", color="#e0e0e0", lw=0.8)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
if len(series) > 1:
    ax_build.legend(frameon=False)

fig.tight_layout()
fig.savefig(args.output, dpi=150)
plt.show()
