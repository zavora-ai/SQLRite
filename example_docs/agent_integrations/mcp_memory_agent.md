# Example: `mcp_memory_agent.sh`

Source file:

- `examples/agent_integrations/mcp_memory_agent.sh`

## Purpose

This example shows the wire-level MCP interaction path over stdio.

Use it when you want:

- a shell-level MCP demonstration
- explicit request framing
- a reference for agent runtimes that speak MCP over stdio

## Prerequisites

| Requirement | Why |
|---|---|
| built `sqlrite` binary | the script launches SQLRite in MCP mode |
| demo database | the MCP call needs searchable data |

Prepare the environment:

```bash
cargo build --bin sqlrite
./target/debug/sqlrite init --db sqlrite_demo.db --seed-demo
```

## Run It

```bash
examples/agent_integrations/mcp_memory_agent.sh
```

## What the Script Does

| Step | Description |
|---|---|
| build MCP frames | emits `initialize`, `initialized`, and `tools/call` messages |
| start SQLRite MCP mode | runs `sqlrite mcp --db ... --auth-token ...` |
| call `search` | sends an MCP tool call with query text and `top_k` |

## Expected Result

The script prints the MCP server response frames to stdout.

## What to Notice

- the script is intentionally low-level and exposes the framing details
- it is useful when debugging agent-tool interoperability rather than when you want the highest-level integration
