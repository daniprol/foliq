"""Types of the Rust extension `foliq._core`.

Results are plain dicts and lists; these `TypedDict`s describe their keys.
"""

from os import PathLike
from typing import Any, Literal, TypedDict

from foliq.plugins import Parser, Sink, Splitter
from foliq.tools import Tool

__version__: str
SKILL: str  # the agent skill file, as printed by `foliq skill`

class FoliqError(Exception): ...
class NotFoundError(FoliqError): ...
class PluginError(FoliqError): ...

class Collection(TypedDict):
    name: str
    source: Literal["folder"]
    location: str
    access: list[str]

class CollectionStatus(TypedDict):
    collection: Collection
    documents: int
    chunks: int
    tokens: int

class SyncFailure(TypedDict):
    collection: str
    path: str  # empty when the whole collection failed
    message: str

class SyncReport(TypedDict):
    added: int
    updated: int
    unchanged: int
    removed: int
    failed: list[SyncFailure]

class DocumentInfo(TypedDict):
    id: str
    collection: str
    path: str
    title: str
    token_count: int
    chunk_count: int
    metadata: dict[str, Any]

class OutlineSection(TypedDict):
    number: str
    title: str
    level: int
    token_count: int
    page: int | None
    line: int

class Outline(TypedDict):
    collection: str
    path: str
    title: str
    token_count: int
    pages: int | None
    sections: list[OutlineSection]

class Passage(TypedDict):
    citation: str
    collection: str
    path: str
    title: str
    section: str | None
    heading_path: list[str]
    line_start: int
    line_end: int
    page_start: int | None
    page_end: int | None
    text: str
    token_count: int
    truncated: bool

class SearchResult(TypedDict):
    citation: str
    collection: str
    path: str
    title: str
    section: str
    heading_path: list[str]
    line_start: int
    line_end: int
    page_start: int | None
    page_end: int | None
    score: float
    text: str
    token_count: int

class Context(TypedDict):
    text: str
    token_count: int
    passages: list[Passage]

class Chunk(TypedDict):
    id: str
    document_id: str
    ordinal: int
    start: int  # byte offsets in the document's text
    end: int
    line_start: int
    line_end: int
    page_start: int | None
    page_end: int | None
    section: str
    heading_path: list[str]
    token_count: int
    text: str
    content_key: str  # a hash of the text: equal texts have equal keys

Principal = str | list[str] | None

class Index:
    """A searchable index of documents, saved in one folder (`.foliq` by default)."""

    def __init__(
        self,
        path: str | PathLike[str] = ".foliq",
        *,
        parsers: list[Parser] = ...,
        splitter: Splitter | None = None,
        sinks: list[Sink] = ...,
        max_tokens: int = 400,
        mirror: bool = True,
    ) -> None: ...
    @property
    def path(self) -> str: ...
    def add(
        self, folder: str | PathLike[str], *, collection: str | None = None, access: list[str] = ...
    ) -> Collection: ...
    def remove(self, collection: str) -> bool: ...
    def sync(self) -> SyncReport: ...
    def collections(self) -> list[Collection]: ...
    def status(self) -> list[CollectionStatus]: ...
    def documents(
        self, *, collections: list[str] = ..., path_prefix: str | None = None, principal: Principal = None
    ) -> list[DocumentInfo]: ...
    def outline(self, reference: str, *, principal: Principal = None) -> Outline: ...
    def read(self, reference: str, *, budget: int | None = None, principal: Principal = None) -> Passage: ...
    def search(
        self,
        query: str,
        *,
        limit: int = 10,
        collections: list[str] = ...,
        path_prefix: str | None = None,
        principal: Principal = None,
    ) -> list[SearchResult]: ...
    def context(
        self,
        query: str,
        *,
        budget: int = 2000,
        limit: int = 10,
        collections: list[str] = ...,
        path_prefix: str | None = None,
        principal: Principal = None,
    ) -> Context: ...
    def chunks(
        self, *, collections: list[str] = ..., path_prefix: str | None = None, principal: Principal = None
    ) -> list[Chunk]: ...
    def tools(self, *, principal: Principal = None) -> list[Tool]: ...

def run_cli(argv: list[str]) -> int: ...
