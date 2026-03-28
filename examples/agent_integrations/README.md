# SQLRite Agent Integration Examples

These examples show how to call SQLRite from Python, TypeScript, and MCP-oriented tooling.

## Prerequisites

Build the CLI and create a demo database:

```bash
cargo build --bin sqlrite
cargo build --bin sqlrite-mcp

./target/debug/sqlrite init --db sqlrite_demo.db --seed-demo
./target/debug/sqlrite serve --db sqlrite_demo.db --bind 127.0.0.1:8099
```

## Python SDK example

```bash
python3 examples/agent_integrations/python_memory_agent.py --base-url http://127.0.0.1:8099 --query "agent memory" --top-k 2
```

## TypeScript SDK example

```bash
npm --prefix sdk/typescript install
npm --prefix sdk/typescript run build
node examples/agent_integrations/typescript_memory_agent.mjs --base-url http://127.0.0.1:8099 --query "agent memory" --top-k 2
```

## MCP example

```bash
examples/agent_integrations/mcp_memory_agent.sh
```
