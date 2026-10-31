"""foliq: search, read and cite your documents from AI agents.

```python
import foliq

index = foliq.Index(".foliq")
index.add("docs")
index.sync()
print(index.context("how long do refunds take?", budget=1500)["text"])
```

The engine is written in Rust (`foliq._core`). This package adds the agent
tools (`foliq.tools`), the plugin protocols (`foliq.plugins`) and the CLI entry point.
"""

from foliq._core import SKILL, FoliqError, Index, NotFoundError, PluginError, __version__
from foliq.plugins import Parser, Sink, Splitter
from foliq.tools import Tool

__all__ = [
    "SKILL",
    "FoliqError",
    "Index",
    "NotFoundError",
    "Parser",
    "PluginError",
    "Sink",
    "Splitter",
    "Tool",
    "__version__",
]
