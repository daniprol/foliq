# foliq

**Search, read and cite your documents from AI agents.** PDF, Word,
PowerPoint, Excel, HTML and Markdown, in one local index. Rust core, CLI and
Python API. No server, no API key.

```console
$ uvx foliq index ./docs
synced: 214 added, 0 updated, 0 unchanged, 0 removed

$ foliq search "refund enterprise customers"
docs/billing.pdf#2.3 (p4, L120-141)  Billing > Refunds > Enterprise  [7.81]
    Enterprise customers can ask for a refund within 30 days...

$ foliq read docs/billing.pdf#2.3          # the whole section
$ foliq context "how long do refunds take" --budget 1500
```

## Why

Agents are good at searching code with `grep` and reading files. Documents are
harder: PDFs and Office files aren't text, and whole files don't fit in a
prompt. foliq gives agents the same workflow for documents:

- **Find**: `search` ranks chunks (BM25) and prints a citation for each.
- **Navigate**: `outline` shows the sections of a document with their sizes in tokens.
- **Read exactly what's needed**: `read doc#2.3` or `read doc.pdf#p4-6`, with `--budget`.
- **Answer**: `context` packs the best passages (whole sections when they fit)
  into a token budget, each with its citation.

Every command checks for changed files first, so answers are never stale.
Copies of PDFs and Office files as Markdown live in `.foliq/text/`, so `grep`
works on them too.

## For agents

```console
$ foliq skill > .claude/skills/foliq/SKILL.md
```

The skill file teaches an agent the search, outline, read and cite workflow.

## Python

```python
import foliq

index = foliq.Index(".foliq")
index.add("docs")
index.add("hr", access=["group:hr"])  # only principals in group:hr see these
index.sync()

index.search("refund policy", limit=5)
index.read("docs/billing.pdf#2.3", budget=800)
index.context("how long do refunds take?", budget=1500)["text"]
```

### Agent tools

```python
tools = index.tools(principal="user:ana")      # sees only what user:ana may see
openai_tools = [tool.to_openai() for tool in tools]   # or .to_anthropic()
by_name = {tool.name: tool for tool in tools}
result_text = by_name[call.name].call_json(call.arguments)
```

The tools are `search_documents`, `get_context`, `list_documents`,
`outline_document` and `read_document`. They are plain functions with JSON
schemas, so they work with any agent framework. The principal is bound when
the tools are made, never passed by the model.

### Plugins

Every part can be replaced. A Python plugin is any object with the right
method:

```python
class CsvParser:
    extensions = ("csv",)

    def parse(self, data: bytes, path: str) -> str:
        return csv_to_markdown(data)  # return Markdown; foliq finds the structure


class SentenceSplitter:
    def split(self, text: str) -> list[str]:
        return split_into_sentences(text)  # pieces of the text, in order


class QdrantSink:
    def apply(self, changes: list[dict]) -> None:
        ...  # every upsert and delete after each sync


index = foliq.Index(parsers=[CsvParser()], splitter=SentenceSplitter(), sinks=[QdrantSink()])
```

Rust plugins implement the traits in `foliq-core` (`Source`, `Parser`,
`Splitter`, `Store`, `Sink`). The `foliq` command runs only Rust code; Python
plugins work through the Python API.

## Formats

| Format             | Extensions               | How                                          |
|--------------------|--------------------------|----------------------------------------------|
| Markdown, text     | md, markdown, mdx, txt, rst | as is                                     |
| HTML               | html, htm, xhtml         | main content, headings, lists, tables        |
| Word               | docx                     | headings from styles, lists, tables          |
| PowerPoint         | pptx                     | one section per slide, with speaker notes    |
| Excel, OpenDocument| xlsx, xlsm, xls, ods     | one table per sheet                          |
| PDF                | pdf                      | text per page ([pdf_oxide](https://crates.io/crates/pdf_oxide), pure Rust) |

Scanned PDFs need OCR, which is not built in yet.

## Status

Version 0.1: folders, SQLite with full-text search, the formats above, access
rules, the CLI and the Python API. Next: embeddings and hybrid search, an MCP
server, cloud sources (S3, Azure), Postgres, and Rust plugins packaged as
Python wheels.

## Development

```console
uv sync                           # installs tools and builds the extension
uv run pytest                     # Python tests
scripts/cargo-test.sh --workspace # Rust tests
uv run prek install               # git hooks: ruff, ty, cargo fmt, clippy, tests
```

See [AGENTS.md](AGENTS.md) for the design and code style.

## License

MIT
