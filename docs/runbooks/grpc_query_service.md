# gRPC QueryService Runbook (Sprint 22)

## Purpose

Operate and validate SQLRite's native gRPC `QueryService` for SDK-aligned query execution.

## Start Service

```bash
cargo run -- grpc --db sqlrite_demo.db --bind 127.0.0.1:50051
```

Options:

- `--profile balanced|durable|fast_unsafe`
- `--index-mode brute_force|lsh_ann|hnsw_baseline|disabled`

## Validate Service

```bash
cargo run --bin sqlrite-grpc-client -- --addr 127.0.0.1:50051 health
cargo run --bin sqlrite-grpc-client -- --addr 127.0.0.1:50051 \
  query --text "agent memory" --top-k 2
cargo run --bin sqlrite-grpc-client -- --addr 127.0.0.1:50051 \
  sql --statement "SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"
```

## Deterministic Smoke

```bash
bash scripts/run-s22-grpc-sdk-smoke.sh
```

Artifacts:

- `project_plan/reports/s22_grpc_sdk_smoke.log`

## Failure Modes

1. Invalid request payload:
- gRPC status: `INVALID_ARGUMENT`
- fix request fields (`query_text` or `query_embedding` required, `top_k >= 1`).

2. Database open/runtime failure:
- gRPC status: `INTERNAL`
- verify DB path and file permissions.

3. Connection refused/timeouts:
- verify bind address and port availability.
- run `sqlrite-grpc-client health` to confirm readiness.
