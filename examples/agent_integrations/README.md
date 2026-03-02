# SQLRite Agent Integration Examples (S25)

Reference integrations for common agent stacks.

## Prerequisites

1. Build SQLRite binary:

```bash
cargo build --bin sqlrite
```

2. Seed demo database:

```bash
target/debug/sqlrite init --db sqlrite_demo.db --seed-demo
```

3. Start server for SDK examples:

```bash
target/debug/sqlrite serve --db sqlrite_demo.db --bind 127.0.0.1:8099
```

## Python SDK agent-memory query

```bash
python3 examples/agent_integrations/python_memory_agent.py --base-url http://127.0.0.1:8099 --query "agent memory" --top-k 2
```

## TypeScript SDK agent-memory query

```bash
npm --prefix sdk/typescript install
npm --prefix sdk/typescript run build
node examples/agent_integrations/typescript_memory_agent.mjs --base-url http://127.0.0.1:8099 --query "agent memory" --top-k 2
```

## MCP tool-mode agent-memory query

```bash
examples/agent_integrations/mcp_memory_agent.sh
```

## Full deterministic contract suite

```bash
bash scripts/run-s25-agent-contract-suite.sh
```

Artifacts:

- `project_plan/reports/s25_agent_contract_suite.log`
- `project_plan/reports/s25_agent_contract_report.json`
