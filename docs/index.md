---
title: Get started
---

<p align="center">
  <img alt="foliq" src="assets/logo-wordmark-light.svg#only-light" height="64">
  <img alt="foliq" src="assets/logo-wordmark-dark.svg#only-dark" height="64">
</p>

<p align="center"><strong>ripgrep for documents, made for AI agents.</strong></p>

Search, read and cite PDF, Word, PowerPoint, Excel, HTML and Markdown files.
One local index, a Rust engine, a command line and a Python API. No server, no
API key.

Every search hit is a citation, like `docs/billing.pdf#2.3 (p4, L120-141)`,
and every citation is a reference that `read` accepts. So an agent finds a
passage, reads exactly the section around it, and quotes its source, all within
the token budget you give it.

## Install

foliq is on [PyPI](https://pypi.org/project/foliq/). The package has the
`foliq` command and the Python API, with prebuilt wheels for Linux, macOS and
Windows, and no Python dependencies.

=== "Command line"

    ```console
    uvx foliq --help        # run without installing
    uv tool install foliq   # or install the `foliq` command
    pipx install foliq      # or with pipx
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

=== "npx"

    ```console
    npx skills add daniprol/foliq
    ```

=== "pnpm"

    ```console
    pnpm dlx skills add daniprol/foliq
    ```

=== "bun"

    ```console
    bunx skills add daniprol/foliq
    ```

=== "foliq"

    ```console
    foliq skill --install            # .agents/skills/foliq, in this project
    foliq skill --install --claude   # also linked from .claude/skills/foliq
    foliq skill --install --global   # in your home folder, for all projects
    ```

The [skill](https://github.com/daniprol/foliq/blob/main/skills/foliq/SKILL.md)
teaches an agent the workflow above. For Python agents, see
[Agent tools](agent-tools.md).

## How fast

On public test corpora, foliq indexes Word files at 129 per second and
PowerPoint files at 148 per second on one thread (python-docx: 35, python-pptx:
50), fails on fewer broken PDFs than PyMuPDF and pypdf, and ranks as well as the
best BM25 libraries. See [Benchmarks](benchmarks.md).
