"""foliq benchmarks: parsing speed, extraction quality and search quality.

| File          | What it does                                                   |
|---------------|----------------------------------------------------------------|
| `__main__.py` | The command line: `python -m bench download / run / charts`     |
| `datasets.py` | Downloads the public corpora at pinned commits into `data/`     |
| `libraries.py`| One small `extract(path) -> text` function per library          |
| `worker.py`   | Runs one library over a list of files, in its own process       |
| `parsing.py`  | Speed and failures on real-world files, quality vs ground truth |
| `scoring.py`  | Text normalization and similarity scores                       |
| `search.py`   | Retrieval quality and speed on BEIR SciFact                     |
| `charts.py`   | Turns `results/*.json` into the SVG charts of the README        |
"""
