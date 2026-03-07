# Server and API Guide

This guide covers HTTP server mode, MCP mode, and the native gRPC service.

## Transport Overview

| Interface | Best for | Start command |
|---|---|---|
| HTTP | web apps, services, simple SDK usage | `sqlrite serve ...` |
| gRPC | typed service-to-service calls | `sqlrite grpc ...` |
| MCP | agent tool runtimes | `sqlrite mcp ...` |

## Recommended Flow

```mermaid
flowchart LR
  A["Seed database"] --> B["Start HTTP or gRPC server"]
  B --> C["Send query or SQL request"]
  C --> D["Integrate SDK or MCP client"]
```

## Before You Start

Create a demo database:

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
```

## 1. Start the HTTP Server

```bash
sqlrite serve --db sqlrite_demo.db --bind 127.0.0.1:8099
```

## Core HTTP Endpoints

| Endpoint | Use for | Result |
|---|---|---|
| `GET /healthz` | liveness | process and storage health |
| `GET /readyz` | readiness | readiness and schema state |
| `GET /metrics` | metrics scraping | Prometheus-style metrics |
| `POST /v1/query` | retrieval | ranked query results |
| `POST /v1/sql` | SQL over HTTP | SQL result rows |
| `POST /v1/rerank-hook` | reranking integration | rerank-ready payload |

### Query endpoint example

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":3}' \
  http://127.0.0.1:8099/v1/query
```

Expected response shape:

| Field | Meaning |
|---|---|
| `kind` | response type |
| `row_count` | number of rows returned |
| `rows` | actual query results |

### SQL endpoint example

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"statement":"SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"}' \
  http://127.0.0.1:8099/v1/sql
```

## 2. Run the MCP Tool Server

Print the manifest:

```bash
sqlrite mcp --db sqlrite_demo.db --print-manifest
```

Run over stdio with auth:

```bash
sqlrite mcp --db sqlrite_demo.db --auth-token dev-token
```

Use MCP mode when SQLRite should appear as a callable tool inside an agent runtime.

## 3. Run the Native gRPC Service

Start the service:

```bash
sqlrite grpc --db sqlrite_demo.db --bind 127.0.0.1:50051
```

Use the companion client:

```bash
sqlrite-grpc-client --addr 127.0.0.1:50051 health
sqlrite-grpc-client --addr 127.0.0.1:50051 query --text "agent memory" --top-k 2
sqlrite-grpc-client --addr 127.0.0.1:50051 sql --statement "SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"
```

## Choosing the Right Interface

| Need | Best interface |
|---|---|
| easiest local integration | HTTP |
| strict service contracts | gRPC |
| agent tools and MCP clients | MCP |
| ad hoc local use | CLI |

## Secure Deployment Note

If you start the HTTP server with `--secure-defaults`, normal query and SQL requests need auth-context headers. See `official_docs/security/security_and_multi_tenant.md`.

## Deeper References

- `project_docs/runbooks/mcp_tool_server.md`
- `project_docs/runbooks/grpc_query_service.md`
- `project_docs/architecture/ha_replication_reference.md`
- `project_docs/runbooks/ha_control_plane.md`
