"""Parsing benchmarks: speed and failures on real files, quality against ground truth.

Every library runs in its own process (see `worker.py`), one file at a time,
with a time limit per file. foliq runs as a user runs it: `Index.sync()` over
the whole folder, which also splits, counts tokens and writes the SQLite
index, so its time includes more work than a plain text extraction. It runs
once on one thread (a fair comparison with the others) and once on all cores.
"""

import json
import os
import select
import shutil
import subprocess
import sys
import time
from pathlib import Path

from bench import datasets, scoring
from bench.libraries import libraries_for

RESULTS = Path(__file__).resolve().parent.parent / "results"
WORK = datasets.DATA / "work"
TIME_LIMIT_SECONDS = 30
FOLIQ_TIME_LIMIT_SECONDS = 3600
FOLIQ = "foliq (Rust)"


def prepare_folder(name: str, files: list[Path]) -> Path:
    """Copy the files into one flat folder, so every library reads the same thing."""
    folder = WORK / name / "documents"
    if folder.exists():
        shutil.rmtree(folder)
    folder.mkdir(parents=True)
    for file in files:
        shutil.copyfile(file, folder / file.name)
    return folder


def run_library(name: str, files: list[Path], output: Path) -> dict[int, dict]:
    """Run one library over the files and return what happened to each one, by position."""
    output.mkdir(parents=True, exist_ok=True)
    files_json = output / "files.json"
    files_json.write_text(json.dumps([str(file) for file in files]))
    results: dict[int, dict] = {}
    first = 0
    while first < len(files):
        command = [sys.executable, "-m", "bench.worker", "library", name, str(files_json), str(output), str(first)]
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        current = first  # the file the worker is on (or about to warm up with)
        deadline = time.monotonic() + 3 * TIME_LIMIT_SECONDS
        while True:
            ready, _, _ = select.select([process.stdout], [], [], max(0.0, deadline - time.monotonic()))
            if not ready:  # the file took too long: kill the worker and go on after it
                process.kill()
                process.wait()
                results[current] = {"seconds": TIME_LIMIT_SECONDS, "error": "timeout"}
                first = current + 1
                break
            line = process.stdout.readline()
            if not line:  # the worker exited: either finished, or crashed on `current`
                process.wait()
                if current not in results and current < len(files):
                    results[current] = {"seconds": 0.0, "error": f"crash (exit code {process.returncode})"}
                first = current + 1 if current < len(files) else len(files)
                break
            if not line.startswith("@@bench "):
                continue
            message = json.loads(line.removeprefix("@@bench "))
            if "start" in message:
                current = message["start"]
                deadline = time.monotonic() + TIME_LIMIT_SECONDS
            elif "done" in message:
                results[message["done"]] = {"seconds": message["seconds"], "error": message["error"]}
                current = message["done"] + 1
        process.stdout.close()
    return results


def run_foliq(documents: Path, output: Path, threads: int | None) -> dict:
    """Index the folder with foliq; returns the sync time and the failed files."""
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)
    environment = dict(os.environ)
    environment.pop("RAYON_NUM_THREADS", None)  # None means all cores
    if threads is not None:
        environment["RAYON_NUM_THREADS"] = str(threads)
    command = [sys.executable, "-m", "bench.worker", "foliq", str(documents), str(output)]
    completed = subprocess.run(
        command, capture_output=True, text=True, env=environment, timeout=FOLIQ_TIME_LIMIT_SECONDS, check=False
    )
    for line in completed.stdout.splitlines():
        if line.startswith("@@bench "):
            return json.loads(line.removeprefix("@@bench "))
    raise RuntimeError(f"foliq worker failed: {completed.stderr[-2000:]}")


def summary(name: str, seconds: float, failed: int, files: int, megabytes: float) -> dict:
    return {
        "name": name,
        "seconds": round(seconds, 3),
        "failed": failed,
        "files_per_second": round(files / seconds, 2) if seconds else None,
        "megabytes_per_second": round(megabytes / seconds, 2) if seconds else None,
    }


def speed(file_format: str) -> dict:
    """Time every library on the real-world corpus of one format."""
    files = datasets.speed_corpus(file_format)
    documents = prepare_folder(f"speed-{file_format}", files)
    files = sorted(documents.iterdir())
    megabytes = sum(file.stat().st_size for file in files) / 1e6
    print(f"speed {file_format}: {len(files)} files, {megabytes:.0f} MB")
    rows = []
    for threads, label in [(1, FOLIQ), (None, f"foliq (Rust, {os.cpu_count()} threads)")]:
        result = run_foliq(documents, WORK / f"speed-{file_format}" / label, threads)
        rows.append(summary(label, result["seconds"], len(result["failed"]), len(files), megabytes))
        print(f"  {label}: {result['seconds']:.1f}s, {len(result['failed'])} failed")
    for library in libraries_for(file_format):
        results = run_library(library.name, files, WORK / f"speed-{file_format}" / library.folder)
        seconds = sum(result["seconds"] for result in results.values())
        failed = sum(1 for result in results.values() if result["error"])
        rows.append(summary(library.label, seconds, failed, len(files), megabytes))
        print(f"  {library.label}: {seconds:.1f}s, {failed} failed")
    report = {"format": file_format, "files": len(files), "megabytes": round(megabytes, 1), "libraries": rows}
    write_result(f"speed-{file_format}.json", report)
    return report


def quality(file_format: str) -> dict:
    """Score every library's text against the reference text of each document."""
    cases = datasets.quality_corpus(file_format)
    documents = prepare_folder(f"quality-{file_format}", [case.document for case in cases])
    truths = {case.document.name: case.truth.read_text(errors="replace") for case in cases}
    files = sorted(documents.iterdir())
    print(f"quality {file_format}: {len(files)} documents")
    outputs: dict[str, Path] = {}
    foliq_output = WORK / f"quality-{file_format}" / "foliq"
    run_foliq(documents, foliq_output, threads=None)
    outputs[FOLIQ] = foliq_output
    for library in libraries_for(file_format):
        library_output = WORK / f"quality-{file_format}" / library.folder
        run_library(library.name, files, library_output)
        outputs[library.label] = library_output
    rows = []
    for name, output in outputs.items():
        per_document = {}
        for file in files:
            extracted_file = output / f"{file.name}.txt"
            extracted = extracted_file.read_text(errors="replace") if extracted_file.exists() else ""
            per_document[file.name] = {
                "similarity": round(scoring.similarity(extracted, truths[file.name]), 4),
                "word_f1": round(scoring.word_f1(extracted, truths[file.name]), 4),
            }
        rows.append(
            {
                "name": name,
                "similarity": round(average(row["similarity"] for row in per_document.values()), 4),
                "word_f1": round(average(row["word_f1"] for row in per_document.values()), 4),
                "documents": per_document,
            }
        )
        print(f"  {name}: similarity {rows[-1]['similarity']:.3f}, word F1 {rows[-1]['word_f1']:.3f}")
    report = {"format": file_format, "documents": len(files), "libraries": rows}
    write_result(f"quality-{file_format}.json", report)
    return report


def average(values) -> float:
    values = list(values)
    return sum(values) / len(values) if values else 0.0


def write_result(name: str, report: dict) -> None:
    RESULTS.mkdir(exist_ok=True)
    (RESULTS / name).write_text(json.dumps(report, indent=2) + "\n")
