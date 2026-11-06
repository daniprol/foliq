# foliq agent eval

Does foliq help an AI agent answer questions about a folder of documents?
This eval gives the same questions to the same model in three setups and
compares the answers, the citations and the cost. It runs on
[Inspect](https://inspect.aisi.org.uk), with each agent in its own Docker
container with no network.

| Setup            | The agent has                                                          |
|------------------|------------------------------------------------------------------------|
| `baseline`       | a shell with `grep`, `ripgrep`, `pdftotext`, and Python with pypdf, pdfplumber, openpyxl and pandas |
| `foliq_skill`    | the same, plus foliq, an index of the folder, and the [foliq skill](../skills/foliq/SKILL.md) listed the way coding agents list skills |
| `foliq_no_skill` | the same, plus foliq and the index, but nothing tells the agent about them |

`foliq_no_skill` answers the question "does an agent find foliq on its own?"
(the only hint is the `.foliq` folder next to the documents).

## Quickstart

You need Docker, uv and an API key for the model you test (for example
`ANTHROPIC_API_KEY` or `OPENAI_API_KEY`).

```console
$ cd evals
$ uv run python -m foliq_eval download    # 1,000 arXiv papers and 6 workbooks, ~2.2 GB, under an hour
$ uv run python -m foliq_eval prepare     # corpora, Docker images, foliq indexes (~10 min)
$ uv run python -m foliq_eval retrieval   # what foliq returns for each question, no model, ~30 s
```

A first, cheap agent run (5 questions per setup, once each):

```console
$ uv run inspect eval-set foliq_eval/tasks.py --log-dir logs/smoke \
    --model anthropic/claude-sonnet-5-5 --model-role grader=anthropic/claude-haiku-5-5 \
    --limit 5 --epochs 1
```

The full run: every dev question, three times, in all three setups.

```console
$ uv run inspect eval-set foliq_eval/tasks.py --log-dir logs/sonnet-5-5 \
    --model anthropic/claude-sonnet-5-5 --model-role grader=anthropic/claude-haiku-5-5
$ uv run python -m foliq_eval report logs/sonnet-5-5   # writes results/sonnet-5-5.md and .json
$ uv run inspect view --log-dir logs/sonnet-5-5         # every transcript, step by step
```

That is 56 samples × 3 runs × 3 setups = 504 agent runs. Expect a few
million tokens per setup; try `--limit` first to see the cost of your model.
Pin the exact model version, and keep the same grader across runs, so results
stay comparable. Smaller downloads: `download 100` and `prepare 100` skip the
1,000-paper corpus (and `-T max_papers=100` leaves it out of a run).

Task options go after `-T`: `-T split=heldout` runs the held-out questions,
`-T groups=needle,tables` runs some groups only.

## What is asked

50 questions, each checked by hand in the original file
([`questions.yaml`](questions.yaml)). They are in five groups, reported
separately because foliq may matter little on easy lookups and a lot on big
folders:

| Group          | n  | What it tests                                                       |
|----------------|----|---------------------------------------------------------------------|
| `single_fact`  | 10 | one answer, in one place of one document named in the question       |
| `tables`       | 10 | a number from a PDF table or a spreadsheet cell                      |
| `multi_hop`    | 10 | two passages, from two pages or two documents                        |
| `needle`       | 10 | no document is named; asked over 10, 100 and 1,000 papers            |
| `unanswerable` | 10 | the answer is not in the folder: the right reply is NOT FOUND         |

10 of the 50 (2 per group) are **held out**: they are never used while tuning
foliq or its skill, only for a final report.

The documents are public and pinned, so anyone can rerun the eval:

- **Papers**: arXiv 2201.00001 to 2201.01000 (version 1), the first 1,000
  submissions of January 2022, so the folders are a plain mix of fields and
  layouts. The questions are about the first 10; the others are distractors.
- **Workbooks**: six US government spreadsheets with real-world layouts
  (title rows, merged headers, footnotes, several sheets): TTB beer
  statistics, USDA cheese and milk, Census bicycle commuting for ~1,470
  cities, and Federal Student Aid collection agencies.

The agent sees a folder `/docs` with `papers/` and `spreadsheets/`. The
10-paper folder is used for every group, and the needle questions are also
asked of the 100- and 1,000-paper folders.

## What is measured

Per model, setup and question group (`report`):

| Column       | Meaning                                                               |
|--------------|-----------------------------------------------------------------------|
| Correct      | a grader model says the answer matches the reference answer           |
| Right file   | the answer cites the file the answer is in                            |
| Right page   | ...with the right page (PDF) or sheet (workbook)                      |
| NOT FOUND    | the answer says NOT FOUND: right for `unanswerable`, a false refusal elsewhere |
| Tokens       | input + output tokens of the agent (not the grader)                   |
| Tool calls   | shell commands the agent ran                                          |
| Time         | working time of the sample, in seconds                                |
| Used foliq   | the agent ran `foliq` at least once                                   |

Plus a table of the needle questions by folder size: the scale curve.

## How it is fair

- Same model, prompt, limits and documents in every setup. Only the tools
  differ. The containers have no network.
- Each question runs 3 times (`--epochs`), because agents vary between runs.
- The foliq index is built once per folder by `prepare`, like a user who
  indexed their folder before asking (it took 88 s for the 1,000 papers).
  Each sample gets its own copy, so samples cannot affect each other.
- The skill is shown the way Claude Code shows skills: a name, a one-line
  description and a file to read. Whether the agent reads it is part of the
  test.
- Answers must end with a `Sources:` list, in every setup, so citations can
  be checked the same way.

## The retrieval check

`retrieval` asks foliq every answerable dev question word for word, with no
model, and checks whether the fact survived parsing, whether `foliq search`
finds the right file and page, and whether `foliq context` returns the
answer. It is the cheap first step before an agent run. The latest output is
in [`results/retrieval.md`](results/retrieval.md).

## Files

| Path                      | What it is                                                  |
|---------------------------|-------------------------------------------------------------|
| `questions.yaml`          | the questions, answers, sources and evidence text            |
| `foliq_eval/tasks.py`     | the three setups as Inspect tasks                            |
| `foliq_eval/prompts.py`   | what the agent is told                                       |
| `foliq_eval/scorers.py`   | citation, not-found and foliq-use checks                      |
| `foliq_eval/corpora.py`   | downloads and corpus folders                                 |
| `foliq_eval/sandbox.py`   | Docker images, indexes and compose files                     |
| `foliq_eval/retrieval.py` | the retrieval check                                          |
| `foliq_eval/report.py`    | results tables from Inspect logs                             |
| `docker/Dockerfile`       | the `baseline` and `foliq` images                            |
| `results/`                | results worth keeping (logs stay in `logs/`, ignored by git)  |

`uv run pytest` runs the checks of the scorers and of `questions.yaml`.
