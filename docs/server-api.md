# Server, API, gRPC, and MCP

SQLRite is embedded first, but it also ships network and agent-facing interfaces.

## Interfaces

| Interface | Best for |
|---|---|
| HTTP | simple integration and SDK use |
| compact HTTP | lower-overhead agent and benchmark clients |
| gRPC | typed service-to-service calls |
| MCP | agent tool runtimes |

## HTTP server

```bash
sqlrite serve --db sqlrite_demo.db --bind 127.0.0.1:8099
```

Endpoints:

| Endpoint | Use |
|---|---|
| `GET /healthz` | liveness |
| `GET /readyz` | readiness |
| `GET /metrics` | metrics |
| `POST /v1/query` | full retrieval response |
| `POST /v1/query-compact` | compact array-oriented retrieval response |
| `POST /v1/sql` | SQL over HTTP |
| `POST /v1/rerank-hook` | reranking integration |

## Full query response

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":3}' \
  http://127.0.0.1:8099/v1/query
```

## Compact query response

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":3}' \
  http://127.0.0.1:8099/v1/query-compact
```

Use `query-compact` when lower transport overhead matters more than rich JSON field names.

## SQL over HTTP

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"statement":"SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"}' \
  http://127.0.0.1:8099/v1/sql
```

## gRPC

Start the server:

```bash
sqlrite grpc --db sqlrite_demo.db --bind 127.0.0.1:50051
```

Use the client:

```bash
sqlrite-grpc-client --addr 127.0.0.1:50051 health
sqlrite-grpc-client --addr 127.0.0.1:50051 query --text "agent memory" --top-k 2
sqlrite-grpc-client --addr 127.0.0.1:50051 sql --statement "SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"
```

## MCP

Print the manifest:

```bash
sqlrite mcp --db sqlrite_demo.db --print-manifest
```

Run over stdio:

```bash
sqlrite mcp --db sqlrite_demo.db --auth-token dev-token
```

## Secure server note

With `--secure-defaults`, query and SQL requests require auth context headers:

- `x-sqlrite-actor-id`
- `x-sqlrite-tenant-id`
- `x-sqlrite-roles`
