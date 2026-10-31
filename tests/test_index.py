"""The Python API: search, read, context, access rules and plugins."""

from pathlib import Path
from typing import Any

import pytest

import foliq
from foliq.plugins import ParsedDocument


def test_search_returns_cited_chunks(index: foliq.Index) -> None:
    best = index.search("hotel price per night")[0]
    assert best["path"] == "travel.docx"
    assert best["heading_path"] == ["Travel policy", "Hotels"]
    assert best["citation"].startswith("docs/travel.docx#1.1 (L")


def test_context_fits_the_budget(index: foliq.Index) -> None:
    context = index.context("returned within thirty days", budget=120)
    assert context["token_count"] <= 120
    assert context["passages"][0]["path"] == "warranty.pdf"
    assert context["text"].startswith("[1] docs/warranty.pdf")


def test_missing_documents_raise_not_found(index: foliq.Index) -> None:
    with pytest.raises(foliq.NotFoundError):
        index.read("nothing.md")
    with pytest.raises(foliq.NotFoundError):
        index.read("notes.md#9")


def test_chunks_can_be_exported(index: foliq.Index) -> None:
    chunks = index.chunks(collections=["docs"])
    assert all(chunk["token_count"] > 0 and len(chunk["content_key"]) == 32 for chunk in chunks)


def test_access_rules(tmp_path: Path) -> None:
    (tmp_path / "hr").mkdir()
    (tmp_path / "hr" / "salaries.md").write_text("# Salaries\n\nSalary bands are secret.\n")
    index = foliq.Index(tmp_path / ".foliq")
    index.add(tmp_path / "hr", access=["group:hr"])
    index.sync()
    assert index.search("salary", principal="user:ana") == []
    assert len(index.search("salary", principal=["user:ana", "group:hr"])) == 1
    assert len(index.search("salary")) == 1  # no principal: a trusted caller sees everything


def test_tools_are_bound_to_a_principal(tmp_path: Path) -> None:
    (tmp_path / "hr").mkdir()
    (tmp_path / "hr" / "salaries.md").write_text("# Salaries\n\nSalary bands are secret.\n")
    index = foliq.Index(tmp_path / ".foliq")
    index.add(tmp_path / "hr", access=["group:hr"])
    index.sync()

    tools = {tool.name: tool for tool in index.tools(principal="user:ana")}
    assert set(tools) == {"search_documents", "get_context", "list_documents", "outline_document", "read_document"}
    assert "principal" not in str(tools["search_documents"].parameters)
    assert tools["search_documents"](query="salary").startswith("No results")
    assert tools["read_document"].call_json('{"reference": "hr/salaries.md"}').startswith("Error:")

    hr_tools = {tool.name: tool for tool in index.tools(principal="group:hr")}
    assert "Salary bands are secret." in hr_tools["read_document"].call_json({"reference": "hr/salaries.md"})
    assert hr_tools["get_context"].to_anthropic()["input_schema"]["required"] == ["query"]


class CsvParser:
    name = "csv"
    extensions = ("csv",)

    def parse(self, data: bytes, path: str) -> ParsedDocument:
        rows = [line.split(",") for line in data.decode().splitlines()]
        lines = ["| " + " | ".join(row) + " |" for row in rows]
        lines.insert(1, "|" + " --- |" * len(rows[0]))
        return {"text": "\n".join(lines) + "\n", "title": f"Table {path}", "metadata": {"rows": len(rows)}}


class ParagraphSplitter:
    def split(self, text: str) -> list[str]:
        return [paragraph for paragraph in text.split("\n\n") if paragraph.strip()]


class ListSink:
    def __init__(self) -> None:
        self.changes: list[dict[str, Any]] = []

    def apply(self, changes: list[dict[str, Any]]) -> None:
        self.changes.extend(changes)


def test_python_plugins(tmp_path: Path) -> None:
    folder = tmp_path / "data"
    folder.mkdir()
    (folder / "cities.csv").write_text("city,country\nLugo,Spain\n")
    (folder / "notes.md").write_text("# Notes\n\nFirst paragraph.\n\nSecond paragraph.\n")
    sink = ListSink()
    index = foliq.Index(tmp_path / ".foliq", parsers=[CsvParser()], splitter=ParagraphSplitter(), sinks=[sink])
    index.add(folder)
    assert index.sync()["added"] == 2

    assert index.read("cities.csv")["title"] == "Table cities.csv"
    assert index.documents()[0]["metadata"] == {"rows": 2}
    notes = [chunk for chunk in index.chunks() if chunk["text"].endswith("paragraph.")]
    assert [(chunk["text"], chunk["heading_path"]) for chunk in notes] == [
        ("First paragraph.", ["Notes"]),
        ("Second paragraph.", ["Notes"]),
    ]
    assert sorted(change["document"]["path"] for change in sink.changes) == ["cities.csv", "notes.md"]


def test_plugin_errors_are_reported_per_file(tmp_path: Path) -> None:
    class BrokenParser:
        extensions = ("csv",)

        def parse(self, data: bytes, path: str) -> str:
            raise RuntimeError("cannot read this")

    (tmp_path / "data").mkdir()
    (tmp_path / "data" / "a.csv").write_text("x")
    index = foliq.Index(tmp_path / ".foliq", parsers=[BrokenParser()])
    index.add(tmp_path / "data")
    failed = index.sync()["failed"]
    assert len(failed) == 1
    assert "RuntimeError: cannot read this" in failed[0]["message"]
