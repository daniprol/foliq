"""Every built-in format becomes Markdown with its structure."""

import foliq


def test_every_format_is_indexed(index: foliq.Index) -> None:
    paths = [document["path"] for document in index.documents()]
    assert paths == ["notes.md", "page.html", "prices.xlsx", "results.pptx", "travel.docx", "warranty.pdf"]


def test_docx_keeps_headings_lists_and_tables(index: foliq.Index) -> None:
    text = index.read("travel.docx")["text"]
    assert text.startswith("# Travel policy\n")
    assert "## Hotels" in text
    assert "- Book through the travel portal" in text
    assert "| Paris | 200 |" in text


def test_pptx_has_one_section_per_slide_with_notes(index: foliq.Index) -> None:
    outline = index.outline("results.pptx")
    assert [section["title"] for section in outline["sections"]] == ["Quarterly results", "Roadmap"]
    assert outline["pages"] == 2
    assert "Notes: Dates are not final." in index.read("results.pptx#2")["text"]


def test_xlsx_sheets_become_tables(index: foliq.Index) -> None:
    text = index.read("prices.xlsx")["text"]
    assert "| Pro plan | 25.5 |" in text
    # Separate blocks of cells are separate tables, and dates read as dates.
    assert "## A1:B4" in text
    assert "## D1:D2" in text
    assert "| 2024-03-15 |" in text
    assert "B4 (Price) = SUM(B2:B3)" in text
    assert "A3 (Ana): Most customers pick this one." in text


def test_pdf_pages_are_tracked(index: foliq.Index) -> None:
    page = index.read("warranty.pdf#p2")
    assert "returned within thirty days" in page["text"]
    assert "manufacturing defects" not in page["text"]
    assert (page["page_start"], page["page_end"]) == (2, 2)


def test_html_keeps_the_main_content(index: foliq.Index) -> None:
    text = index.read("page.html")["text"]
    assert "menu" not in text
    assert "## Shipping" in text
