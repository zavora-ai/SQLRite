# Example: `typescript_memory_agent.mjs`

Source file:

- `examples/agent_integrations/typescript_memory_agent.mjs`

## Purpose

This example shows the smallest TypeScript SDK integration path for agent-memory retrieval.

Use it when you want:

- Node.js access to SQLRite over HTTP
- a local built SDK package
- direct JSON printing of query results

## Prerequisites

| Requirement | Why |
|---|---|
| SQLRite server on `http://127.0.0.1:8099` | the SDK talks to the HTTP API |
| TypeScript SDK built locally | the example imports the built output |

Prepare the SDK:

```bash
npm --prefix sdk/typescript install
npm --prefix sdk/typescript run build
```

## Run It

```bash
node examples/agent_integrations/typescript_memory_agent.mjs --base-url http://127.0.0.1:8099 --query "agent memory" --top-k 2
```

## Expected Result

The script prints a JSON query envelope returned by the TypeScript SDK.

## What to Notice

- the script imports from `sdk/typescript/dist/index.js`
- the client constructor takes the base URL directly
- the request shape matches the HTTP query payload
