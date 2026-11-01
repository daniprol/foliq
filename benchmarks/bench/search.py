"""Search benchmark: BM25 retrieval on BEIR SciFact.

SciFact has 5,183 scientific abstracts and 300 claims; each claim lists the
abstracts that support or refute it (the BEIR test set). Every engine indexes
"title + abstract" and returns its top 100 documents per claim. We report:

- nDCG@10 and Recall@100, the standard BEIR metrics,
- indexing time and median time of a top-10 query, on one thread.

foliq indexes one Markdown file per abstract and returns chunks, so a
document's score is its best chunk's score.
"""

import functools
import json
import math
import re
import shutil
import sqlite3
import statistics
import time
from collections.abc import Callable
from pathlib import Path

from bench import datasets
from bench.parsing import WORK, write_result

TOP_K = 100
STOP_WORDS: set[str] = set()  # filled from bm25s's English list, shared by the Python engines
WORD = re.compile(r"\w+")

Search = Callable[..., list[str]]  # search(query, k=100) -> the best k document ids


def load_scifact() -> tuple[dict[str, tuple[str, str]], dict[str, str], dict[str, set[str]]]:
    folder = datasets.DATA / "scifact"
    documents = {}
    for line in (folder / "corpus.jsonl").read_text().splitlines():
        row = json.loads(line)
        documents[str(row["doc_id"])] = (row["title"], " ".join(row["abstract"]))
    queries, relevant = {}, {}
    for line in (folder / "claims_dev.jsonl").read_text().splitlines():
        row = json.loads(line)
        queries[str(row["id"])] = row["claim"]
        relevant[str(row["id"])] = {str(doc_id) for doc_id in row["cited_doc_ids"]}
    return documents, queries, relevant


# Metrics ------------------------------------------------------------------------


def ndcg_at_10(ranking: list[str], relevant: set[str]) -> float:
    gain = sum(1 / math.log2(rank + 2) for rank, doc_id in enumerate(ranking[:10]) if doc_id in relevant)
    ideal = sum(1 / math.log2(rank + 2) for rank in range(min(len(relevant), 10)))
    return gain / ideal


def recall_at_100(ranking: list[str], relevant: set[str]) -> float:
    return len(set(ranking[:TOP_K]) & relevant) / len(relevant)


# Engines: each one builds an index and returns a `search(query, k) -> [doc ids]` -


@functools.cache
def english_stemmer():
    import Stemmer

    return Stemmer.Stemmer("english")


def english_tokens(text: str) -> list[str]:
    """Lowercase words, minus stop words, stemmed (the same tokens bm25s uses)."""
    words = [word for word in WORD.findall(text.lower()) if word not in STOP_WORDS]
    return english_stemmer().stemWords(words)


FOLIQ_FOLDER = WORK / "search" / "foliq"


def write_markdown_files(documents: dict[str, tuple[str, str]]) -> None:
    """One Markdown file per abstract, for foliq (written before the clock starts)."""
    if FOLIQ_FOLDER.exists():
        shutil.rmtree(FOLIQ_FOLDER)
    (FOLIQ_FOLDER / "documents").mkdir(parents=True)
    for doc_id, (title, abstract) in documents.items():
        (FOLIQ_FOLDER / "documents" / f"{doc_id}.md").write_text(f"# {title}\n\n{abstract}\n")


def build_foliq(documents: dict[str, tuple[str, str]]) -> Search:
    import foliq

    index = foliq.Index(FOLIQ_FOLDER / ".foliq", mirror=False)
    index.add(FOLIQ_FOLDER / "documents", collection="scifact")
    index.sync()

    def search(query: str, k: int = TOP_K) -> list[str]:
        ranking: list[str] = []
        for hit in index.search(query, limit=3 * k):
            doc_id = Path(hit["path"]).stem
            if doc_id not in ranking:
                ranking.append(doc_id)
        return ranking[:k]

    return search


def build_sqlite_fts5(documents: dict[str, tuple[str, str]]) -> Search:
    """Plain SQLite FTS5 with the porter stemmer: the engine under foliq, without foliq."""
    connection = sqlite3.connect(":memory:")
    connection.execute(
        "CREATE VIRTUAL TABLE docs USING fts5(doc_id UNINDEXED, title, body, tokenize='porter unicode61')"
    )
    connection.executemany("INSERT INTO docs VALUES (?, ?, ?)", [(i, t, a) for i, (t, a) in documents.items()])
    connection.commit()

    def search(query: str, k: int = TOP_K) -> list[str]:
        words = [word for word in WORD.findall(query.lower()) if word not in STOP_WORDS]
        match = " OR ".join(f'"{word}"' for word in words)
        rows = connection.execute("SELECT doc_id FROM docs WHERE docs MATCH ? ORDER BY bm25(docs) LIMIT ?", (match, k))
        return [row[0] for row in rows]

    return search


