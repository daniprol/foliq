"""The `foliq` command (the same Rust code as the standalone binary)."""

import json
import subprocess
import sys
from pathlib import Path


def foliq(*arguments: str, cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "-m", "foliq", *arguments], cwd=cwd, capture_output=True, text=True, check=False
    )


def test_index_search_and_read(tmp_path: Path, documents_folder: Path) -> None:
    indexed = foliq("index", str(documents_folder), cwd=tmp_path)
    assert indexed.returncode == 0, indexed.stderr
    assert "6 added" in indexed.stdout

    search = foliq("search", "hotel", "price", "per", "night", "--json", cwd=tmp_path)
    results = json.loads(search.stdout)
    assert results[0]["path"] == "travel.docx"

    read = foliq("read", results[0]["citation"], cwd=tmp_path)
    assert read.stdout.startswith("## Hotels")


def test_errors_exit_with_one(tmp_path: Path) -> None:
    result = foliq("read", "missing.md", cwd=tmp_path)
    assert result.returncode == 1
    assert "not found" in result.stderr


def test_skill_is_printed(tmp_path: Path) -> None:
    assert foliq("skill", cwd=tmp_path).stdout.startswith("---\nname: foliq")
