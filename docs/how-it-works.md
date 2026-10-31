# How it works

```text
 Source ──► Parser ──► Splitter ──► Store ──► search, outline, read, context
 (files)   (→ Markdown) (→ chunks)  (SQLite)
                                       └────► Sinks (text mirror, your exports)
```

## One text per document

Every document becomes one Markdown text. Everything else points into it with
byte positions: headings, tables, pages, sections and chunks. So a chunk always
knows its lines, its pages and its heading path, and `read` can return exactly
one section.

## Sections

Sections are numbered from the headings, like an outline:

```text
docs/billing.md  Billing  (1210 tokens)
  1  Billing  (1210 tokens, L1)
    1.1  Refunds  (530 tokens, L12)
    1.2  Invoices  (640 tokens, L40)
```

Text before the first heading is section `0`.

## Chunks

The default splitter makes one chunk per section. A section bigger than 400
tokens (`max_tokens`) is cut at paragraph, list and sentence boundaries. A
chunk never crosses a heading, so each chunk has one heading path. Tokens are
counted with the `o200k_base` tokenizer (the one of recent OpenAI models).

## Search

Search uses SQLite's full-text index with BM25 ranking. Titles and headings
weigh more than body text. Common words ("the", "how") are ignored, and words
are stemmed ("refunds" finds "refund").

`context` searches, then grows each hit to its whole section when the section
fits in a third of the budget, skips overlapping passages, and stops at the
budget.

## Sync

A sync is cheap when little changed:

1. List the files with a version each (modified time and size).
2. Skip files with the same version, parser and splitter as last time.
3. Read the others. If the bytes have the same hash, only record the new version.
4. Parse and split what really changed, in parallel.
5. Delete documents whose files are gone, and send all changes to the sinks.

If a folder can't be listed (for example, a disk isn't mounted), its documents
are kept: nothing is deleted because of a failed listing.

## Files

```text
.foliq/
├── index.sqlite   collections, documents, chunks, full-text index
└── text/          Markdown copies of PDFs and Office files
```
