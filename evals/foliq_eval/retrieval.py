"""The retrieval check: what foliq itself returns for each question, no model.

For every answerable dev question and corpus, this asks foliq the question word
for word, in the same container the agents use, and checks:

| Column         | 1 means                                                       |
|----------------|---------------------------------------------------------------|
| `parsed`       | the evidence text is in foliq's text of the source document   |
| `file_top10`   | `foliq search` lists the source file in its top 10 results    |
| `page_top10`   | ...with a result on the right page (PDF) or sheet (workbook)  |
| `in_context`   | `foliq context --budget 2000` returns all the evidence text   |

`parsed` separates parsing problems (the fact is lost or garbled when the
file becomes text) from search problems (the fact is there, but not found).
It is the cheap first step before running agents, and needs no API key.
"""

import json
import re
import subprocess
from pathlib import Path

from foliq_eval import sandbox
from foliq_eval.corpora import CORPUS_SIZES, corpus_folder, corpus_name, index_folder
from foliq_eval.report import GROUPS, table_row
from foliq_eval.tasks import load_questions

CONTEXT_BUDGET = 2000
COLUMNS = ["parsed", "file_top10", "page_top10", "in_context", "context_tokens"]
SEARCH_LIMIT = 10


def run(output: Path, max_papers: int = 1000) -> None:
    """Check every answerable dev question; write `output`.md and `output`.json.

    Held-out questions are left out: this check is for tuning foliq.
    """
    questions = [question for question in load_questions() if question["sources"] and question["split"] == "dev"]
    rows = []
    for size in CORPUS_SIZES:
        if size > max_papers:
            continue
        name = corpus_name(size)
        with Container(name) as container:
            for question in questions:
                if question["group"] == "needle" or size == CORPUS_SIZES[0]:
                    rows.append(check(container, question, name))
        print(f"  {name}: checked")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.with_suffix(".json").write_text(json.dumps(rows, indent=1, ensure_ascii=False) + "\n")
    output.with_suffix(".md").write_text(markdown(rows))
    print(f"  wrote {output.with_suffix('.md')}")


class Container:
    """A foliq sandbox for one corpus, kept running while the checks run.

    `with Container(name) as container:` starts it and removes it at the end
    (like Python's `with open(...)`).
    """

    def __init__(self, corpus: str):
        self.corpus = corpus
        self.id = ""
        self.documents: dict[str, str] = {}  # file -> foliq's text, read once

    def __enter__(self) -> "Container":
        command = ["docker", "run", "--detach", "--rm", "--network", "none"]
        command += sandbox.mounts(corpus_folder(self.corpus))
        command += ["--volume", f"{index_folder(self.corpus)}:/opt/foliq-index:ro"]
        command += [sandbox.FOLIQ_IMAGE, "tail", "-f", "/dev/null"]
        self.id = subprocess.run(command, check=True, capture_output=True, text=True).stdout.strip()
        self.foliq_shell(sandbox.COPY_INDEX)
        return self

    def __exit__(self, *exception) -> None:
        subprocess.run(["docker", "rm", "--force", self.id], capture_output=True)

    def foliq_shell(self, command: str) -> str:
        return self.execute(["sh", "-c", command])

    def foliq(self, *arguments: str) -> str:
        return self.execute(["foliq", *arguments])

    def execute(self, command: list[str]) -> str:
        result = subprocess.run(
            ["docker", "exec", "--workdir", "/docs", self.id, *command], check=True, capture_output=True, text=True
        )
        return result.stdout

    def document_text(self, file: str) -> str:
        if file not in self.documents:
            self.documents[file] = self.foliq("read", f"docs/{file}")
        return self.documents[file]


def check(container: Container, question: dict, corpus: str) -> dict:
    sources = question["sources"]
    evidence = question.get("evidence", [])
    results = json.loads(container.foliq("--json", "search", question["question"], "--limit", str(SEARCH_LIMIT)))
    context = json.loads(container.foliq("--json", "context", question["question"], "--budget", str(CONTEXT_BUDGET)))
    documents = " ".join(container.document_text(source["file"]) for source in sources)

    file_hits = [source for source in sources if any(result["path"] == source["file"] for result in results)]
    page_hits = [source for source in sources if any(is_location(result, source) for result in results)]
    first_rank = next(
        (rank for rank, result in enumerate(results, 1) if result["path"] in {source["file"] for source in sources}),
        None,
    )
    return {
        "question": question["id"],
        "group": question["group"],
        "corpus": corpus,
        "parsed": all_found(evidence, documents),
        "file_top10": len(file_hits) / len(sources),
        "page_top10": len(page_hits) / len(sources),
        "first_rank": first_rank,
        "in_context": all_found(evidence, context["text"]),
        "context_tokens": context["token_count"],
        "missing_from_document": [text for text in evidence if normalize(text) not in normalize(documents)],
    }


def is_location(result: dict, source: dict) -> bool:
    """Whether a search result is in the source's file, on its page or sheet."""
    if result["path"] != source["file"]:
        return False
    if "pages" in source:
        pages = set(range(result["page_start"] or 0, (result["page_end"] or 0) + 1))
        return bool(pages & set(source["pages"]))
    return normalize(source["sheet"]) in normalize(" ".join(result["heading_path"]))


def all_found(evidence: list[str], text: str) -> float:
    text = normalize(text)
    return 1.0 if all(normalize(piece) in text for piece in evidence) else 0.0


def normalize(text: str) -> str:
    """Compare text the way a reader would: no case, bold marks, thousands
    separators, curly quotes or repeated spaces."""
    text = text.lower().replace("*", "").replace(",", "")
    text = re.sub("[\u2018\u2019]", "'", text)
    return re.sub(r"\s+", " ", text)


def markdown(rows: list[dict]) -> str:
    lines = [
        "# foliq retrieval check",
        "",
        "What `foliq search` and `foliq context` return for each answerable question,",
        "asked word for word, with no model. Made by `uv run python -m foliq_eval retrieval`.",
        "",
        "| Group | Corpus | n | Parsed | File in top 10 | Page in top 10 | Answer in context | Context tokens |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    keys = sorted(
        {(row["group"], row["corpus"]) for row in rows},
        key=lambda key: (GROUPS.index(key[0]), int(key[1].split("-")[1])),
    )
    for group, corpus in keys:
        selected = [row for row in rows if row["group"] == group and row["corpus"] == corpus]
        means = {column: sum(row[column] for row in selected) / len(selected) for column in COLUMNS}
        cells = [f"{means[column]:.0%}" for column in COLUMNS[:-1]] + [f"{means['context_tokens']:,.0f}"]
        lines.append(table_row([group, corpus, str(len(selected)), *cells]))
    lost = [row for row in rows if row["missing_from_document"] and row["corpus"] == corpus_name(CORPUS_SIZES[0])]
    if lost:
        lines += ["", "## Evidence missing from foliq's text of the document", ""]
        for row in lost:
            lines.append(f"- `{row['question']}`: " + ", ".join(repr(text) for text in row["missing_from_document"]))
    return "\n".join(lines) + "\n"
