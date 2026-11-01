# Access rules

Each document has an access list: the principals that may see it, like
`"group:hr"` or `"user:ana"`. An empty list means everyone.

```python
index.add("wiki")                                  # everyone
index.add("hr", access=["group:hr"])               # only group:hr
index.sync()

index.search("salary", principal="user:ana")                  # wiki only
index.search("salary", principal=["user:ana", "group:hr"])    # wiki and hr
index.search("salary")                                         # no principal: everything
```

- The store filters **before** ranking, so a search never shows, counts or
  ranks documents the caller may not see.
- `read` and `outline` answer "not found" for a document the caller may not
  see, as if it didn't exist.
- No `principal` means a trusted caller (your own code, the local CLI) that
  sees everything. Pass one whenever the request comes from a user.
- A source can give a file its own access list (cloud storage permissions, in
  later versions). It then replaces the collection's list.

For agents, bind the principal when you make the tools, never in the model's
arguments:

```python
tools = index.tools(principal=current_user.principals)
```

From the command line: `foliq index hr --access group:hr`, and
`foliq search salary --principal user:ana`.
