# /// script
# requires-python = ">=3.11"
# dependencies = ["matplotlib"]
# ///
# AI Generated
"""Plot the space-time tradeoff of each structure at a few fixed n: memory (bits / element)
against query time (top row) and query latency (bottom row), one column per n.

    uv run tradeoff.py                              # results.jsonl, n = 1e5, 1e6, 1e7
    uv run tradeoff.py --ns 1e4 1e6 1e8
    uv run tradeoff.py --stat median
"""

import argparse
import json
from collections import defaultdict

import matplotlib.pyplot as plt
import matplotlib.ticker as ticker

# Same palette and name order as plot.py, so a structure has the same color in both.
COLORS = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300"]

parser = argparse.ArgumentParser()
parser.add_argument("file", nargs="?", default="results.jsonl")
parser.add_argument("--ns", nargs="+", type=float, default=[1e5, 1e6, 1e7])
parser.add_argument("--stat", choices=["mean", "median"], default="mean")
parser.add_argument("-o", "--output", default="tradeoff.png")
args = parser.parse_args()

# structure -> {n: record}
by_name = defaultdict(dict)
with open(args.file) as f:
    for line in f:
        r = json.loads(line)
        by_name[r["name"].rsplit("::", 1)[-1].removesuffix("Family")][r["n"]] = r

names = sorted(by_name)
color_of = {name: COLORS[i % len(COLORS)] for i, name in enumerate(names)}


def nearest(records, target):
    """Record whose n is closest to target (in log scale), or None if none is within 10%."""
    n = min(records, key=lambda n: max(n, target) / min(n, target))
    return records[n] if max(n, target) / min(n, target) <= 1.1 else None


def pareto(points):
    """Points not beaten on both memory and time, in increasing memory order."""
    front, best = [], float("inf")
    for p in sorted(points):
        if p[1] < best:
            front.append(p)
            best = p[1]
    return front


fig, axes = plt.subplots(2, len(args.ns), figsize=(5 * len(args.ns), 9), sharex=True, sharey="row", squeeze=False)
fronts = {}  # ax -> pareto front; drawn once all axes have their final x-range
for col, target in enumerate(args.ns):
    for row, (key, title) in enumerate([("query_time", "Query time"), ("query_latency", "Query latency")]):
        ax = axes[row][col]
        # (bits / element, ns / query, std dev, name)
        points = []
        for name in names:
            r = nearest(by_name[name], target)
            if r is None or key not in r:
                continue  # structure wasn't run at this n
            points.append((r["memory_bytes"] * 8 / r["n"], r[key][args.stat] * 1e9, r[key]["std_dev"] * 1e9, name))
        fronts[ax] = pareto(points)
        for x, y, sd, name in points:
            # Dominated structures are drawn hollow.
            ax.errorbar(
                x, y, yerr=sd, color=color_of[name], marker="o", ms=7, capsize=3, zorder=3,
                mfc=None if (x, y, sd, name) in fronts[ax] else "white",
            )
        ax.set_title(f"{title}, n = {target:.0e}")
        ax.set_xscale("log")
        ax.set_yscale("log")
        for axis in (ax.xaxis, ax.yaxis):
            axis.set_major_locator(ticker.LogLocator(subs=(1, 2, 5)))
            axis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:g}"))
            axis.set_minor_formatter(ticker.NullFormatter())
        ax.grid(True, which="major", color="#e0e0e0", lw=0.8)
        for side in ("top", "right"):
            ax.spines[side].set_visible(False)
        if row == 1:
            ax.set_xlabel("memory (bits / element)")
        if col == 0:
            ax.set_ylabel(f"{args.stat} ± std dev (ns / query)")

# Pareto staircase: the best time reachable within a given memory budget, so it runs flat
# from the last frontier point to the right edge.
for ax, front in fronts.items():
    if not front:
        continue
    lo, hi = ax.get_xlim()
    xs = [p[0] for p in front] + [hi]
    ys = [p[1] for p in front] + [front[-1][1]]
    ax.step(xs, ys, where="post", color="#888888", lw=1.2, zorder=2)
    ax.set_xlim(lo, hi)

# One legend for the whole figure; every structure appears even if it's missing from some n.
handles = [plt.Line2D([], [], color=color_of[n], marker="o", ls="", ms=7) for n in names]
fig.legend(handles, names, loc="upper center", ncol=len(names), frameon=False)
fig.tight_layout(rect=(0, 0, 1, 0.95))
fig.savefig(args.output, dpi=150)
plt.show()
