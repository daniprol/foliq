"""The benchmark command line.

uv run python -m bench download          # fetch the corpora (about 400 MB)
uv run python -m bench all               # run everything, then draw the charts
uv run python -m bench quality [pdf ...] # one benchmark, some formats
uv run python -m bench speed [pdf ...]
uv run python -m bench search
uv run python -m bench charts            # redraw the charts from results/*.json
"""

import os
import sys

QUALITY_FORMATS = ["pdf", "docx", "pptx", "xlsx", "html"]
SPEED_FORMATS = ["pdf", "docx", "pptx", "xlsx"]


def main() -> None:
    command = sys.argv[1] if len(sys.argv) > 1 else "all"
    formats = sys.argv[2:]
    # Searching is timed on one thread for every engine, foliq included.
    if command in ("search", "all"):
        os.environ.setdefault("RAYON_NUM_THREADS", "1")

    from bench import charts, datasets, parsing, search

    if command == "download":
        datasets.download_all()
    elif command == "quality":
        for file_format in formats or QUALITY_FORMATS:
            parsing.quality(file_format)
    elif command == "speed":
        for file_format in formats or SPEED_FORMATS:
            parsing.speed(file_format)
    elif command == "search":
        search.run()
    elif command == "charts":
        charts.draw_all()
    elif command == "all":
        for file_format in QUALITY_FORMATS:
            parsing.quality(file_format)
        search.run()
        for file_format in SPEED_FORMATS:
            parsing.speed(file_format)
        charts.draw_all()
    else:
        print(__doc__)
        sys.exit(2)


if __name__ == "__main__":
    main()
