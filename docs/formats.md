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
| Excel, OpenDocument | xlsx, xlsm, xlsb, xls, ods  | one section per sheet, a table per block of cells ([below](#spreadsheets)) |
| PDF                 | pdf                         | text per page, headings from font sizes ([pdf_oxide](https://crates.io/crates/pdf_oxide)) |

All parsers are written in Rust and run in parallel, one file per CPU core.

Pages and slides are marked with `<!-- page N -->` and `<!-- slide N -->`
lines, so a citation can say `p4` and `read doc.pdf#p4` works.

## Spreadsheets

foliq reads what a person wrote or Excel computed, and ignores how the sheet
looks and how it runs:

- **Read**: values (dates as `2024-03-15`, percentages as `12%`), tables,
  formulas, links, named ranges, comments and notes, text boxes, pictures
  (with their alt text) and charts (title and series). Hidden sheets are read
  and marked `(hidden)`.
- **Ignored**: macros (never run or read), recalculation (the values Excel
  saved are used), styles, colours, and number formats other than dates and
  percentages.

Each sheet is a `#` section. Cells that touch form one table, so a title, a
table and a side table on the same sheet stay apart. When a sheet has several
tables, each gets a `##` heading with its range, like `## Orders (A1:F1201)`.
Big tables are written in blocks of rows (`### Rows 2–41`) that repeat the
header, so every chunk says what its columns mean.

```markdown
# Prices

4 rows × 4 columns. Tables: A1:B4, D1:D2.

## A1:B4

| Product | Price |
| --- | --- |
| Basic plan | 10 |
| Pro plan | 25.5 |
| Total | 35.5 |

## D1:D2

| Launch |
| --- |
| 2024-03-15 |

## Formulas

- B4 (Price) = SUM(B2:B3)

## Notes

- A3 (Ana): Most customers pick this one.
```

Pictures, charts and comments are read from XLSX and XLSM. XLS, XLSB and ODS
give values and formulas.

## Not built in (yet)

- Scanned PDFs and images need OCR.
- Old binary Office files (`.doc`, `.ppt`).
- Password-protected spreadsheets.

Any format can be added with a [parser plugin](plugins.md), in Python or Rust.
A parser only has to return Markdown.
