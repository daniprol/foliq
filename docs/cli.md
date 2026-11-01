# Command line

The `foliq` command is written in Rust. It is the same whether you run it with
`uvx foliq`, install it with `uv tool install foliq`, or build it with
`cargo install --path crates/foliq-cli` from a clone.

## Commands

| Command                    | Does                                                    |
|----------------------------|---------------------------------------------------------|
| `foliq index [FOLDER]...`  | adds folders as collections, then syncs everything      |
| `foliq status`             | the collections and their sizes                         |
| `foliq remove NAME`        | removes a collection (your files are not touched)       |
| `foliq outline [REF]`      | lists the documents, or the sections of one document    |
| `foliq read REF`           | prints a document, a section or pages                   |
| `foliq search QUERY`       | ranked chunks with citations                            |
| `foliq context QUERY`      | the best passages, packed into a token budget           |
| `foliq skill`              | prints the skill file for agents                        |

## References

A reference names a document and, optionally, a part of it. Citations printed
by foliq are references too.

| Reference                 | Means                                      |
|---------------------------|--------------------------------------------|
| `docs/billing.pdf`        | the whole document (collection `docs`)     |
| `docs/billing.pdf#2.3`    | section 2.3, with its subsections          |
| `docs/billing.pdf#p4`     | page 4                                     |
| `docs/billing.pdf#p4-6`   | pages 4 to 6                               |
| `billing.pdf`             | works too, when only one document matches  |

## Options

| Option                    | Commands          | Does                                              |
|---------------------------|-------------------|---------------------------------------------------|
| `--budget N`, `-b`        | read, context     | the most tokens to print (context default: 2000)  |
| `--limit N`, `-n`         | search, context   | the most results (default: 10)                    |
| `--collection NAME`, `-c` | search, context   | only this collection (repeatable)                 |
| `--path PREFIX`           | search, context   | only paths that start with this                   |
| `--name NAME`             | index             | the collection name (default: the folder's name)  |
| `--access PRINCIPAL`      | index             | who may see the added folder (see [Access rules](access.md)) |
| `--principal PRINCIPAL`   | all               | answer as this principal                          |
| `--json`                  | all               | JSON output                                       |
| `--no-refresh`            | all               | don't check for changed files first               |
| `--index DIR`             | all               | the index folder (also `FOLIQ_INDEX`)             |

Without `--index`, foliq uses the nearest `.foliq` folder in the current
folder or a parent, like git finds `.git`. Otherwise it creates `./.foliq`.

## The text mirror

PDFs and Office files are also saved as Markdown in `.foliq/text/`, so `grep`,
`rg` and `cat` work on them:

```console
rg -n "termination" .foliq/text/
```
