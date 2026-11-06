"""The citation checks, on answers written the ways agents write them."""

import yaml
from foliq_eval.scorers import cited_pages, cites_location, lines_naming
from foliq_eval.tasks import QUESTIONS


def test_pages_in_the_usual_citation_styles():
    assert cited_pages("- /docs/papers/2201.00006.pdf, page 5") == {5}
    assert cited_pages("docs/papers/2201.00006.pdf#1.1 (p5-6, L215-231)") == {5, 6}
    assert cited_pages("2201.00006.pdf, pages 4 to 6") == {4, 5, 6}
    assert cited_pages("2201.00006.pdf (pp. 4, 120)") == {4, 120}
    assert cited_pages("docs/papers/2201.00006.pdf#p6") == {6}
    # A file name is not a page.
    assert cited_pages("/docs/papers/2201.00006.pdf") == set()


def test_location_needs_the_right_file_and_page_or_sheet():
    pdf = {"file": "papers/2201.00006.pdf", "pages": [6]}
    assert cites_location("Sources:\n- /docs/papers/2201.00006.pdf, page 6", pdf)
    assert not cites_location("Sources:\n- /docs/papers/2201.00006.pdf, page 2", pdf)
    assert not cites_location("Sources:\n- /docs/papers/2201.00007.pdf, page 6", pdf)

    workbook = {"file": "spreadsheets/cheese_per_cap.xlsx", "sheet": "Cheese per cap since '95"}
    assert cites_location('- /docs/spreadsheets/cheese_per_cap.xlsx, sheet "Cheese per cap since 95"', workbook)
    assert not cites_location("- /docs/spreadsheets/cheese_per_cap.xlsx, sheet Contents", workbook)


def test_file_is_found_without_its_folder_or_extension():
    assert lines_naming("see 2201.00006 (page 5)", "papers/2201.00006.pdf")


def test_every_question_is_complete():
    questions = yaml.safe_load(QUESTIONS.read_text())
    assert len({question["id"] for question in questions}) == len(questions)
    for question in questions:
        assert question["group"] in {"single_fact", "tables", "multi_hop", "needle", "unanswerable"}
        assert question["split"] in {"dev", "heldout"}
        if question["group"] == "unanswerable":
            assert question["sources"] == []
            assert question["answer"].startswith("NOT FOUND")
        else:
            assert question["sources"]
            assert question["evidence"]
            for source in question["sources"]:
                assert ("pages" in source) != ("sheet" in source)
