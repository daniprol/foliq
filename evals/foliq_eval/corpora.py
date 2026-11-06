"""Download the public documents and build the folders the agents search.

Everything lands in `evals/data/` (ignored by git). Every file is pinned, so
every run sees exactly the same documents:

| Folder                      | What it is                                               |
|-----------------------------|----------------------------------------------------------|
| `data/arxiv/`               | arXiv papers 2201.00001 to 2201.01000, version 1 (~2.5 GB) |
| `data/spreadsheets/`        | 6 public government workbooks (TTB, USDA, Census, FSA)    |
| `data/corpora/library-N/`   | what the agent sees at `/docs`: N papers + the workbooks |
| `data/indexes/library-N/`   | the foliq index of that corpus (made by `prepare`)        |

The papers are the first 1,000 arXiv submissions of January 2022, so the
corpora are a plain, unpicked mix of fields. `library-10` holds the 10 papers
the questions are about. `library-100` and `library-1000` add 90 and 990
other papers, so the same questions can be asked of bigger folders.
"""

import os
import shutil
import time
import urllib.request
from pathlib import Path

DATA = Path(__file__).resolve().parent.parent / "data"

# Sizes of the paper folders. The questions are about the first 10 papers.
CORPUS_SIZES = [10, 100, 1000]

# The workbooks come from the TidyTuesday repository, which keeps copies of
# public data files. Pinned to one commit.
TIDYTUESDAY = "https://raw.githubusercontent.com/rfordatascience/tidytuesday/f422450f6519830f6ce0312e5294e3cf110c72ed"
SPREADSHEETS = {
    # US Treasury, Alcohol and Tobacco Tax and Trade Bureau: monthly beer report
    "ttb_monthly_stats_2019-12.xlsx": "data/2020/2020-03-31/ttb_monthly_stats_2019-12.xlsx",
    # The same bureau: beer by state, 2008-2019, three sheets
    "ttb_brewery_state_2008-2019.xlsx": "data/2020/2020-03-31/ttb_brewery_state_2008-2019.xlsx",
    # USDA Economic Research Service: cheese per person, two periods
    "cheese_per_cap.xlsx": "data/2019/2019-01-29/raw_datasets/cheese_per_cap.xlsx",
    # USDA: fluid milk sales by product, 1975-2017
    "fluidmilk.xlsx": "data/2019/2019-01-29/raw_datasets/fluidmilk.xlsx",
    # US Census Bureau, American Community Survey: bicycle commuting, ~1,470 small cities
    "supplemental-table1.xlsx": "data/2019/2019-11-05/supplemental-table1.xlsx",
    # Federal Student Aid: private collection agency statistics, one quarter
    "PCA_Report_FY16Q3.xlsx": "data/2019/2019-11-26/PCA_Report_FY16Q3.xlsx",
}


def corpus_name(size: int) -> str:
    """The name of the corpus with `size` papers, like `library-100`."""
    return f"library-{size}"


def corpus_folder(name: str) -> Path:
    return DATA / "corpora" / name


def index_folder(name: str) -> Path:
    return DATA / "indexes" / name


def arxiv_ids(count: int) -> list[str]:
    """The first `count` arXiv ids of January 2022: 2201.00001, 2201.00002..."""
    return [f"2201.{number:05d}" for number in range(1, count + 1)]


def download(max_papers: int = 1000) -> None:
    """Fetch every paper and workbook that is not on disk yet."""
    papers = DATA / "arxiv"
    papers.mkdir(parents=True, exist_ok=True)
    ids = arxiv_ids(max_papers)
    for number, arxiv_id in enumerate(ids, 1):
        target = papers / f"{arxiv_id}.pdf"
        if not target.exists():
            print(f"  [{number}/{len(ids)}] arXiv {arxiv_id}")
            fetch(f"https://arxiv.org/pdf/{arxiv_id}v1", target)
            time.sleep(1)  # arXiv asks automated clients to go slowly

    spreadsheets = DATA / "spreadsheets"
    spreadsheets.mkdir(parents=True, exist_ok=True)
    for name, path in SPREADSHEETS.items():
        target = spreadsheets / name
        if not target.exists():
            print(f"  {name}")
            fetch(f"{TIDYTUESDAY}/{path}", target)


def fetch(url: str, target: Path) -> None:
    """Download `url` to `target`, trying again on network errors."""
    request = urllib.request.Request(url, headers={"User-Agent": "foliq-evals"})
    for attempt in range(4):
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                data = response.read()
            break
        except OSError as error:
            if attempt == 3:
                raise
            print(f"    retrying after: {error}")
            time.sleep(2 ** (attempt + 1))
    # Write to a temporary name first, so a stopped download is never kept.
    partial = target.with_suffix(target.suffix + ".part")
    partial.write_bytes(data)
    partial.rename(target)


def build(max_papers: int = 1000) -> list[str]:
    """Build the corpus folders from the downloads. Returns their names."""
    names = []
    for size in CORPUS_SIZES:
        if size > max_papers:
            continue
        name = corpus_name(size)
        folder = corpus_folder(name)
        if folder.exists():
            shutil.rmtree(folder)
        (folder / "papers").mkdir(parents=True)
        (folder / "spreadsheets").mkdir(parents=True)
        for arxiv_id in arxiv_ids(size):
            link_or_copy(DATA / "arxiv" / f"{arxiv_id}.pdf", folder / "papers" / f"{arxiv_id}.pdf")
        for spreadsheet in SPREADSHEETS:
            link_or_copy(DATA / "spreadsheets" / spreadsheet, folder / "spreadsheets" / spreadsheet)
        names.append(name)
        print(f"  {name}: {size} papers, {len(SPREADSHEETS)} workbooks")
    return names


def link_or_copy(source: Path, target: Path) -> None:
    """Hard-link `source` to `target` (no extra disk space), or copy it."""
    try:
        os.link(source, target)
    except OSError:
        shutil.copy2(source, target)
