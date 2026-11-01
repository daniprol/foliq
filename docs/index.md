# foliq

**Search, read and cite your documents from AI agents.**

PDF, Word, PowerPoint, Excel, HTML and Markdown in one local index. A Rust
engine with a command line and a Python API. No server, no API key.

## Install

=== "Command line"

    ```console
    uvx foliq --help        # run without installing
    uv tool install foliq   # or install the `foliq` command
    ```

=== "Python"

    ```console
    uv add foliq            # or: pip install foliq
    ```

## Quickstart

```console
$ foliq index ./docs
synced: 214 added, 0 updated, 0 unchanged, 0 removed
  docs  214 documents, 1890 chunks, 512330 tokens  (/home/me/docs)

$ foliq search "refund enterprise customers"
docs/billing.pdf#2.3 (p4, L120-141)  Billing > Refunds > Enterprise  [7.81]
    Enterprise customers can ask for a refund within 30 days...

$ foliq read docs/billing.pdf#2.3
$ foliq context "how long do refunds take" --budget 1500
```

The same from Python:

```python
import foliq

index = foliq.Index(".foliq")
index.add("docs")
index.sync()

for result in index.search("refund enterprise customers", limit=5):
    print(result["citation"], result["title"])

print(index.context("how long do refunds take?", budget=1500)["text"])
```

## The idea

Agents are good at searching code: `grep`, then read the right lines. Documents
are harder. PDFs and Office files aren't text, and whole files don't fit in a
prompt. foliq gives agents the same workflow for documents:

| Step     | Command                         | What the agent gets                          |
|----------|---------------------------------|----------------------------------------------|
| Find     | `foliq search QUERY`            | ranked chunks, each with a citation           |
| Navigate | `foliq outline DOC`             | the sections, with their sizes in tokens      |
| Read     | `foliq read DOC#2.3`            | exactly one section (or pages: `#p4-6`)       |
| Answer   | `foliq context QUERY --budget N`| the best passages, cited, within N tokens     |

Every citation, like `docs/billing.pdf#2.3 (p4, L120-141)`, is also a reference
that `read` accepts. Every command checks for changed files first, so answers
are never stale.

## Give it to your agent

```console
foliq skill > .claude/skills/foliq/SKILL.md
```

The skill file teaches an agent the workflow above. For Python agents, see
[Agent tools](agent-tools.md).
