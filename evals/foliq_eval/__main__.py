"""The eval's command line. The eval itself runs with `inspect eval-set`.

uv run python -m foliq_eval download [MAX_PAPERS]   # papers and workbooks (~2.5 GB for 1000)
uv run python -m foliq_eval prepare [MAX_PAPERS]    # corpora, Docker images, foliq indexes
uv run python -m foliq_eval retrieval [MAX_PAPERS]  # what foliq returns for each question, no model
uv run python -m foliq_eval report LOG_FOLDER [OUT] # results tables (default OUT: results/<folder name>)
"""

import sys
import time
from pathlib import Path

from foliq_eval import corpora, report, retrieval, sandbox


def main() -> None:
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    arguments = sys.argv[2:]
    if command == "download":
        corpora.download(int(arguments[0]) if arguments else 1000)
    elif command == "prepare":
        names = corpora.build(int(arguments[0]) if arguments else 1000)
        sandbox.build_images()
        for name in names:
            started = time.perf_counter()
            sandbox.build_index(name)
            print(f"    indexed in {time.perf_counter() - started:.1f} s")
    elif command == "retrieval":
        retrieval.run(Path("results") / "retrieval", int(arguments[0]) if arguments else 1000)
    elif command == "report" and arguments:
        log_folder = Path(arguments[0])
        output = Path(arguments[1]) if len(arguments) > 1 else Path("results") / log_folder.name
        report.report(log_folder, output)
    else:
        print(__doc__)
        sys.exit(2)


if __name__ == "__main__":
    main()
