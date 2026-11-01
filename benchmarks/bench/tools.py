"""Competitors that are not Python packages: pdftotext, pandoc and Apache Tika.

Each one is called from `libraries.py` like any other library, so `worker.py`
times it file by file in the same way.

| Competitor          | How it runs                                              |
|---------------------|----------------------------------------------------------|
| pdftotext, pandoc   | one process per file (start-up is a few milliseconds)    |
| Apache Tika (Java)  | one local Tika server per worker, one HTTP request per file |

Java takes seconds to start, which would swamp the time of a small file, so
the Tika server starts during the worker's untimed warm-up and stops with it.

`uv run python -m bench tools` downloads Tika; pdftotext and pandoc come from
the system package manager.
"""

import ctypes
import shutil
import signal
import subprocess
import time
import urllib.request
from pathlib import Path

from bench import datasets

TIKA_VERSION = "3.3.3"
TIKA_JAR = datasets.DATA / "tools" / f"tika-server-standard-{TIKA_VERSION}.jar"
TIKA_URL = (
    "https://repo1.maven.org/maven2/org/apache/tika/tika-server-standard/"
    f"{TIKA_VERSION}/tika-server-standard-{TIKA_VERSION}.jar"
)
TIKA_PORT = 9998
# Talk to the local Tika server directly, never through an HTTP proxy.
LOCAL = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def install() -> None:
    """Download Apache Tika and say which system tools are missing."""
    if shutil.which("java") and not TIKA_JAR.exists():
        print(f"  downloading Apache Tika {TIKA_VERSION}")
        TIKA_JAR.parent.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve(TIKA_URL, TIKA_JAR)
    for command in ["java", "pdftotext", "pandoc"]:
        if shutil.which(command) is None:
            print(f"  missing `{command}`: install it with your package manager (apt, brew)")


def is_installed(command: str) -> bool:
    """True when a tool can run."""
    if command == "tika":
        return shutil.which("java") is not None and TIKA_JAR.exists()
    return shutil.which(command) is not None


def run(command: list[str]) -> str:
    """Run a command on one file and return what it printed."""
    completed = subprocess.run(command, capture_output=True, check=False)
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.decode(errors="replace").strip()[-300:] or f"exit {completed.returncode}")
    return completed.stdout.decode(errors="replace")


def die_with_parent() -> None:
    """Ask Linux to kill this child process when the worker that started it dies.

    The parent kills a worker stuck on a file with SIGKILL, which gives it no
    chance to stop its own children, so they would otherwise live on.
    """
    prctl = getattr(ctypes.CDLL(None), "prctl", None)  # Linux only; elsewhere children may outlive a killed worker
    if prctl is not None:
        set_death_signal = 1  # PR_SET_PDEATHSIG
        prctl(set_death_signal, signal.SIGKILL)


_tika: subprocess.Popen | None = None


def tika(path: Path) -> str:
    """Send one file to a local Tika server and return its plain text."""
    global _tika  # one server per worker, like a module-level cache in any Python program
    if _tika is None or _tika.poll() is not None:
        _tika = subprocess.Popen(
            ["java", "-jar", str(TIKA_JAR), "--port", str(TIKA_PORT)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            preexec_fn=die_with_parent,
        )
        wait_for_tika()
    request = urllib.request.Request(
        f"http://localhost:{TIKA_PORT}/tika",
        data=path.read_bytes(),
        method="PUT",
        # Without a content type urllib sends a form type, and Tika returns nothing.
        headers={
            "Accept": "text/plain",
            "Content-Type": "application/octet-stream",
            "Content-Disposition": f'attachment; filename="{path.name}"',
        },
    )
    with LOCAL.open(request, timeout=60) as response:
        return response.read().decode(errors="replace")


def wait_for_tika() -> None:
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        try:
            with LOCAL.open(f"http://localhost:{TIKA_PORT}/version", timeout=2):
                return
        except OSError:
            time.sleep(0.5)
    raise RuntimeError("Tika server did not start")
