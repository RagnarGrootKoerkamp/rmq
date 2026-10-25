# /// script
# requires-python = ">=3.11"
# dependencies = ["plotly"]
# ///
# AI Generated
"""Interactive (plotly) version of tradeoff.py for a single n: memory (bits / element) against
query time (left) and query latency (right). Hover a point for its exact numbers; click a
legend entry to hide a structure in both panels.

    uv run scripts/tradeoff_plotly.py                            # results.jsonl, n = 1e7
    uv run scripts/tradeoff_plotly.py --n 1e6
    uv run scripts/tradeoff_plotly.py --stat median -o tradeoff.html
    uv run scripts/tradeoff_plotly.py runs/a.jsonl runs/b.jsonl  # compare runs
    uv run scripts/tradeoff_plotly.py --vary B                   # connect configurations differing only in B
    uv run scripts/tradeoff_plotly.py --vary S,B                 # per structure, vary the first of these it has

All points of a structure are joined by one line, from the smallest to the largest value of the
varied param. Without --vary, a structure is connected only if exactly one of its params takes
more than one value.
"""

import argparse
import json
import math
from collections import defaultdict
from pathlib import Path

import plotly.graph_objects as go
from plotly.subplots import make_subplots

# Same palette and name order as plot.py, so a structure has the same color in both.
COLORS = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300"]
# Per file, when comparing several.
SYMBOLS = ["circle", "square", "diamond", "triangle-up", "cross", "star"]
DASHES = ["solid", "dash", "dot", "dashdot", "longdash", "longdashdot"]

parser = argparse.ArgumentParser()
parser.add_argument("files", nargs="*", default=["results.jsonl"])
parser.add_argument("--n", type=float, default=1e7)
parser.add_argument("--stat", choices=["mean", "median"], default="mean")
parser.add_argument("-o", "--output", default="tradeoff.html")
parser.add_argument(
    "--vary", type=lambda s: s.split(","), default=[], metavar="PARAM[,PARAM...]",
)
args = parser.parse_args()

# (structure, file) -> {params: {n: record}}, params as a sorted tuple of (name, value)
series = defaultdict(lambda: defaultdict(dict))
for path in args.files:
    with open(path) as f:
        for line in f:
            r = json.loads(line)
            params = tuple(sorted(r.get("params", {}).items()))
            name = r["name"].rsplit("::", 1)[-1].removesuffix("Family")
            series[name, path][params][r["n"]] = r

names = sorted({name for name, _ in series})
color_of = {name: COLORS[i % len(COLORS)] for i, name in enumerate(names)}
symbol_of = {path: SYMBOLS[i % len(SYMBOLS)] for i, path in enumerate(args.files)}
dash_of = {path: DASHES[i % len(DASHES)] for i, path in enumerate(args.files)}
multi_file = len(args.files) > 1


def series_label(name, path):
    return f"{name} ({Path(path).stem})" if multi_file else name


def nearest(records, target):
    """Record whose n is closest to target (in log scale), or None if none is within 10%."""
    n = min(records, key=lambda n: max(n, target) / min(n, target))
    return records[n] if max(n, target) / min(n, target) <= 1.1 else None


def params_label(params):
    return "<" + ", ".join(f"{k}={v}" for k, v in params) + ">" if params else ""


def vary_param(configs):
    """The param to connect a structure's configurations along, or None."""
    keys = {k for params in configs for k, _ in params}
    for k in args.vary:
        if k in keys:
            return k
    if args.vary:
        return None
    varying = [k for k in keys if len({dict(params).get(k) for params in configs}) > 1]
    return varying[0] if len(varying) == 1 else None


panels = [("query_time", "Query time"), ("query_latency", "Query latency")]
fig = make_subplots(
    rows=1, cols=len(panels), shared_yaxes=True, horizontal_spacing=0.04,
    subplot_titles=[f"{title}, n = {args.n:.0e}" for _, title in panels],
)

# (bits / element, ns / query, name, file, params, actual n) per panel
points = {key: [] for key, _ in panels}
vary_of = {}
for (name, path), configs in sorted(series.items()):
    vary_of[name, path] = vary_param(configs)
    for params, records in configs.items():
        r = nearest(records, args.n)
        if r is None:
            continue  # configuration wasn't run at this n
        for key, _ in panels:
            if key in r:
                points[key].append(
                    (r["memory_bytes"] * 8 / r["n"], r[key][args.stat] * 1e9, name, path, params, r["n"])
                )

# Shared x-range (log10 units) with a little padding.
xs_all = [p[0] for ps in points.values() for p in ps]
if not xs_all:
    raise SystemExit(f"no structure has results near n = {args.n:.0e}")
lo, hi = math.log10(min(xs_all)) - 0.1, math.log10(max(xs_all)) + 0.1

shown = set()  # series already in the legend
for col, (key, _) in enumerate(panels, start=1):
    # Join a structure's configurations from the smallest to the largest value of its varied param.
    lines = defaultdict(list)
    for p in points[key]:
        name, path, params = p[2], p[3], p[4]
        vary = vary_of[name, path]
        if vary is not None and vary in dict(params):
            lines[name, path].append((dict(params)[vary], params, p[0], p[1]))
    for (name, path), line in lines.items():
        if len(line) < 2:
            continue
        line.sort()
        fig.add_trace(
            go.Scatter(
                x=[q[2] for q in line], y=[q[3] for q in line], mode="lines",
                line=dict(color=color_of[name], dash=dash_of[path], width=1.5),
                legendgroup=series_label(name, path), hoverinfo="skip", showlegend=False,
            ),
            row=1, col=col,
        )

    for p in points[key]:
        x, y, name, path, params, n = p
        label = series_label(name, path)
        vary = vary_of[name, path]
        # Tag each point with its value of the varied param.
        text = f"<b>{vary}={dict(params)[vary]}</b>" if vary in dict(params) else ""
        fig.add_trace(
            go.Scatter(
                x=[x], y=[y], mode="markers+text", name=label, legendgroup=label,
                showlegend=label not in shown,
                marker=dict(size=10, symbol=symbol_of[path], color=color_of[name]),
                text=[text], textposition="top center", textfont=dict(color=color_of[name]),
                customdata=[[n]],
                hovertemplate=(
                    f"<b>{name}{params_label(params)}</b><br>"
                    + (f"file: {Path(path).name}<br>" if multi_file else "")
                    + "memory: %{x:.3g} bits / element<br>"
                    f"{args.stat}: %{{y:.3g}} ns<br>"
                    "n: %{customdata[0]:,}<extra></extra>"
                ),
            ),
            row=1, col=col,
        )
        shown.add(label)

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
