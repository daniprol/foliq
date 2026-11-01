"""Text normalization and similarity scores for extraction quality.

Libraries differ in markup (Markdown, plain text, tables with `|`), which is
not what we score. Every output and every reference goes through the same
`normalize` first, so only the words and their order count:

- `similarity`: 1 - normalized edit distance, the py-pdf/benchmarks metric.
  Rewards the right words in the right reading order.
- `word_f1`: overlap of the bags of words. Rewards finding the text at all.
"""

import re
from collections import Counter

from rapidfuzz.distance import Levenshtein

LIGATURES = {"ﬀ": "ff", "ﬁ": "fi", "ﬂ": "fl", "ﬃ": "ffi", "ﬄ": "ffl", "ﬅ": "ft", "ﬆ": "st"}

HTML_COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
IMAGE = re.compile(r"!\[[^\]]*\]\([^)]*\)")
LINK = re.compile(r"\[([^\]]*)\]\([^)]*\)")
TABLE_RULE = re.compile(r"^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$", re.MULTILINE)
HYPHEN_BREAK = re.compile(r"(\w)-\n(\w)")
MARKUP = re.compile(r"[#*_|`>~]+")
WHITESPACE = re.compile(r"\s+")
WORD = re.compile(r"\w+")


def normalize(text: str) -> str:
    """Plain lowercase words separated by single spaces."""
    for ligature, letters in LIGATURES.items():
        text = text.replace(ligature, letters)
    text = HTML_COMMENT.sub(" ", text)
    text = IMAGE.sub(" ", text)
    text = LINK.sub(r"\1", text)
    text = TABLE_RULE.sub(" ", text)
    text = HYPHEN_BREAK.sub(r"\1\2", text)
    text = MARKUP.sub(" ", text)
    return WHITESPACE.sub(" ", text).strip().lower()


def similarity(extracted: str, truth: str) -> float:
    """1.0 when the normalized texts are equal, 0.0 when nothing matches."""
    return Levenshtein.normalized_similarity(normalize(extracted), normalize(truth))


def word_f1(extracted: str, truth: str) -> float:
    found = Counter(WORD.findall(normalize(extracted)))
    expected = Counter(WORD.findall(normalize(truth)))
    common = sum((found & expected).values())
    if common == 0:
        return 0.0
    precision = common / sum(found.values())
    recall = common / sum(expected.values())
    return 2 * precision * recall / (precision + recall)
