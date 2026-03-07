# SDK Guide

This guide covers the Python and TypeScript SDKs.

Both SDKs use the same HTTP service surface as the CLI examples in the server guide.

## Before You Start

Start a local server:

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
sqlrite serve --db sqlrite_demo.db --bind 127.0.0.1:8099
```

## SDK Comparison

| SDK | Best for | Install flow |
|---|---|---|
| Python | local agents, scripts, backend tooling | `python -m pip install -e sdk/python` |
| TypeScript | Node.js apps, agents, web backends | `npm --prefix sdk/typescript install && npm --prefix sdk/typescript run build` |

## Python SDK

### Install

```bash
python -m pip install -e sdk/python
```

### Query example

```python
from sqlrite_sdk import SqlRiteClient

client = SqlRiteClient("http://127.0.0.1:8099")
response = client.query(query_text="agent memory", top_k=3)
print(response["row_count"])
print(response["rows"][0]["chunk_id"])
```

What to expect:

- the Python SDK returns the same JSON envelope as the HTTP API
- `response["rows"]` contains the actual result rows

## TypeScript SDK

### Install and build

```bash
npm --prefix sdk/typescript install
npm --prefix sdk/typescript run build
```

### Query example

```ts
import { SqlRiteClient } from "@sqlrite/sdk";

const client = new SqlRiteClient("http://127.0.0.1:8099");
const response = await client.query({ query_text: "agent memory", top_k: 3 });
console.log(response.row_count);
console.log(response.rows[0].chunk_id);
```

What to expect:

- the TypeScript SDK returns the same query envelope as the HTTP API
- `response.rows` contains the actual rows

## Example Integrations

| Example | Guide |
|---|---|
| Python agent integration | `example_docs/agent_integrations/python_memory_agent.md` |
| TypeScript agent integration | `example_docs/agent_integrations/typescript_memory_agent.md` |
| MCP shell integration | `example_docs/agent_integrations/mcp_memory_agent.md` |

## When to Use SDKs vs Raw HTTP

| Need | Best option |
|---|---|
| quick curl-level debugging | raw HTTP |
| Python application integration | Python SDK |
| Node.js or TypeScript application integration | TypeScript SDK |
| agent tool runtime integration | MCP |

## Related Guides

- `official_docs/integrations/server_and_api_guide.md`
- `example_docs/README.md`
