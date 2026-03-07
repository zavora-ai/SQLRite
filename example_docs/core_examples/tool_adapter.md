# Example: `tool_adapter`

Source file:

- `examples/tool_adapter.rs`

## Purpose

This example shows how to expose SQLRite through a tool-oriented interface rather than calling the search API directly.

Use it when you want:

- named tool-call routing
- a JSON request and response boundary
- MCP-style manifest generation
- a starting point for agent-tool integration

## Run It

```bash
cargo run --example tool_adapter
```

## What the Example Does

| Step | Description |
|---|---|
| open database | creates an in-memory SQLRite instance |
| create adapter | wraps the database in `SqlRiteToolAdapter` |
| ingest through tool API | inserts a chunk using a tool request |
| call named tool | invokes `search` with JSON arguments |
| print manifest size | shows how many tools are exposed |

## Observed Output

```text
== tool_adapter results ==
named tool response: {
  "status": "ok",
  "payload": [
    {
      "chunk_id": "chunk-tool-1",
      "content": "Tool adapters expose SQLRite as callable functions.",
      "doc_id": "doc-tool-1",
      "hybrid_score": 1.0,
      "metadata": {
        "tenant": "acme"
      },
      "text_score": 1.0,
      "vector_score": 0.0
    }
  ]
}
tools exposed: 4
```

## What to Notice

- the adapter exposes a JSON-shaped boundary that is easier for tool brokers and agents to consume
- `mcp_tools_manifest()` lets you discover the exported tools without standing up a server first

## Good Follow-Up Changes

- wrap the adapter in your agent runtime
- add authorization checks before forwarding tool calls
- map your own tool naming conventions to SQLRite operations
