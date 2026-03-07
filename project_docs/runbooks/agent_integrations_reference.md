# Agent Integrations Reference Runbook (S25)

## Purpose

Validate SQLRite reference integrations across MCP, HTTP/OpenAPI, Python SDK, and TypeScript SDK with deterministic contract checks.

## One-command Contract Suite

```bash
bash scripts/run-s25-agent-contract-suite.sh
```

Generated artifacts:

- `project_plan/reports/s25_agent_contract_suite.log`
- `project_plan/reports/s25_agent_contract_report.json`
- `project_plan/reports/s25_agent_memory_setup.log`
- `project_plan/reports/s25_agent_memory_setup.json`

## Setup-Time Gate (<15 minutes)

```bash
bash scripts/run-s25-agent-memory-setup.sh
```

Gate result is recorded in:

- `project_plan/reports/s25_agent_memory_setup.json`

## Monthly Release-Gate Review

```bash
bash scripts/run-s25-release-gate-review.sh
```

Generated artifact:

- `project_plan/reports/s25_release_gate_review.md`

## Determinism Contract

The S25 contract suite asserts:

1. Same top chunk ID across:
- `/v1/query`
- gRPC bridge (`/grpc/sqlrite.v1.QueryService/Query`)
- Python SDK
- TypeScript SDK
- MCP `search` tool

2. Same row count across all surfaces.
3. All surfaces return `kind=query` payloads.
4. Setup-time gate remains under 15 minutes.
