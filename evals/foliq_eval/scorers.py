"""How an answer is scored. The answer itself is graded by a model
(`model_graded_fact`, in `tasks.py`); the checks here need no model:

| Scorer           | 1 means                                                    |
|------------------|------------------------------------------------------------|
| `cited_file`     | the answer names the file(s) the answer is in              |
| `cited_page`     | ...and the right page (PDF) or sheet (workbook) next to it |
| `said_not_found` | the answer says NOT FOUND                                  |
| `used_foliq`     | the agent ran the `foliq` command at least once            |

The citation checks are NaN (not counted) for unanswerable questions, which
have nothing to cite. `said_not_found` is the right reply there, and a false
refusal everywhere else.
"""

import math
import re

from inspect_ai.model import ChatMessageAssistant
from inspect_ai.scorer import Metric, SampleScore, Score, Target, grouped, metric, scorer
from inspect_ai.solver import TaskState

NOT_APPLICABLE = math.nan

# "p4", "p. 4", "pp. 4-5", "p4-5", "page 4", "pages 4 to 5", "pages 4, 5"
PAGE_PATTERN = re.compile(
    r"\b(?:pages?|pp?\.?)\s*(\d+)(?:\s*(?:-|\u2013|to|and|,)\s*p?(\d+))?",
    re.IGNORECASE,
)


@metric
def mean_where_scored() -> Metric:
    """The mean, leaving out samples the scorer does not apply to (NaN)."""

    def compute(scores: list[SampleScore]) -> float:
        values = [float(score.score.value) for score in scores]
        values = [value for value in values if not math.isnan(value)]
        return sum(values) / len(values) if values else NOT_APPLICABLE

    return compute


# Every scorer reports its overall mean and its mean per question group.
METRICS = [mean_where_scored(), grouped(mean_where_scored(), "group", all=False)]


@scorer(metrics=METRICS)
def cited_file():
    """The share of the expected files that the answer names."""

    async def score(state: TaskState, target: Target) -> Score:
        sources = state.metadata["sources"]
        if not sources:
            return Score(value=NOT_APPLICABLE, explanation="nothing to cite")
        answer = state.output.completion
        found = [source["file"] for source in sources if lines_naming(answer, source["file"])]
        return Score(value=len(found) / len(sources), explanation=f"cited: {found or 'none'}")

    return score


@scorer(metrics=METRICS)
def cited_page():
    """The share of the expected sources cited with the right page or sheet."""

    async def score(state: TaskState, target: Target) -> Score:
        sources = state.metadata["sources"]
        if not sources:
            return Score(value=NOT_APPLICABLE, explanation="nothing to cite")
        answer = state.output.completion
        right = [source["file"] for source in sources if cites_location(answer, source)]
        return Score(value=len(right) / len(sources), explanation=f"right page or sheet: {right or 'none'}")

    return score


@scorer(metrics=METRICS)
def said_not_found():
    """1 if the answer says NOT FOUND."""

    async def score(state: TaskState, target: Target) -> Score:
        refused = "NOT FOUND" in state.output.completion.upper()
        return Score(value=1.0 if refused else 0.0)

    return score


@scorer(metrics=METRICS)
def used_foliq():
    """1 if the agent ran `foliq` in the shell at least once."""

    async def score(state: TaskState, target: Target) -> Score:
        commands = shell_commands(state)
        foliq_commands = [command for command in commands if re.search(r"(^|[\s;&|(])foliq\s", command + " ")]
        return Score(value=1.0 if foliq_commands else 0.0, explanation="\n".join(foliq_commands[:5]))

    return score


def shell_commands(state: TaskState) -> list[str]:
    """Every command the agent ran with the bash tool."""
    commands = []
    for message in state.messages:
        if isinstance(message, ChatMessageAssistant):
            for call in message.tool_calls or []:
                if call.function == "bash":
                    commands.append(str(call.arguments.get("command", "")))
    return commands


def lines_naming(answer: str, file: str) -> list[str]:
    """The lines of `answer` that name `file` (with or without its extension)."""
    stem = file.rsplit("/", 1)[-1].rsplit(".", 1)[0].lower()
    return [line for line in answer.splitlines() if stem in line.lower()]


def cites_location(answer: str, source: dict) -> bool:
    """Whether a line naming the source's file also names its page or sheet."""
    lines = lines_naming(answer, source["file"])
    if "pages" in source:
        expected = set(source["pages"])
        return any(expected & cited_pages(line) for line in lines)
    sheet = simplify(source["sheet"])
    return any(sheet in simplify(line) for line in lines)


def cited_pages(line: str) -> set[int]:
    """The page numbers in a line: "p5-6" gives {5, 6}."""
    pages = set()
    for match in PAGE_PATTERN.finditer(line):
        first = int(match.group(1))
        last = int(match.group(2)) if match.group(2) else first
        if last < first or last - first > 50:  # "page 4, 120" is two pages, not a range
            pages.update({first, last})
        else:
            pages.update(range(first, last + 1))
    return pages


def simplify(text: str) -> str:
    """Lower case, without quotes and repeated spaces, to compare sheet names."""
    text = re.sub("[\"'`\u2018\u2019\u201c\u201d]", "", text.lower())
    return re.sub(r"\s+", " ", text)
