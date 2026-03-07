# SQLRite MCP Tool Server Runbook

Status: S20 baseline  
Date: March 1, 2026

## Purpose

Operate SQLRite as an MCP stdio tool server for agent runtimes.

## Start MCP server (unified CLI)

```bash
sqlrite mcp --db /var/lib/sqlrite/sqlrite.db --auth-token "$SQLRITE_MCP_TOKEN"
```

## Start MCP server (dedicated binary)

```bash
sqlrite-mcp --db /var/lib/sqlrite/sqlrite.db --auth-token "$SQLRITE_MCP_TOKEN"
```

## Print MCP manifest

```bash
sqlrite mcp --db /var/lib/sqlrite/sqlrite.db --auth-token "$SQLRITE_MCP_TOKEN" --print-manifest
```

## MCP methods

1. `initialize`
2. `ping`
3. `tools/list`
4. `tools/call`

## Auth behavior

1. If server started without `--auth-token`, `tools/call` does not require `arguments.auth_token`.
2. If server started with `--auth-token`, `tools/call` must include matching `arguments.auth_token`.
3. Auth failures return JSON-RPC error code `-32001`.

## Tool catalog

1. `search`
2. `ingest`
3. `health`
4. `delete_by_metadata`

## Smoke validation

```bash
cargo build --bin sqlrite
scripts/run-s20-mcp-smoke.sh
```

Expected artifacts:

1. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s20_mcp_smoke.log`
2. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s20_benchmark_mcp.json`

## Troubleshooting

1. Parse errors (`-32700`):
- verify `Content-Length` framing and valid UTF-8 JSON payloads.

2. Unknown method (`-32601`):
- ensure method is one of `initialize`, `ping`, `tools/list`, `tools/call`.

3. Invalid params (`-32602`):
- ensure `tools/call` includes `params.name` and object `params.arguments`.

4. Unauthorized (`-32001`):
- ensure request includes `arguments.auth_token` matching server `--auth-token`.
