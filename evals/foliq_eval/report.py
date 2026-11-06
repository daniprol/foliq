"""Turn the Inspect logs of a run into results tables (Markdown and JSON).

One row per sample and epoch is read from the logs, then averaged per model,
setup and question group. Needle questions also get a table by corpus size.
"""

import json
import math
from collections import defaultdict
from pathlib import Path

from inspect_ai.event import ModelEvent
from inspect_ai.log import EvalLog, EvalSample, list_eval_logs, read_eval_log
from inspect_ai.model import ChatMessageAssistant

SETUPS = ["baseline", "foliq_skill", "foliq_no_skill"]
GROUPS = ["single_fact", "tables", "multi_hop", "needle", "unanswerable"]

# What each column is, in the order the tables show them.
COLUMNS = {
    "correct": "Correct",
    "cited_file": "Right file",
    "cited_page": "Right page",
    "said_not_found": "NOT FOUND",
    "tokens": "Tokens",
    "tool_calls": "Tool calls",
    "seconds": "Time (s)",
    "used_foliq": "Used foliq",
}

# The grader's letter grades as numbers.
GRADES = {"C": 1.0, "P": 0.5, "I": 0.0, "N": 0.0}


def report(log_folder: Path, output: Path) -> None:
    """Read every log in `log_folder`; write `output`.md and `output`.json."""
    rows = []
    for info in list_eval_logs(str(log_folder)):
        log = read_eval_log(info)
        if log.status != "success":
            print(f"  skipping {info.name}: {log.status}")
            continue
        rows += [sample_row(log, sample) for sample in log.samples or []]
    if not rows:
        raise SystemExit(f"no finished logs in {log_folder}")

    output.parent.mkdir(parents=True, exist_ok=True)
    # JSON has no NaN: write "not applicable" as null.
    plain = [{key: None if value != value else value for key, value in row.items()} for row in rows]
    output.with_suffix(".json").write_text(json.dumps(plain, indent=1) + "\n")
    output.with_suffix(".md").write_text(markdown(rows))
    print(f"  wrote {output.with_suffix('.md')} and {output.with_suffix('.json')} ({len(rows)} samples)")


def sample_row(log: EvalLog, sample: EvalSample) -> dict:
    """The numbers of one sample (one question, corpus and epoch)."""
    model = log.eval.model
    scores = sample.scores or {}

    def value(name: str) -> float:
        score = scores.get(name)
        return math.nan if score is None else float(score.value)

    grade = scores.get("model_graded_fact")
    return {
        "model": model,
        "setup": log.eval.metadata["setup"],
        "question": sample.metadata["question"],
        "group": sample.metadata["group"],
        "corpus": sample.metadata["corpus"],
        "epoch": sample.epoch,
        "correct": GRADES.get(str(grade.value), 0.0) if grade else math.nan,
        "cited_file": value("cited_file"),
        "cited_page": value("cited_page"),
        "said_not_found": value("said_not_found"),
        "used_foliq": value("used_foliq"),
        "tokens": agent_tokens(sample),
        "tool_calls": count_tool_calls(sample),
        "seconds": sample.working_time or math.nan,
        "error": str(sample.error.message) if sample.error else None,
    }


def agent_tokens(sample: EvalSample) -> int:
    """Tokens the agent used: every model call except the grader's.

    (`sample.model_usage` would also count the grader when it is the same model.)
    """
    total = 0
    for event in sample.events:
        if isinstance(event, ModelEvent) and event.role is None and event.output.usage:
            total += event.output.usage.total_tokens
    return total


def count_tool_calls(sample: EvalSample) -> int:
    """How many shell commands the agent ran."""
    calls = 0
    for message in sample.messages:
        if isinstance(message, ChatMessageAssistant):
            calls += sum(1 for call in message.tool_calls or [] if call.function == "bash")
    return calls


def average(rows: list[dict], column: str) -> float:
    values = [row[column] for row in rows if not math.isnan(row[column])]
    return sum(values) / len(values) if values else math.nan


def markdown(rows: list[dict]) -> str:
    """The tables, one section per model."""
    lines = ["# foliq agent eval: results", ""]
    lines += [
        "Means over every question and run. *NOT FOUND* is the right reply for",
        "unanswerable questions and a false refusal for the others. *Right file*",
        "and *Right page* only count questions that have an answer.",
        "",
    ]
    by_model = defaultdict(list)
    for row in rows:
        by_model[row["model"]].append(row)
    for model, model_rows in by_model.items():
        lines += [f"## {model}", ""]
        lines += group_table(model_rows)
        if any(row["group"] == "needle" for row in model_rows):
            lines += ["", "### Needle questions by corpus size", ""]
            lines += needle_table(model_rows)
        errors = [row for row in model_rows if row["error"]]
        if errors:
            lines += ["", f"{len(errors)} samples ended with an error (counted as wrong)."]
        lines.append("")
    return "\n".join(lines)


def group_table(rows: list[dict]) -> list[str]:
    header = ["Group", "Setup", "n", *COLUMNS.values()]
    lines = [table_row(header), table_row(["---"] * len(header))]
    for group in [*GROUPS, "all"]:
        for setup in SETUPS:
            selected = [row for row in rows if row["setup"] == setup and group in (row["group"], "all")]
            if selected:
                cells = [format_value(column, average(selected, column)) for column in COLUMNS]
                lines.append(table_row([group, setup, str(len(selected)), *cells]))
    return lines


def needle_table(rows: list[dict]) -> list[str]:
    needles = [row for row in rows if row["group"] == "needle"]
    corpora = sorted({row["corpus"] for row in needles}, key=lambda name: int(name.split("-")[1]))
    header = ["Papers", *[f"{setup} correct" for setup in SETUPS], *[f"{setup} tokens" for setup in SETUPS]]
    lines = [table_row(header), table_row(["---"] * len(header))]
    for corpus in corpora:
        cells = [corpus.split("-")[1]]
        for column in ["correct", "tokens"]:
            for setup in SETUPS:
                selected = [row for row in needles if row["setup"] == setup and row["corpus"] == corpus]
                cells.append(format_value(column, average(selected, column)) if selected else "")
        lines.append(table_row(cells))
    return lines


def format_value(column: str, value: float) -> str:
    if math.isnan(value):
        return "—"
    if column in ("tokens",):
        return f"{value:,.0f}"
    if column in ("tool_calls", "seconds"):
        return f"{value:.1f}"
    return f"{value:.0%}"


def table_row(cells: list[str]) -> str:
    return "| " + " | ".join(cells) + " |"
