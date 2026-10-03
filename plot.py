# /// script
# requires-python = ">=3.11"
# dependencies = ["matplotlib"]
# ///
"""Plot mean build and query time against n from results.jsonl."""

import json
import sys
from collections import defaultdict

import matplotlib.pyplot as plt
import matplotlib.ticker as ticker

COLORS = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300"]

path = sys.argv[1] if len(sys.argv) > 1 else "results.jsonl"
series = defaultdict(list)  # name -> [(n, build_mean, query_mean)]
with open(path) as f:
    for line in f:
        r = json.loads(line)
        name = r["name"].rsplit("::", 1)[-1].removesuffix("Family")
        series[name].append((r["n"], r["build_time"]["mean"], r["query_time"]["mean"]))

fig, (ax_build, ax_query) = plt.subplots(1, 2, figsize=(11, 4.5))
for color, (name, rows) in zip(COLORS, sorted(series.items())):
    rows.sort()
    ns = [r[0] for r in rows]
    ax_build.plot(ns, [r[1] * 1e3 for r in rows], marker="o", lw=2, color=color, label=name)
    ax_query.plot(ns, [r[2] * 1e9 for r in rows], marker="o", lw=2, color=color, label=name)

for ax, title, unit in [(ax_build, "Build time", "ms"), (ax_query, "Query time", "ns / query")]:
    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_title(title)
    ax.set_xlabel("n")
    ax.set_ylabel(f"mean ({unit})")
    lo, hi = ax.get_ylim()
    if hi / lo < 100:  # spans < 2 decades: also tick at 2x and 5x
        ax.yaxis.set_major_locator(ticker.LogLocator(subs=(1, 2, 5)))
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda y, _: f"{y:g}"))
    ax.yaxis.set_minor_formatter(ticker.NullFormatter())
    ax.grid(True, which="major", color="#e0e0e0", lw=0.8)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
if len(series) > 1:
    ax_query.legend(frameon=False)

fig.tight_layout()
fig.savefig("results.png", dpi=150)
plt.show()
