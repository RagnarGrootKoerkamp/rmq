# /// script
# requires-python = ">=3.11"
# dependencies = ["plotly"]
# ///
# AI Generated
"""Interactive (plotly) version of tradeoff.py for a single n: memory (bits / element) against
query time (left) and query latency (right). Hover a point for its exact numbers; click a
legend entry to hide a structure in both panels.

    uv run tradeoff_plotly.py                       # results.jsonl, n = 1e7
    uv run tradeoff_plotly.py --n 1e6
    uv run tradeoff_plotly.py --stat median -o tradeoff.html
"""

import argparse
import json
import math
from collections import defaultdict

import plotly.graph_objects as go
from plotly.subplots import make_subplots

# Same palette and name order as plot.py, so a structure has the same color in both.
COLORS = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300"]

parser = argparse.ArgumentParser()
parser.add_argument("file", nargs="?", default="results.jsonl")
parser.add_argument("--n", type=float, default=1e7)
parser.add_argument("--stat", choices=["mean", "median"], default="mean")
parser.add_argument("-o", "--output", default="tradeoff.html")
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


panels = [("query_time", "Query time"), ("query_latency", "Query latency")]
fig = make_subplots(
    rows=1, cols=len(panels), shared_yaxes=True, horizontal_spacing=0.04,
    subplot_titles=[f"{title}, n = {args.n:.0e}" for _, title in panels],
)

# (bits / element, ns / query, name, actual n) per panel
points = {key: [] for key, _ in panels}
for name in names:
    r = nearest(by_name[name], args.n)
    if r is None:
        continue  # structure wasn't run at this n
    for key, _ in panels:
        if key in r:
            points[key].append((r["memory_bytes"] * 8 / r["n"], r[key][args.stat] * 1e9, name, r["n"]))

# Shared x-range (log10 units) with a little padding, so the staircase can run to the edge.
xs_all = [p[0] for ps in points.values() for p in ps]
if not xs_all:
    raise SystemExit(f"no structure has results near n = {args.n:.0e}")
lo, hi = math.log10(min(xs_all)) - 0.1, math.log10(max(xs_all)) + 0.1

shown = set()  # structures already in the legend
for col, (key, _) in enumerate(panels, start=1):
    front = pareto(points[key])

    # Pareto staircase: the best time reachable within a given memory budget, so it runs flat
    # from the last frontier point to the right edge.
    if front:
        fig.add_trace(
            go.Scatter(
                x=[p[0] for p in front] + [10**hi], y=[p[1] for p in front] + [front[-1][1]],
                mode="lines", line=dict(shape="hv", color="#888888", width=1.5),
                hoverinfo="skip", showlegend=False,
            ),
            row=1, col=col,
        )

    for p in points[key]:
        x, y, name, n = p
        on_front = p in front
        fig.add_trace(
            go.Scatter(
                x=[x], y=[y], mode="markers", name=name, legendgroup=name,
                showlegend=name not in shown,
                # Dominated structures are drawn hollow.
                marker=dict(
                    size=10, color=color_of[name] if on_front else "white",
                    line=dict(color=color_of[name], width=2),
                ),
                customdata=[[n, "yes" if on_front else "no"]],
                hovertemplate=(
                    f"<b>{name}</b><br>"
                    "memory: %{x:.3g} bits / element<br>"
                    f"{args.stat}: %{{y:.3g}} ns<br>"
                    "n: %{customdata[0]:,}<br>"
                    "pareto: %{customdata[1]}<extra></extra>"
                ),
            ),
            row=1, col=col,
        )
        shown.add(name)

fig.update_xaxes(
    type="log", range=[lo, hi], title_text="memory (bits / element)",
    gridcolor="#e0e0e0", dtick="D2", showline=True, linecolor="#444",
)
fig.update_yaxes(type="log", gridcolor="#e0e0e0", dtick="D2", showline=True, linecolor="#444")
fig.update_yaxes(title_text=f"{args.stat} (ns / query)", row=1, col=1)
fig.update_layout(
    template="plotly_white", width=1200, height=600,
    legend=dict(orientation="h", yanchor="bottom", y=1.08, xanchor="center", x=0.5),
    margin=dict(t=110),
)
fig.write_html(args.output, include_plotlyjs="cdn")
fig.show()
