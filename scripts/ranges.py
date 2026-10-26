# /// script
# requires-python = ">=3.11"
# dependencies = ["plotly"]
# ///
# AI Generated
"""Bar plot of query time or query latency against range size, from the output of
`cargo run --release -- ranges <run>` (one line per structure and range size 2^B). Each structure
(name + params) gets its own bar in every group, so table variants can be compared side by side.
Hover a bar for its exact numbers; click a legend entry to hide a structure.

    uv run scripts/ranges.py runs/abc_ranges.jsonl                          # query time
    uv run scripts/ranges.py runs/abc_ranges.jsonl --metric latency --stat median
    uv run scripts/ranges.py runs/abc_ranges.jsonl --names OffsetSparseTable BitBlocks
    uv run scripts/ranges.py runs/abc_ranges.jsonl runs/def_ranges.jsonl    # compare runs
    uv run scripts/ranges.py runs/abc_ranges.jsonl --weigh                  # contribution to a random query

With --weigh, each band B is multiplied by the share of fully random queries (l <= r uniform)
with r - l in band B, so the bars of a structure sum to its expected fully random query cost.
"""

import argparse
import colorsys
import json
from collections import defaultdict
from pathlib import Path

import plotly.graph_objects as go

# Same palette as plot.py
COLORS = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300"]
METRICS = {
    "time": ("query_time", "Query time (independent queries)"),
    "latency": ("query_latency", "Query latency (dependent chain)"),
}

parser = argparse.ArgumentParser()
parser.add_argument("files", nargs="+")
parser.add_argument("--metric", choices=METRICS, default="time")
parser.add_argument("--stat", choices=["mean", "median"], default="mean")
parser.add_argument("--names", nargs="*", help="only plot structures whose label contains one of these")
parser.add_argument("--weigh", action="store_true", help="multiply band B by its share of a fully random query benchmark")
parser.add_argument("-o", "--output", default="ranges.html")
args = parser.parse_args()
key, title = METRICS[args.metric]


def params_label(params):
    return "<" + ", ".join(f"{k}={v}" for k, v in sorted(params.items())) + ">" if params else ""


def band_weight(b, n):
    """Share of uniform pairs l <= r < n with r - l in [2^(B-1), 2^B): n - d pairs have r - l = d."""
    lo, hi = 2 ** (b - 1), min(2**b, n)
    c = max(hi - lo, 0)
    return (c * n - (lo + hi - 1) * c / 2) / (n * (n + 1) / 2)


# label -> {B: (value, std_dev)}, values in ns, with B parsed from the distribution name "2^B"
series = defaultdict(dict)
ns = set()
multi_file = len(args.files) > 1
for path in args.files:
    with open(path) as f:
        for line in f:
            r = json.loads(line)
            label = r["name"].rsplit("::", 1)[-1].removesuffix("Family") + params_label(r.get("params", {}))
            if args.names and not any(s in label for s in args.names):
                continue
            if multi_file:
                label += f" ({Path(path).stem})"
            b = int(r["distribution"].removeprefix("2^"))
            weight = band_weight(b, r["n"]) if args.weigh else 1
            series[label][b] = (r[key][args.stat] * 1e9 * weight, r[key]["std_dev"] * 1e9 * weight)
            ns.add(r["n"])

if not series:
    parser.error("no results match")


def color(i):
    """The plot.py palette first, then hues spaced by the golden angle so extra structures stay distinct."""
    if i < len(COLORS):
        return COLORS[i]
    h = (0.6 + (i - len(COLORS)) * 0.618034) % 1
    l, s = (0.45, 0.65) if i % 2 else (0.6, 0.75)
    return "#{:02x}{:02x}{:02x}".format(*(round(c * 255) for c in colorsys.hls_to_rgb(h, l, s)))


labels = list(series)  # file order, so a structure keeps its color across --names filters of the same file
bs = sorted({b for points in series.values() for b in points})

fig = go.Figure()
for i, label in enumerate(labels):
    points = series[label]
    present = [b for b in bs if b in points]
    fig.add_trace(
        go.Bar(
            x=present, y=[points[b][0] for b in present], name=label, marker_color=color(i),
            error_y=dict(array=[points[b][1] for b in present], color="#808080", thickness=1, width=0),
            customdata=[(b - 1, points[b][1]) for b in present],
            hovertemplate=(
                f"<b>{label}</b><br>"
                "r - l ∈ [2<sup>%{customdata[0]}</sup>, 2<sup>%{x}</sup>)<br>"
                f"{args.stat}: %{{y:.3g}} ns<br>"
                "std dev: %{customdata[1]:.3g} ns<extra></extra>"
            ),
        )
    )

n_label = ", ".join(f"{n:,}" for n in sorted(ns))
fig.update_xaxes(
    title_text="range size r - l ∈ [2<sup>B-1</sup>, 2<sup>B</sup>)",
    tickvals=bs, ticktext=[f"2<sup>{b}</sup>" for b in bs], range=[bs[0] - 0.6, bs[-1] + 0.6],
    showline=True, linecolor="#444",
)
fig.update_yaxes(
    title_text=f"{'weighted ' if args.weigh else ''}{args.stat} ± std dev (ns / query)", rangemode="tozero",
    gridcolor="#e0e0e0", showline=True, linecolor="#444",
)
fig.update_layout(
    template="plotly_white", width=1400, height=600, title_text=f"{title}{', weighted by random query share' if args.weigh else ''}, n = {n_label}",
    barmode="group", bargap=0.2, bargroupgap=0.1,  # bargroupgap leaves a gap between neighbours
    showlegend=len(labels) > 1,
    legend=dict(orientation="h", yanchor="bottom", y=1.02, xanchor="center", x=0.5),
    margin=dict(t=110),
)
fig.write_html(args.output, include_plotlyjs="cdn")
fig.show()
