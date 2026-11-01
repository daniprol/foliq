"""Draws the README charts from `results/*.json`, in a light and a dark version.

Each chart is a set of small panels of horizontal bars, best first. foliq is
blue and every other library is gray, so the eye finds foliq first. The SVGs
have a transparent background and are shown with a `<picture>` tag that picks
the version matching the reader's GitHub theme.
"""

import json
from pathlib import Path

import matplotlib

matplotlib.use("svg")
import matplotlib.pyplot as plt  # the backend must be chosen before this import

# Same SVG ids on every run, so a re-run only changes the charts whose numbers changed.
matplotlib.rcParams["svg.hashsalt"] = "foliq"

RESULTS = Path(__file__).resolve().parent.parent / "results"

THEMES = {
    "light": {"foliq": "#2a78d6", "other": "#b9b7b0", "text": "#0b0b0b", "muted": "#52514e", "grid": "#e4e3df"},
    "dark": {"foliq": "#3987e5", "other": "#5c5b57", "text": "#f0efec", "muted": "#c3c2b7", "grid": "#33332f"},
}


def load(name: str) -> dict | None:
    path = RESULTS / name
    return json.loads(path.read_text()) if path.exists() else None


def draw_panels(
    name: str,
    panels: list[tuple[str, list[tuple[str, float]]]],
    value_format: str,
    higher_is_better: bool,
    maximum: float | None = None,
) -> None:
    """One row of panels; each panel is (title, [(library, value), ...])."""
    tallest = max(len(rows) for _, rows in panels)
    for theme_name, theme in THEMES.items():
        figure, axes = plt.subplots(1, len(panels), figsize=(3.3 * len(panels), 0.38 * tallest + 0.9), squeeze=False)
        for axis, (title, rows) in zip(axes[0], panels, strict=True):
            rows = sorted(rows, key=lambda row: row[1], reverse=higher_is_better)[::-1]
            labels = [label for label, _ in rows]
            values = [value for _, value in rows]
            colors = [theme["foliq"] if label.startswith("foliq") else theme["other"] for label in labels]
            axis.barh(labels, values, color=colors, height=0.62)
            largest = max(values) if values else 1
            for position, value in enumerate(values):
                axis.text(
                    value + largest * 0.02,
                    position,
                    value_format.format(value),
                    va="center",
                    fontsize=8.5,
                    color=theme["muted"],
                )
            axis.set_xlim(0, (maximum or largest) * 1.3)
            if maximum:
                axis.set_xticks([0, maximum / 2, maximum])
            # Panels with fewer bars keep them at the top, aligned with the others.
            axis.set_ylim(len(rows) - tallest - 0.4, len(rows) - 0.4)
            axis.set_title(title, loc="left", fontsize=11, fontweight="bold", color=theme["text"])
            axis.tick_params(axis="y", length=0, labelsize=9, colors=theme["text"])
            axis.tick_params(axis="x", length=0, labelsize=7.5, colors=theme["muted"])
            axis.grid(axis="x", color=theme["grid"], linewidth=0.8)
            axis.set_axisbelow(True)
            for side in axis.spines.values():
                side.set_visible(False)
            for label in axis.get_yticklabels():
                if label.get_text().startswith("foliq"):
                    label.set_fontweight("bold")
        figure.tight_layout(w_pad=2.5)
        path = RESULTS / f"{name}-{theme_name}.svg"
        figure.savefig(path, transparent=True, metadata={"Date": None})
        plt.close(figure)
        # matplotlib ends some lines with a space; the trailing-whitespace hook rejects them.
        lines = path.read_text().splitlines()
        path.write_text("\n".join(line.rstrip() for line in lines) + "\n")


def draw_all() -> None:
    quality_panels = []
    for file_format in ["pdf", "docx", "pptx", "xlsx", "html"]:
        report = load(f"quality-{file_format}.json")
        if report:
            rows = [(row["name"], 100 * row["similarity"]) for row in report["libraries"]]
            count = report["documents"]
            quality_panels.append((f"{file_format.upper()} ({count} doc{'s' if count != 1 else ''})", rows))
    if quality_panels:
        draw_panels("quality", quality_panels, "{:.0f}%", higher_is_better=True, maximum=100)

    speed_panels = []
    for file_format in ["pdf", "docx", "pptx", "xlsx"]:
        report = load(f"speed-{file_format}.json")
        if report:
            rows = [(row["name"], row["files_per_second"] or 0) for row in report["libraries"]]
            speed_panels.append((f"{file_format.upper()} ({report['files']} files)", rows))
    if speed_panels:
        draw_panels("speed", speed_panels, "{:.0f}/s", higher_is_better=True)

    report = load("search-scifact.json")
    if report:
        engines = report["engines"]
        draw_panels(
            "search-quality",
            [
                ("nDCG@10", [(row["name"], row["ndcg_at_10"]) for row in engines]),
                ("Recall@100", [(row["name"], row["recall_at_100"]) for row in engines]),
            ],
            "{:.2f}",
            higher_is_better=True,
            maximum=1,
        )
        draw_panels(
            "search-speed",
            [
                ("Query, top 10 (ms)", [(row["name"], row["median_query_milliseconds"]) for row in engines]),
                ("Build the index (s)", [(row["name"], row["index_seconds"]) for row in engines]),
            ],
            "{:.1f}",
            higher_is_better=False,
        )
    print(f"charts written to {RESULTS}")
