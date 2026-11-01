# Agent tools

`index.tools()` returns tools for an AI agent. Each tool is a plain function
plus a JSON schema, so it works with any framework or API.

```python
tools = index.tools(principal="user:ana")
by_name = {tool.name: tool for tool in tools}
```

| Tool               | Arguments                       | Returns                                    |
|--------------------|---------------------------------|--------------------------------------------|
| `search_documents` | `query`, `limit`, `collection`  | the best chunks, each with a citation      |
| `get_context`      | `query`, `budget`               | the best passages, cited, within a budget  |
| `list_documents`   | `collection`                    | the documents and their sizes              |
| `outline_document` | `reference`                     | the sections, with numbers and sizes       |
| `read_document`    | `reference`, `budget`           | a document, a section or pages             |

Tools return text, which models read best. Errors come back as text too
(`Error: ...`), so the model can correct itself.

## With an API

=== "OpenAI"

    ```python
    response = client.responses.create(
        model=model,
        input=messages,
        tools=[tool.to_openai() for tool in tools],  # .to_openai_chat() for Chat Completions
    )
    for call in response.output:
        if call.type == "function_call":
            output = by_name[call.name].call_json(call.arguments)
    ```

=== "Anthropic"

    ```python
    message = client.messages.create(
        model=model,
        max_tokens=2048,
        messages=messages,
        tools=[tool.to_anthropic() for tool in tools],
    )
    for block in message.content:
        if block.type == "tool_use":
            output = by_name[block.name].call_json(block.input)
    ```

=== "Plain functions"

    ```python
    for tool in tools:
        print(tool.name, tool.parameters)
    tool = by_name["get_context"]
    print(tool(query="refund policy", budget=1000))
    ```

## The principal is bound, never asked

`principal` is fixed when the tools are made. It is not a tool argument, so the
model can't ask for documents its user may not see. Make the tools once per
user (or per request). See [Access rules](access.md).
