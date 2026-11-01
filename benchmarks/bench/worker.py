"""Runs one library over a list of files, in its own process.

The parent (`parsing.py`) starts this as `python -m bench.worker ...` and reads
its progress from stdout. A file that hangs or crashes the process only costs
that file: the parent kills the worker and starts a new one after it. That is
the only way to stop a library stuck in C code.

Two modes:

    python -m bench.worker library <name> <files.json> <output folder> <first>
    python -m bench.worker foliq <documents folder> <output folder>

Progress lines start with `@@bench `, so anything a library prints is ignored.
"""

import contextlib
import json
import os
import sys
import time
from pathlib import Path

PREFIX = "@@bench "


def main() -> None:
    # Keep the real stdout for progress lines; libraries that print go to stderr.
    progress = sys.stdout
    sys.stdout = sys.stderr
    mode = sys.argv[1]
    if mode == "library":
        run_library(progress, sys.argv[2], Path(sys.argv[3]), Path(sys.argv[4]), int(sys.argv[5]))
    elif mode == "foliq":
        run_foliq(progress, Path(sys.argv[2]), Path(sys.argv[3]))


def report(progress, message: dict) -> None:
    progress.write(PREFIX + json.dumps(message) + "\n")
    progress.flush()


def run_library(progress, name: str, files_json: Path, output: Path, first: int) -> None:
    from bench.libraries import BY_NAME

    library = BY_NAME[name]
    files = [Path(path) for path in json.loads(files_json.read_text())]
    # Warm up once (imports, lazy initialization), so they are not timed.
    report(progress, {"warmup": True})
    with contextlib.suppress(Exception):
        library.extract(files[first])
    for index in range(first, len(files)):
        report(progress, {"start": index})
        started = time.perf_counter()
        error = None
        text = ""
        try:
            text = library.extract(files[index]) or ""
        except Exception as exception:  # any library error counts as a failed file
            error = f"{type(exception).__name__}: {exception}"[:300]
        seconds = time.perf_counter() - started
        (output / f"{files[index].name}.txt").write_text(text, errors="replace")
        report(progress, {"done": index, "seconds": seconds, "error": error, "characters": len(text)})


def run_foliq(progress, documents: Path, output: Path) -> None:
    """Index a whole folder with foliq and keep the Markdown of every document."""
    import foliq

    index_folder = output / ".foliq"
    index = foliq.Index(index_folder)
    index.add(documents, collection="corpus")
    started = time.perf_counter()
    sync_report = index.sync()
    seconds = time.perf_counter() - started
    failed = {Path(failure["path"]).name: failure["message"][:300] for failure in sync_report["failed"]}
    # The text mirror holds the Markdown of each document (`<name>.md`); plain
    # text formats are not mirrored, so read those from the source.
    for document in documents.iterdir():
        mirrored = index_folder / "text" / "corpus" / f"{document.name}.md"
        text = mirrored.read_text(errors="replace") if mirrored.exists() else ""
        (output / f"{document.name}.txt").write_text(text, errors="replace")
    report(
        progress,
        {"seconds": seconds, "failed": failed, "threads": os.environ.get("RAYON_NUM_THREADS", "all")},
    )


if __name__ == "__main__":
    main()
