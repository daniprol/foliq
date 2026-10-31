# Formats

Every format becomes Markdown. foliq then finds the headings, tables and pages
in that Markdown, so every format gets the same outline, sections and
citations.

| Format              | Extensions                  | How                                                         |
|---------------------|-----------------------------|-------------------------------------------------------------|
| Markdown, text      | md, markdown, mdx, txt, text, rst | as is (YAML front matter becomes metadata)            |
| HTML                | html, htm, xhtml            | the main content, with headings, lists, tables and code     |
| Word                | docx                        | headings from styles, lists, tables                         |
| PowerPoint          | pptx                        | one section per slide, with tables and speaker notes        |
| Excel, OpenDocument | xlsx, xlsm, xls, ods        | one section per sheet, as a table                           |
| PDF                 | pdf                         | text per page, headings from font sizes ([pdf_oxide](https://crates.io/crates/pdf_oxide)) |

All parsers are written in Rust and run in parallel, one file per CPU core.

Pages and slides are marked with `<!-- page N -->` and `<!-- slide N -->`
lines, so a citation can say `p4` and `read doc.pdf#p4` works.

## Not built in (yet)

- Scanned PDFs and images need OCR.
- Old binary Office files (`.doc`, `.ppt`).

Any format can be added with a [parser plugin](plugins.md), in Python or Rust.
A parser only has to return Markdown.
