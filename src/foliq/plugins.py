"""The shape of Python plugins.

A plugin is any object with the right methods; there is nothing to inherit.
These `Protocol`s only document the shape (and let type checkers check it).

```python
class CsvParser:
    name = "csv"
    version = "1"  # change it to re-parse every CSV file
    extensions = ("csv",)

    def parse(self, data: bytes, path: str) -> str:
        rows = [line.split(",") for line in data.decode().splitlines()]
        header = "| " + " | ".join(rows[0]) + " |"
        ...
        return markdown


index = foliq.Index(parsers=[CsvParser()])
```

Python plugins work through `foliq.Index`. The `foliq` command only runs the
built-in (Rust) plugins.
"""

from collections.abc import Sequence
from typing import Any, Protocol, TypedDict


class ParsedDocument(TypedDict, total=False):
    text: str  # required: the document as Markdown
    title: str
    metadata: dict[str, Any]


class Parser(Protocol):
    """Turns a file's bytes into Markdown.

    Return the Markdown text, or a dict with `text` plus optional `title` and
    `metadata`. foliq finds the headings, tables and pages in the Markdown.
    Write `<!-- page N -->` lines to mark pages, so citations show them.
    """

    @property
    def extensions(self) -> Sequence[str]:
        """Like `("csv", "tsv")`. They take priority over the built-in parsers."""
        ...

    def parse(self, data: bytes, path: str) -> str | ParsedDocument: ...


class Splitter(Protocol):
    """Cuts a document's Markdown text into chunks.

    Return pieces of the text, in order. foliq finds each piece in the text and
    adds its lines, pages, section and heading path.
    """

    def split(self, text: str) -> list[str]: ...


class Sink(Protocol):
    """Receives every change after a sync, to copy it somewhere else.

    Each change is a dict: `{"type": "upsert", "document": {...}, "chunks": [...]}`
    or `{"type": "delete", "document_id": ..., "collection": ..., "path": ...}`.
    Applying the same changes twice must be safe.
    """

    def apply(self, changes: list[dict[str, Any]]) -> None: ...
