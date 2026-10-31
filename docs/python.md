# Python

```python
import foliq

index = foliq.Index(".foliq")      # created if it doesn't exist
index.add("docs")                  # a folder becomes a collection named "docs"
index.add("contracts", collection="legal", access=["group:legal"])
report = index.sync()              # reads new and changed files, in parallel
```

Results are plain dicts and lists, ready for a prompt or `json.dumps`. Their
keys are typed with `TypedDict`s, so your editor completes them.

## Methods

| Method                                   | Returns                                         |
|------------------------------------------|-------------------------------------------------|
| `add(folder, collection=, access=)`      | the collection                                  |
| `remove(collection)`                     | `False` if there was none                       |
| `sync()`                                 | counts of added, updated, unchanged, removed, and failures |
| `collections()`, `status()`              | the collections, with sizes for `status`        |
| `documents(collections=, path_prefix=)`  | documents without their text                    |
| `outline(reference)`                     | the sections, with numbers and token counts     |
| `read(reference, budget=)`               | a passage: text, citation, lines, pages         |
| `search(query, limit=, collections=, path_prefix=)` | chunks with citations and scores     |
| `context(query, budget=, limit=)`        | `text` with numbered, cited passages, and the passages |
| `chunks(collections=, path_prefix=)`     | every chunk, to export or embed                 |
| `tools(principal=)`                      | [tools for an AI agent](agent-tools.md)         |

Every reading method also takes `principal=` (see [Access rules](access.md)).

## Options

```python
foliq.Index(
    ".foliq",
    parsers=[],        # your parsers, tried before the built-in ones
    splitter=None,     # your splitter (default: one chunk per section)
    sinks=[],          # receive every change after a sync
    max_tokens=400,    # the biggest chunk of the default splitter
    mirror=True,       # write Markdown copies to .foliq/text
)
```

See [Plugins](plugins.md).

## Errors

| Exception        | When                                                    |
|------------------|---------------------------------------------------------|
| `NotFoundError`  | no such document, section, collection or folder         |
| `PluginError`    | a parser, splitter or sink failed                       |
| `FoliqError`     | the base class of both                                  |
| `ValueError`     | a bad argument, like an ambiguous reference             |

A file that fails to parse doesn't stop a sync: it is listed in
`report["failed"]`, and its previous version stays in the index.
