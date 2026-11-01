# Notes for coding agents

foliq indexes documents (PDF, Word, PowerPoint, Excel, HTML, Markdown) and lets
AI agents search, read and cite them. It is a Rust engine with a CLI and a
Python API. Read this file before changing anything.

## Philosophy

### 1. Small core, everything pluggable (like pi)

foliq is a small engine with sockets, in the spirit of the pi coding agent:
the core does little, does it fast, and every part can be replaced.

- Each socket is one small Rust trait in `crates/foliq-core`: `Source`,
  `Parser`, `Splitter`, `Store`, `Sink` (and `Embedder`, used from 0.2).
  Built-in plugins implement the same traits as user plugins. The engine
  never special-cases a built-in.
- Every socket can be filled from Rust or from Python. Python classes plug in
  through `foliq.Index(parsers=..., splitter=..., sinks=...)`. Rust plugins
  depend on `foliq-core` only.
- The `foliq` command runs Rust code only. It never starts Python (like ruff
  or uv). Python plugins work through `import foliq` (like polars).
- Rust does the heavy work: parsing, splitting, token counting, storage,
  search, sync. Files are parsed in parallel and only changed files are
  re-processed.
- Start small. Add a feature when someone needs it, as a plugin if it can be
  one. Prefer one obvious way to do each thing. If a change makes the core
  bigger, ask whether it could be a plugin instead.

### 2. Clean, organized, consistent code that reads well the first time

- Names say what things are, in plain words: `document`, `chunks`,
  `heading_path`, `content_hash`, `is_visible_to`. No abbreviations
  (`doc`, `cfg`, `ctx`, `idx`) and no Rust jargon in names.
- **Rust must be readable by a Python developer who has never seen Rust.**
  Prefer simple, straight-line code over clever generics, macros, lifetimes
  or iterator chains. When a Rust feature would surprise a Python developer
  (`Option`, `?`, `Arc`, traits, `#[serde(...)]`), add a short comment,
  ideally comparing it to Python.
- Every file starts with a `//!` comment saying what it does. Every public
  item has a `///` comment. Comments explain *why*, not *what*.
- One job per file. Module docs list the files of a folder in a small table
  (see `crates/foliq/src/index/mod.rs`).
- Be consistent: same words for the same things everywhere (a "reference" is
  `collection/path#section`, a "citation" adds `(p4, L10-22)`, a "principal"
  is who is asking). The Python API, the CLI and the Rust API use the same
  names.
- The same rules apply to Python: small functions, clear names, no magic.

## Map

| Path                  | What it is                                                     |
|-----------------------|----------------------------------------------------------------|
| `crates/foliq-core`   | Types (`Document`, `Chunk`...) and plugin traits. Small, stable |
| `crates/foliq`        | The engine: parsers, folder source, splitter, SQLite store, sinks, `Index` |
| `crates/foliq-cli`    | The `foliq` command (`run(args)`), and `SKILL.md` for agents    |
| `src/*.rs`            | The Python extension `foliq._core` (PyO3)                       |
| `src/foliq`           | The Python package: tools for agents, plugin protocols, CLI entry |
| `tests`               | Python tests, with generated DOCX/PPTX/XLSX/PDF fixtures         |
| `benchmarks`          | Speed and quality against other libraries, on public corpora (own uv project) |

## Key ideas

- Every format becomes Markdown, the one canonical text of a document.
  Headings, tables and pages (`<!-- page N -->`) are found in that text, and
  everything (elements, sections, chunks) is a byte range of it.
- Sections are numbered like an outline (`2.3`). References and citations use
  those numbers, so an agent can go from a search hit to the full section.
- Access rules live on documents (`access`: a list of principals; empty means
  everyone). The store filters before ranking, so results never leak.
  `index.tools(principal=...)` binds the principal; it is never a tool argument.
- Sync is incremental: version (mtime and size), then content hash, then the
  parser and splitter fingerprints decide what is re-processed. A source that
  can't be listed deletes nothing.

## Workflow

- Tools: uv, maturin, ruff, ty, pytest, cargo fmt, clippy. Hooks run with prek
  (`prek.toml`): `uv run prek install` once.
- `uv run pytest` builds the extension and runs the Python tests.
  `scripts/cargo-test.sh --workspace` runs the Rust tests.
- Tests use real files in temporary folders, built by `tests/fixtures.py`.
  Avoid mocks.
- The version lives in one place: `[workspace.package]` in `Cargo.toml`.
