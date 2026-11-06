"""An Inspect eval of foliq with AI agents. See `evals/README.md`.

| File          | What it does                                               |
|---------------|------------------------------------------------------------|
| `__main__.py` | the `python -m foliq_eval` commands: download, prepare, report |
| `corpora.py`  | downloads the papers and workbooks, builds the corpus folders |
| `sandbox.py`  | Docker images, the prebuilt foliq indexes, compose files    |
| `prompts.py`  | what the agent is told in each setup                       |
| `tasks.py`    | the three setups as Inspect tasks, and their samples       |
| `scorers.py`  | citation, not-found and foliq-use checks                    |
| `report.py`   | turns Inspect logs into the results tables                  |
"""
