"""The Docker sandbox: images, the prebuilt foliq indexes, and compose files.

Each sample runs in a fresh container with no network. The corpus is mounted
read-only at `/docs`, so the three setups see the same files:

| Setup            | Image            | Extra in the container                         |
|------------------|------------------|------------------------------------------------|
| `baseline`       | `foliq-eval-baseline` | nothing                                   |
| `foliq_skill`    | `foliq-eval-foliq`    | `/docs/.foliq` (the index) and the skill file |
| `foliq_no_skill` | `foliq-eval-foliq`    | `/docs/.foliq` only                         |

The index is built once per corpus by `prepare`, like a user who indexed the
folder before asking. It is mounted read-only at `/opt/foliq-index` and copied
to `/docs/.foliq` when a sample starts, because foliq writes to its index and
samples must not share one.
"""

import subprocess
from pathlib import Path

from foliq_eval.corpora import DATA, corpus_folder, index_folder

REPOSITORY = Path(__file__).resolve().parents[2]
DOCKERFILE = REPOSITORY / "evals" / "docker" / "Dockerfile"

BASELINE_IMAGE = "foliq-eval-baseline"
FOLIQ_IMAGE = "foliq-eval-foliq"

# Run before each sample of a foliq setup: give it its own copy of the index.
COPY_INDEX = "cp -a /opt/foliq-index /docs/.foliq"


def build_images() -> None:
    """Build both images (foliq is compiled from this checkout)."""
    for target, image in [("baseline", BASELINE_IMAGE), ("foliq", FOLIQ_IMAGE)]:
        print(f"  docker image {image}")
        run(["docker", "build", "--file", str(DOCKERFILE), "--target", target, "--tag", image, str(REPOSITORY)])


def build_index(name: str) -> None:
    """Index corpus `name` with the foliq image, at the paths the agent sees."""
    index = index_folder(name)
    index.mkdir(parents=True, exist_ok=True)
    corpus = corpus_folder(name)
    command = ["docker", "run", "--rm", "--network", "none"]
    command += mounts(corpus)
    command += ["--volume", f"{index}:/docs/.foliq", FOLIQ_IMAGE, "foliq", "index", "/docs", "--name", "docs"]
    print(f"  foliq index of {name}")
    run(command)


def mounts(corpus: Path) -> list[str]:
    """`docker run` arguments that mount each folder of `corpus` under /docs.

    The subfolders are mounted one by one (not `corpus` itself at /docs), so
    /docs stays writable and the index can be copied next to them.
    """
    arguments = []
    for folder in sorted(corpus.iterdir()):
        arguments += ["--volume", f"{folder}:/docs/{folder.name}:ro"]
    return arguments


def compose_file(setup: str, name: str) -> Path:
    """Write (once) and return the compose file for `setup` on corpus `name`.

    Inspect starts one container per sample from this file.
    """
    uses_foliq = setup != "baseline"
    corpus = corpus_folder(name)
    volumes = [f"{folder}:/docs/{folder.name}:ro" for folder in sorted(corpus.iterdir())]
    if uses_foliq:
        volumes.append(f"{index_folder(name)}:/opt/foliq-index:ro")
    lines = [
        "services:",
        "  default:",
        f"    image: {FOLIQ_IMAGE if uses_foliq else BASELINE_IMAGE}",
        "    x-local: true  # built by `prepare`, never pulled",
        "    init: true",
        "    command: tail -f /dev/null",
        "    network_mode: none",
        "    working_dir: /docs",
        "    volumes:",
        *[f'      - "{volume}"' for volume in volumes],
    ]
    path = DATA / "compose" / f"{'foliq' if uses_foliq else 'baseline'}-{name}.yaml"
    path.parent.mkdir(parents=True, exist_ok=True)
    text = "\n".join(lines) + "\n"
    if not path.exists() or path.read_text() != text:
        path.write_text(text)
    return path


def run(command: list[str]) -> None:
    subprocess.run(command, check=True)
