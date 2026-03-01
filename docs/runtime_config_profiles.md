# SQLRite Runtime Profile Contract

Status: Active
Date: March 1, 2026
Owner: SQLRite core

## Objective

Define stable runtime configuration profiles for operator-safe defaults across embedded and server workflows.

## S01 Contract

Runtime profile is selected with:

- `--profile balanced|durable|fast_unsafe`

Vector index mode is selected with:

- `--index-mode brute_force|lsh_ann|disabled`

## Profile Definitions

### `balanced` (default)

Use when:
- General production/dev use where durability and performance are both required.

Configuration:
- `journal_mode = WAL`
- `synchronous = NORMAL`
- `foreign_keys = ON`
- `temp_store = MEMORY`

### `durable`

Use when:
- Stronger durability guarantees are required (accepting lower write throughput).

Configuration:
- `journal_mode = WAL`
- `synchronous = FULL`
- `foreign_keys = ON`
- `temp_store = MEMORY`

### `fast_unsafe`

Use when:
- Benchmarking or ephemeral development speed is prioritized over durability.

Configuration:
- `journal_mode = WAL`
- `synchronous = OFF`
- `foreign_keys = ON`
- `temp_store = MEMORY`

## Index Mode Definitions

### `brute_force`

- Exact cosine search.
- Deterministic quality baseline.
- Default for correctness-sensitive workflows.

### `lsh_ann`

- Approximate nearest-neighbor mode.
- Lower latency at larger corpus sizes.
- Must preserve fallback behavior and deterministic tie-breaking in planner output.

### `disabled`

- No in-memory vector index.
- Useful for text-only, low-memory, or debugging workflows.

## Stability Guarantees

1. Option names (`--profile`, `--index-mode`) are stable in v0.x and carried to v1.0.
2. Profile semantics are documented release-to-release; any change requires release note callout.
3. Default profile remains `balanced` unless major-version policy says otherwise.

## Validation Requirements

Per CI and release checks:

1. `balanced` profile must pass unit/integration tests.
2. `durable` profile must pass write/read consistency tests.
3. `fast_unsafe` profile is allowed only in non-production benchmark profiles.

## S14 Server HA Profile Scaffold

Server mode now includes a high-availability runtime profile surface for replication/failover/recovery scaffolding.

CLI flags:

- `--ha-role standalone|primary|replica`
- `--cluster-id <id>`
- `--node-id <id>`
- `--advertise <host:port>`
- `--peer <host:port>` (repeatable)
- `--sync-ack-quorum <n>`
- `--heartbeat-ms <n>`
- `--election-timeout-ms <n>`
- `--max-replication-lag-ms <n>`
- `--failover manual|automatic`
- `--backup-dir <dir>`
- `--snapshot-interval-s <n>`
- `--pitr-retention-s <n>`
- `--control-token <token>`
- `--disable-sql-endpoint`

Validation contract:

1. Replication disabled requires role `standalone` and no peers.
2. Replication enabled requires role `primary` or `replica`.
3. `sync_ack_quorum >= 1`.
4. `election_timeout_ms > heartbeat_interval_ms`.
5. `max_replication_lag_ms > 0`.
6. Primary quorum cannot exceed cluster size (`peers + self`).

S15 protocol reliability additions:

1. Replication log entries use term/index/checksum validation.
2. Commit index advances only when ACK quorum is satisfied.
3. Vote requests are term-gated and candidate log freshness-checked.
4. Heartbeat updates apply commit progress without exceeding local log head.

S16 resilience additions:

1. `--failover automatic` enables timeout-based automatic promotion checks.
2. Recovery timing can be tracked via `/control/v1/recovery/start` and `/control/v1/recovery/mark-restored`.
3. Chaos scenarios can be injected/cleared for drills:
- `node_crash`
- `disk_full`
- `partition_subset`
4. New resilience read surfaces:
- `GET /control/v1/failover/status`
- `GET /control/v1/resilience`
- `GET /control/v1/chaos/status`
5. `/metrics` now emits HA resilience and chaos counters/gauges for failover and restore duration tracking.

S17 recovery lifecycle additions:

1. Backup/PITR CLI operations:
- `sqlrite backup snapshot`
- `sqlrite backup list`
- `sqlrite backup restore`
- `sqlrite backup pitr-restore`
- `sqlrite backup prune`
2. Recovery control-plane endpoints:
- `POST /control/v1/recovery/snapshot`
- `GET /control/v1/recovery/snapshots`
- `POST /control/v1/recovery/verify-restore`
- `POST /control/v1/recovery/prune-snapshots`

S18 observability additions:

1. Observability control-plane endpoints:
- `GET /control/v1/observability/metrics-map`
- `GET /control/v1/traces/recent`
- `POST /control/v1/observability/reset`
- `GET /control/v1/alerts/templates`
- `POST /control/v1/alerts/simulate`
- `GET /control/v1/slo/report`
2. `/metrics` now emits request/error/latency/tracing/alert counters for SLO analysis.

S19 reliability gate additions:

1. DR game-day and soak validation harness:
- `scripts/run-s19-dr-gameday.sh`
2. SLO window reset support via `POST /control/v1/observability/reset` before soak windows.
3. SLO validation targets used in reference drills:
- availability `>= 99.95%`
- RPO `<= 60s`

S20 agent interoperability additions:

1. MCP runtime command surface:
- `sqlrite mcp [--db PATH] [--profile ...] [--index-mode ...] [--auth-token TOKEN] [--print-manifest]`
2. MCP manifest generation exposes transport/tool/auth contract for agent runtimes.
3. Dedicated MCP runtime binary available as `sqlrite-mcp`.

S21 query-surface interoperability additions:

1. OpenAPI contract endpoint for query surfaces:
- `GET /v1/openapi.json`
2. Retrieval query API endpoint:
- `POST /v1/query`
3. gRPC-style HTTP JSON bridge endpoints:
- `POST /grpc/sqlrite.v1.QueryService/Sql`
- `POST /grpc/sqlrite.v1.QueryService/Query`
