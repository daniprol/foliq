from pathlib import Path

import pytest

import foliq
from fixtures import make_all


@pytest.fixture(scope="session")
def documents_folder(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """One small document of each format (built once per test run)."""
    return make_all(tmp_path_factory.mktemp("documents") / "docs")


@pytest.fixture
def index(tmp_path: Path, documents_folder: Path) -> foliq.Index:
    """An index of the documents folder, synced."""
    index = foliq.Index(tmp_path / ".foliq")
    index.add(documents_folder)
    report = index.sync()
    assert report["failed"] == []
    return index
