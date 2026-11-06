"""The three setups of the eval, as Inspect tasks.

| Task             | The agent has                                              |
|------------------|------------------------------------------------------------|
| `baseline`       | a shell with grep, ripgrep, pdftotext and Python           |
| `foliq_skill`    | the same plus foliq, an index of /docs, and the foliq skill |
| `foliq_no_skill` | the same plus foliq and the index, but nobody mentions it  |

Same questions, same model, same limits. Run all three with:

    uv run inspect eval-set foliq_eval/tasks.py --model <provider/model> --log-dir logs/<name>

Task options (`-T name=value`):
    split=dev          dev, heldout or all
    groups=...         only these question groups, comma separated
    max_papers=1000    leave out corpora with more papers (10, 100 or 1000)
"""

from pathlib import Path

import yaml
from inspect_ai import Epochs, Task, task
from inspect_ai.agent import react
from inspect_ai.dataset import Sample
from inspect_ai.scorer import model_graded_fact
from inspect_ai.tool import bash
from inspect_ai.util import SandboxEnvironmentSpec

from foliq_eval import prompts, sandbox
from foliq_eval.corpora import CORPUS_SIZES, corpus_folder, corpus_name, index_folder
from foliq_eval.scorers import cited_file, cited_page, said_not_found, used_foliq

QUESTIONS = Path(__file__).resolve().parent.parent / "questions.yaml"

# Agents vary from run to run, so every question is asked this many times.
EPOCHS = 3

# Generous limits: they stop a stuck agent, not a normal one.
MESSAGE_LIMIT = 80
TOKEN_LIMIT = 2_000_000
TIME_LIMIT_SECONDS = 15 * 60
COMMAND_TIMEOUT_SECONDS = 180


@task
def baseline(split: str = "dev", groups: str = "", max_papers: int = 1000) -> Task:
    """The agent has a shell with common tools, and no foliq."""
    return agent_task("baseline", split, groups, max_papers)


@task
def foliq_skill(split: str = "dev", groups: str = "", max_papers: int = 1000) -> Task:
    """The agent has foliq, an index of the documents, and the foliq skill."""
    return agent_task("foliq_skill", split, groups, max_papers)


@task
def foliq_no_skill(split: str = "dev", groups: str = "", max_papers: int = 1000) -> Task:
    """The agent has foliq and an index, but is not told about them."""
    return agent_task("foliq_no_skill", split, groups, max_papers)


def agent_task(setup: str, split: str, groups: str, max_papers: int) -> Task:
    return Task(
        dataset=samples(setup, split, groups, max_papers),
        solver=react(
            prompt=prompts.instructions(setup),
            tools=[bash(timeout=COMMAND_TIMEOUT_SECONDS)],
        ),
        scorer=[model_graded_fact(), cited_file(), cited_page(), said_not_found(), used_foliq()],
        epochs=Epochs(EPOCHS, "mean"),
        message_limit=MESSAGE_LIMIT,
        token_limit=TOKEN_LIMIT,
        time_limit=TIME_LIMIT_SECONDS,
        metadata={"setup": setup, "split": split},
    )


def samples(setup: str, split: str, groups: str, max_papers: int) -> list[Sample]:
    """One sample per question and corpus.

    Needle questions are asked of every corpus size; the others of the
    10-paper corpus only.
    """
    wanted_groups = [group.strip() for group in groups.split(",") if group.strip()]
    result = []
    for question in load_questions():
        if split != "all" and question["split"] != split:
            continue
        if wanted_groups and question["group"] not in wanted_groups:
            continue
        sizes = CORPUS_SIZES if question["group"] == "needle" else [CORPUS_SIZES[0]]
        for size in sizes:
            if size > max_papers:
                continue
            result.append(sample(setup, question, corpus_name(size)))
    return result


def sample(setup: str, question: dict, corpus: str) -> Sample:
    check_prepared(setup, corpus)
    files = {}
    if setup == "foliq_skill":
        files[prompts.SKILL_PATH] = prompts.SKILL_FILE.read_text()
    return Sample(
        id=f"{question['id']}@{corpus}",
        input=question["question"],
        target=question["answer"],
        metadata={
            "question": question["id"],
            "group": question["group"],
            "split": question["split"],
            "corpus": corpus,
            "sources": question["sources"],
        },
        sandbox=SandboxEnvironmentSpec("docker", str(sandbox.compose_file(setup, corpus))),
        files=files,
        setup=None if setup == "baseline" else sandbox.COPY_INDEX,
    )


def load_questions() -> list[dict]:
    return yaml.safe_load(QUESTIONS.read_text())


def check_prepared(setup: str, corpus: str) -> None:
    """Stop early, with the fix, if `prepare` has not been run."""
    if not corpus_folder(corpus).exists():
        raise RuntimeError(f"corpus {corpus} is missing: run `uv run python -m foliq_eval prepare`")
    if setup != "baseline" and not (index_folder(corpus) / "index.sqlite").exists():
        raise RuntimeError(f"foliq index of {corpus} is missing: run `uv run python -m foliq_eval prepare`")
