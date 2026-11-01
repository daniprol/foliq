"""Download the public corpora used by the benchmarks, at pinned versions.

Everything lands in `benchmarks/data/` (ignored by git). Git repositories are
fetched with a sparse, shallow checkout of one commit, so a download is small
and every run sees exactly the same files.

| Name        | What it is                                             | Used for          |
|-------------|--------------------------------------------------------|-------------------|
| `pdfjs`     | Mozilla pdf.js test PDFs: 1,000+ real, often broken     | PDF speed, failures |
| `poi`       | Apache POI test files: real DOCX, PPTX, XLSX            | Office speed, failures |
| `docling`   | docling's test documents with reviewed Markdown output  | Office/HTML quality |
| `pypdf`     | py-pdf/benchmarks ground-truth text for 14 PDFs         | PDF quality       |
| `scifact`   | BEIR SciFact: 5,183 abstracts, 300 queries with answers | Search            |
"""

import shutil
import subprocess
import tarfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path

DATA = Path(__file__).resolve().parent.parent / "data"


@dataclass
class GitCorpus:
    """A few folders of one GitHub repository at one commit."""

    name: str
    url: str
    commit: str
    folders: list[str]

    @property
    def path(self) -> Path:
        return DATA / "repos" / self.name


GIT_CORPORA = [
    GitCorpus(
        "pdfjs",
        "https://github.com/mozilla/pdf.js.git",
        "0a4153349dc473ba950fc17941c5ca7eda43c1de",
        ["test/pdfs"],
    ),
    GitCorpus(
        "poi",
        "https://github.com/apache/poi.git",
        "6327f056ba3cfdae326a0261a236460592b80f5c",
        ["test-data/document", "test-data/slideshow", "test-data/spreadsheet"],
    ),
    GitCorpus(
        "docling",
        "https://github.com/docling-project/docling.git",
        "764b5d4dc22b03ea87afd4c2207f510300b27a5b",
        ["tests/data/docx", "tests/data/pptx", "tests/data/xlsx", "tests/data/html"],
    ),
    GitCorpus(
        "pypdf",
        "https://github.com/py-pdf/benchmarks.git",
        "d60dfb6709a6ab9b666267f15322f932ca1fc780",
        ["read/extraction-ground-truth"],
    ),
]

# The 14 PDFs whose text py-pdf/benchmarks checked by hand.
GROUND_TRUTH_PDFS = {
    "GeoTopo-book": "https://raw.githubusercontent.com/py-pdf/sample-files/main/009-pdflatex-geotopo/GeoTopo.pdf",
    **{
        name: f"https://arxiv.org/pdf/{name}.pdf"
        for name in [
            "2201.00214",
            "2201.00151",
            "1707.09725",
            "2201.00021",
            "2201.00037",
            "2201.00069",
            "2201.00178",
            "2201.00201",
            "1602.06541",
            "2201.00200",
            "2201.00022",
            "2201.00029",
            "1601.03642",
        ]
    },
}

SCIFACT_URL = "https://scifact.s3-us-west-2.amazonaws.com/release/latest/data.tar.gz"


def download_all() -> None:
    """Fetch every corpus that is not on disk yet."""
    for corpus in GIT_CORPORA:
        fetch_git(corpus)
    fetch_ground_truth_pdfs()
    fetch_scifact()


def fetch_git(corpus: GitCorpus) -> None:
    if (corpus.path / ".git").exists():
        print(f"  {corpus.name}: already there")
        return
    print(f"  {corpus.name}: fetching {corpus.url} @ {corpus.commit[:7]}")
    corpus.path.mkdir(parents=True, exist_ok=True)

    def git(*arguments: str) -> None:
        subprocess.run(["git", *arguments], cwd=corpus.path, check=True, capture_output=True)

    git("init", "--quiet")
    git("remote", "add", "origin", corpus.url)
    git("sparse-checkout", "set", "--no-cone", *[f"/{folder}/" for folder in corpus.folders])
    git("fetch", "--quiet", "--depth", "1", "--filter=blob:none", "origin", corpus.commit)
    git("checkout", "--quiet", corpus.commit)


def fetch_ground_truth_pdfs() -> None:
    folder = DATA / "pdf-ground-truth"
    folder.mkdir(parents=True, exist_ok=True)
    for name, url in GROUND_TRUTH_PDFS.items():
        target = folder / f"{name}.pdf"
        if target.exists():
            continue
        try:
            download(url, target)
        except OSError as error:
            # arXiv is blocked on some networks; the benchmark then uses the PDFs it has.
            print(f"  could not download {url}: {error}")


def fetch_scifact() -> None:
    folder = DATA / "scifact"
    if (folder / "corpus.jsonl").exists():
        print("  scifact: already there")
        return
    print("  scifact: fetching")
    archive = DATA / "scifact.tar.gz"
    download(SCIFACT_URL, archive)
    with tarfile.open(archive) as tar:
        tar.extractall(DATA / "scifact-raw", filter="data")
    shutil.move(DATA / "scifact-raw" / "data", folder)
    shutil.rmtree(DATA / "scifact-raw")
    archive.unlink()


def download(url: str, target: Path) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "foliq-benchmarks"})
    with urllib.request.urlopen(request, timeout=120) as response:
        data = response.read()
    target.write_bytes(data)


# Corpora, as lists of files ----------------------------------------------------


def speed_corpus(file_format: str) -> list[Path]:
    """Real-world files of one format, for speed and failure counts."""
    if file_format == "pdf":
        folder = DATA / "repos/pdfjs/test/pdfs"
    else:
        sub_folder = {"docx": "document", "pptx": "slideshow", "xlsx": "spreadsheet"}[file_format]
        folder = DATA / "repos/poi/test-data" / sub_folder
    return sorted(path for path in folder.iterdir() if path.suffix.lower() == f".{file_format}")


@dataclass
class QualityCase:
    """A document and the text it should produce."""

    document: Path
    truth: Path


def quality_corpus(file_format: str) -> list[QualityCase]:
    """Documents with a reference text, for extraction quality."""
    if file_format == "pdf":
        truth_folder = DATA / "repos/pypdf/read/extraction-ground-truth"
        documents = sorted((DATA / "pdf-ground-truth").glob("*.pdf"))
        return [QualityCase(document, truth_folder / f"{document.stem}.txt") for document in documents]
    folder = DATA / "repos/docling/tests/data" / file_format
    cases = []
    for document in sorted((folder / "sources").iterdir()):
        truth = folder / "groundtruth" / f"{document.name}.md"
        if truth.exists():
            cases.append(QualityCase(document, truth))
    return cases
