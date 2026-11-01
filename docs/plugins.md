# Plugins

foliq is a small engine with sockets. Every socket can be filled by a built-in
plugin, your own Python class, or your own Rust code.

| Socket   | Does                                       | Built in                         |
|----------|--------------------------------------------|----------------------------------|
| Source   | lists files (with versions) and reads them | folders                          |
| Parser   | turns bytes into Markdown                  | Markdown, HTML, DOCX, PPTX, XLSX, PDF |
| Splitter | cuts a document into chunks                | one chunk per section            |
| Store    | saves and searches documents and chunks    | SQLite with full-text search     |
| Sink     | receives every change after a sync         | text mirror, JSON Lines          |

Python plugins work through `foliq.Index`. The `foliq` command runs Rust code
only, so it uses the built-in plugins and Rust plugins compiled into it.

## Python

A plugin is any object with the right method. There is nothing to inherit.

```python
import csv
import io

import foliq


class CsvParser:
    name = "csv"
    version = "1"           # change it to re-parse every CSV file
    extensions = ("csv",)   # tried before the built-in parsers

    def parse(self, data: bytes, path: str) -> str:
        rows = list(csv.reader(io.StringIO(data.decode())))
        lines = ["| " + " | ".join(row) + " |" for row in rows]
        lines.insert(1, "|" + " --- |" * len(rows[0]))
        return "\n".join(lines)  # or {"text": ..., "title": ..., "metadata": {...}}


class ParagraphSplitter:
    def split(self, text: str) -> list[str]:
        # Return pieces of the text, in order. foliq adds the lines, pages and headings.
        return [paragraph for paragraph in text.split("\n\n") if paragraph.strip()]


class PrintSink:
    def apply(self, changes: list[dict]) -> None:
        for change in changes:
            # {"type": "upsert", "document": {...}, "chunks": [...]}
            # {"type": "delete", "document_id": ..., "collection": ..., "path": ...}
            print(change["type"], change.get("document", change).get("path"))


index = foliq.Index(parsers=[CsvParser()], splitter=ParagraphSplitter(), sinks=[PrintSink()])
```

Parsers run in parallel threads; foliq takes the GIL for each call. An
exception in a plugin fails only that file, and the sync report says why.

## Rust

Rust plugins implement a trait from the `foliq-core` crate and are added with
`Index::builder`:

```rust
--8<-- "crates/foliq/examples/custom_parser.rs"
```

The traits are small: see `crates/foliq-core/src`. Coming next: Rust plugins
packaged as Python wheels, so `pip install foliq-something` adds a fast plugin
to `foliq.Index`.