def build_bm25s(documents: dict[str, tuple[str, str]]) -> Search:
    import bm25s

    doc_ids = list(documents)
    stemmer = english_stemmer()
    corpus = [f"{title} {abstract}" for title, abstract in documents.values()]
    retriever = bm25s.BM25()
    retriever.index(bm25s.tokenize(corpus, stopwords="en", stemmer=stemmer, show_progress=False), show_progress=False)

    def search(query: str, k: int = TOP_K) -> list[str]:
        tokens = bm25s.tokenize([query], stopwords="en", stemmer=stemmer, show_progress=False)
        results, _ = retriever.retrieve(tokens, k=k, show_progress=False, n_threads=1)
        return [doc_ids[position] for position in results[0]]

    return search


def build_rank_bm25(documents: dict[str, tuple[str, str]]) -> Search:
    from rank_bm25 import BM25Okapi

    doc_ids = list(documents)
    engine = BM25Okapi([english_tokens(f"{title} {abstract}") for title, abstract in documents.values()])

    def search(query: str, k: int = TOP_K) -> list[str]:
        scores = engine.get_scores(english_tokens(query))
        best = sorted(range(len(scores)), key=lambda position: -scores[position])[:k]
        return [doc_ids[position] for position in best]

    return search


def build_tantivy(documents: dict[str, tuple[str, str]]) -> Search:
    import tantivy

    builder = tantivy.SchemaBuilder()
    builder.add_text_field("doc_id", stored=True, tokenizer_name="raw")
    builder.add_text_field("title", tokenizer_name="en_stem")
    builder.add_text_field("body", tokenizer_name="en_stem")
    schema = builder.build()
    index = tantivy.Index(schema)
    writer = index.writer(heap_size=200_000_000, num_threads=1)
    for doc_id, (title, abstract) in documents.items():
        writer.add_document(tantivy.Document(doc_id=doc_id, title=title, body=abstract))
    writer.commit()
    writer.wait_merging_threads()
    index.reload()
    searcher = index.searcher()

    def search(query: str, k: int = TOP_K) -> list[str]:
        words = [word for word in WORD.findall(query.lower()) if word not in STOP_WORDS]
        parsed = index.parse_query(" ".join(words), ["title", "body"])
        hits = searcher.search(parsed, k).hits
        return [searcher.doc(address)["doc_id"][0] for _, address in hits]

    return search


ENGINES = {
    "foliq": build_foliq,
    "SQLite FTS5": build_sqlite_fts5,
    "bm25s": build_bm25s,
    "tantivy": build_tantivy,
    "rank-bm25": build_rank_bm25,
}


def run() -> dict:
    import bm25s.stopwords

    STOP_WORDS.update(bm25s.stopwords.STOPWORDS_EN)
    documents, queries, relevant = load_scifact()
    write_markdown_files(documents)
    print(f"search: SciFact, {len(documents)} documents, {len(queries)} queries")
    rows = []
    for name, build in ENGINES.items():
        started = time.perf_counter()
        search = build(documents)
        index_seconds = time.perf_counter() - started
        ndcg, recall, latencies = [], [], []
        for query_id, query in queries.items():
            ranking = search(query)
            ndcg.append(ndcg_at_10(ranking, relevant[query_id]))
            recall.append(recall_at_100(ranking, relevant[query_id]))
        # Query time is measured on its own, for a top-10 search (what an agent asks for).
        for query in queries.values():
            started = time.perf_counter()
            search(query, k=10)
            latencies.append(time.perf_counter() - started)
        row = {
            "name": name,
            "ndcg_at_10": round(statistics.mean(ndcg), 4),
            "recall_at_100": round(statistics.mean(recall), 4),
            "index_seconds": round(index_seconds, 3),
            "median_query_milliseconds": round(1000 * statistics.median(latencies), 3),
        }
        rows.append(row)
        print(
            f"  {name}: nDCG@10 {row['ndcg_at_10']:.3f}, R@100 {row['recall_at_100']:.3f}, "
            f"index {index_seconds:.2f}s, query {row['median_query_milliseconds']:.2f} ms"
        )
    report = {"dataset": "BEIR SciFact", "documents": len(documents), "queries": len(queries), "engines": rows}
    write_result("search-scifact.json", report)
    return report
