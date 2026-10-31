"""Tools that let an AI agent search and read the index.

`index.tools(principal=...)` returns a list of `Tool`s. Each one is a plain
function plus a JSON schema, so it works with any agent framework:

```python
tools = index.tools(principal="user:ana")
openai_tools = [tool.to_openai() for tool in tools]  # or tool.to_anthropic()
by_name = {tool.name: tool for tool in tools}
answer = by_name[call.name].call_json(call.arguments)  # always returns text for the model
```

The principal is bound when the tools are made. It is never a tool argument,
so the model can't ask to see documents its user may not see.

Tools return text, not dicts: it is what the model reads best, and every
passage starts with a citation the model can quote or pass to `read_document`.
"""

import json
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

from foliq._core import FoliqError, Index


@dataclass(frozen=True)
class Tool:
    """A function an AI model can call, with its JSON schema."""

    name: str
    description: str
    parameters: dict[str, Any]  # JSON schema of the arguments
    function: Callable[..., str]

    def __call__(self, **arguments: Any) -> str:
        return self.function(**arguments)

    def call_json(self, arguments: str | dict[str, Any]) -> str:
        """Calls the tool with the arguments a model sent (a JSON string or a dict).

        Errors (a missing document, a bad reference) come back as text, so the
        model can read them and try again.
        """
        try:
            parsed = json.loads(arguments or "{}") if isinstance(arguments, str) else arguments
            return self.function(**parsed)
        except (FoliqError, ValueError, TypeError) as error:
            return f"Error: {error}"

    def to_openai(self) -> dict[str, Any]:
        """The OpenAI Responses API format."""
        return {"type": "function", "name": self.name, "description": self.description, "parameters": self.parameters}

    def to_openai_chat(self) -> dict[str, Any]:
        """The OpenAI Chat Completions format."""
        return {
            "type": "function",
            "function": {"name": self.name, "description": self.description, "parameters": self.parameters},
        }

    def to_anthropic(self) -> dict[str, Any]:
        """The Anthropic Messages API format."""
        return {"name": self.name, "description": self.description, "input_schema": self.parameters}


def _schema(properties: dict[str, Any], required: list[str]) -> dict[str, Any]:
    return {"type": "object", "properties": properties, "required": required, "additionalProperties": False}


def make_tools(index: Index, principal: str | list[str] | None = None) -> list[Tool]:
    """The agent tools for `index`, seeing only what `principal` may see."""

    def search_documents(query: str, limit: int = 8, collection: str | None = None) -> str:
        collections = [collection] if collection else []
        results = index.search(query, limit=limit, collections=collections, principal=principal)
        if not results:
            return "No results. Try other keywords (the words the document would use)."
        lines = []
        for result in results:
            headings = " > ".join(result["heading_path"])
            lines.append(f"{result['citation']}  {result['title']}  {headings}".rstrip())
            lines.append("    " + " ".join(result["text"].split())[:300])
        return "\n".join(lines)

    def get_context(query: str, budget: int = 1500) -> str:
        context = index.context(query, budget=budget, principal=principal)
        return context["text"] or "No results. Try other keywords (the words the document would use)."

    def list_documents(collection: str | None = None) -> str:
        collections = [collection] if collection else []
        documents = index.documents(collections=collections, principal=principal)
        if not documents:
            return "No documents."
        return "\n".join(
            f"{document['collection']}/{document['path']}  {document['title']}  ({document['token_count']} tokens)"
            for document in documents
        )

    def outline_document(reference: str) -> str:
        outline = index.outline(reference, principal=principal)
        pages = f", {outline['pages']} pages" if outline["pages"] else ""
        lines = [
            f"{outline['collection']}/{outline['path']}  {outline['title']}  ({outline['token_count']} tokens{pages})"
        ]
        for section in outline["sections"]:
            indent = "  " * max(section["level"], 1)
            page = f", p{section['page']}" if section["page"] else ""
            lines.append(f"{indent}{section['number']}  {section['title']}  ({section['token_count']} tokens{page})")
        return "\n".join(lines)

    def read_document(reference: str, budget: int = 2000) -> str:
        passage = index.read(reference, budget=budget, principal=principal)
        text = passage["text"]
        if passage["truncated"]:
            text += (
                f"\n\n[Cut at {budget} tokens. Use outline_document to pick one section, then read it with #number.]"
            )
        return text

    reference_help = (
        "A document like 'collection/path.pdf', a section like 'collection/path.pdf#2.3', "
        "or pages like 'collection/path.pdf#p4-6'. Citations from search results work as references."
    )
    return [
        Tool(
            name="search_documents",
            description=(
                "Keyword search over the documents. Returns the best chunks, each with a citation "
                "like 'docs/billing.pdf#2.3 (p4, L120-141)'."
            ),
            parameters=_schema(
                {
                    "query": {"type": "string", "description": "Keywords the answer would contain."},
                    "limit": {"type": "integer", "description": "The most results.", "default": 8},
                    "collection": {"type": ["string", "null"], "description": "Only this collection."},
                },
                ["query"],
            ),
            function=search_documents,
        ),
        Tool(
            name="get_context",
            description=(
                "The best passages for a question, with citations, packed into a token budget. "
                "Often enough to answer directly."
            ),
            parameters=_schema(
                {
                    "query": {"type": "string", "description": "The question or its keywords."},
                    "budget": {"type": "integer", "description": "The most tokens to return.", "default": 1500},
                },
                ["query"],
            ),
            function=get_context,
        ),
        Tool(
            name="list_documents",
            description="Lists the documents you can read, with their sizes in tokens.",
            parameters=_schema(
                {"collection": {"type": ["string", "null"], "description": "Only this collection."}},
                [],
            ),
            function=list_documents,
        ),
        Tool(
            name="outline_document",
            description="The sections of a document, with numbers (for read_document) and sizes in tokens.",
            parameters=_schema({"reference": {"type": "string", "description": reference_help}}, ["reference"]),
            function=outline_document,
        ),
        Tool(
            name="read_document",
            description="Reads a document, one section, or a page range, cut to a token budget.",
            parameters=_schema(
                {
                    "reference": {"type": "string", "description": reference_help},
                    "budget": {"type": "integer", "description": "The most tokens to return.", "default": 2000},
                },
                ["reference"],
            ),
            function=read_document,
        ),
    ]
