---
name: foliq
description: Search, read and cite local documents (PDF, Word, PowerPoint, Excel, HTML, Markdown) with the foliq CLI. Use when a question needs facts from the user's documents or files in an indexed folder.
---

# foliq: search and read documents

`foliq` indexes folders of documents and answers with small, cited pieces of
text, so you never need to load whole files. It keeps itself up to date: every
command re-checks changed files first.

## Workflow

1. **Find**: `foliq search "refund policy enterprise"` lists the best chunks.
   Each line starts with a citation like `docs/billing.pdf#2.3 (p4, L120-141)`.
2. **Get context**: `foliq context "how long do refunds take" --budget 1500`
   returns the best passages, whole sections when they fit, numbered `[1]`,
   `[2]`... and packed into the token budget. Often this is all you need.
3. **Read more**: the part of a citation before the space is a reference.
   - `foliq read docs/billing.pdf#2.3` reads section 2.3 (with subsections).
   - `foliq read docs/billing.pdf#p4-5` reads pages 4 to 5.
   - `foliq outline docs/billing.pdf` shows the sections with their sizes
     in tokens, to pick what to read.
   - Add `--budget N` to any `read` to cap its size.
4. **Cite** your answer with the citations foliq printed.

## Tips

- Search uses keywords (BM25). Use the specific words the document would use;
  try synonyms if the first search finds nothing.
- `foliq outline` with no reference lists every document.
- Narrow down with `--collection NAME` or `--path some/folder/`.
- Add `--json` for structured output.
- Text copies of PDFs and Office files are in `.foliq/text/`, so `grep -rn`
  works there too.
- If nothing is indexed yet: `foliq index ./docs` (any folder).
