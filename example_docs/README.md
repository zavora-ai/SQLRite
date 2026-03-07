# SQLRite Example Documentation

This folder documents every example shipped with the repository.

Use it when you want to understand what an example demonstrates, how to run it, what output to expect, and how to adapt it to real code.

## Example Categories

| Category | What it covers |
|---|---|
| `core_examples/` | embedded Rust usage, ingestion, querying, security, and tool adapters |
| `agent_integrations/` | Python, TypeScript, and MCP integration examples |

## Core Rust Examples

| Example | Guide | Best for |
|---|---|---|
| `examples/basic_search.rs` | `example_docs/core_examples/basic_search.md` | smallest embedded search example |
| `examples/ingestion_worker.rs` | `example_docs/core_examples/ingestion_worker.md` | checkpointed ingest |
| `examples/query_use_cases.rs` | `example_docs/core_examples/query_use_cases.md` | retrieval pattern overview |
| `examples/secure_tenant.rs` | `example_docs/core_examples/secure_tenant.md` | secure multi-tenant embedding |
| `examples/security_rotation_workflow.rs` | `example_docs/core_examples/security_rotation_workflow.md` | encryption-rotation fixture setup |
| `examples/tool_adapter.rs` | `example_docs/core_examples/tool_adapter.md` | tool and MCP-style adapter integration |

## Agent Integration Examples

| Example | Guide | Best for |
|---|---|---|
| `examples/agent_integrations/python_memory_agent.py` | `example_docs/agent_integrations/python_memory_agent.md` | Python SDK calls |
| `examples/agent_integrations/typescript_memory_agent.mjs` | `example_docs/agent_integrations/typescript_memory_agent.md` | TypeScript SDK calls |
| `examples/agent_integrations/mcp_memory_agent.sh` | `example_docs/agent_integrations/mcp_memory_agent.md` | MCP protocol framing |

## Suggested Reading Order

1. `example_docs/core_examples/basic_search.md`
2. `example_docs/core_examples/query_use_cases.md`
3. `example_docs/core_examples/ingestion_worker.md`
4. `example_docs/core_examples/secure_tenant.md`
5. the integration examples you actually need
