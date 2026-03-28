# Examples

These are the runnable examples that matter most for real users.

## Core examples

| Example | Command | What it shows |
|---|---|---|
| `basic_search.rs` | `cargo run --example basic_search` | smallest embedded retrieval flow |
| `query_use_cases.rs` | `cargo run --example query_use_cases` | text, vector, hybrid, filters, doc scope, RRF |
| `ingestion_worker.rs` | `cargo run --example ingestion_worker` | checkpointed ingest and post-ingest search |
| `secure_tenant.rs` | `cargo run --example secure_tenant` | tenant-scoped ingest and secure search |
| `security_rotation_workflow.rs` | `cargo run --example security_rotation_workflow` | creates a reproducible encrypted rotation fixture |
| `tool_adapter.rs` | `cargo run --example tool_adapter` | tool-call integration surface |

## Agent integration examples

| Example | Command | What it shows |
|---|---|---|
| `python_memory_agent.py` | `python3 examples/agent_integrations/python_memory_agent.py --base-url http://127.0.0.1:8099 --query "agent memory" --top-k 2` | Python SDK query flow |
| `typescript_memory_agent.mjs` | `node examples/agent_integrations/typescript_memory_agent.mjs --base-url http://127.0.0.1:8099 --query "agent memory" --top-k 2` | TypeScript SDK query flow |
| `mcp_memory_agent.sh` | `examples/agent_integrations/mcp_memory_agent.sh` | MCP request framing |

## Good starting order

1. `cargo run --example basic_search`
2. `cargo run --example query_use_cases`
3. `cargo run --example ingestion_worker`
4. `cargo run --example secure_tenant`
