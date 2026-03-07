# Example: `python_memory_agent.py`

Source file:

- `examples/agent_integrations/python_memory_agent.py`

## Purpose

This example shows the smallest Python SDK integration path for agent-memory style retrieval.

Use it when you want:

- Python SDK usage without packaging the whole repo first
- simple argument-driven queries
- direct JSON printing of the SQLRite response envelope

## Prerequisites

| Requirement | Why |
|---|---|
| SQLRite server on `http://127.0.0.1:8099` | the SDK talks to the HTTP API |
| Python locally available | to run the script |

Start the server if needed:

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
sqlrite serve --db sqlrite_demo.db --bind 127.0.0.1:8099
```

## Run It

```bash
python3 examples/agent_integrations/python_memory_agent.py --base-url http://127.0.0.1:8099 --query "agent memory" --top-k 2
```

## Expected Result

The script prints a JSON query response with fields such as:

- `kind`
- `row_count`
- `rows`

## What to Notice

- the script adds `sdk/python` to `sys.path` directly, so it works from the repo checkout
- it uses the same query envelope as the HTTP API
