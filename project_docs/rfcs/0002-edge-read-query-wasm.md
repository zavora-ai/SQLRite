# RFC 0002: Edge Read/Query Support Story (WASM + HTTP)

Status: Accepted
Date: March 2, 2026
Owners: SQLRite core team
Sprint: S26

## Context

SQLRite v1 requires a clear cross-platform edge story (XP-05) while preserving SQL-native retrieval semantics and deterministic behavior.

Current server and embedded modes already expose stable retrieval APIs:

1. SQL API (`POST /v1/sql`)
2. Query API (`POST /v1/query`)
3. gRPC compatibility bridge over HTTP JSON
4. Native gRPC QueryService
5. MCP stdio tool contract

For edge/serverless runtimes, write-heavy workloads are constrained by local ephemeral storage and connection lifecycle limits. The immediate need is deterministic read/query support with operationally safe fallbacks.

## Decision

Adopt a two-path edge strategy for v1:

1. Edge HTTP gateway mode (recommended for production)
- Edge worker receives request and forwards to SQLRite server Query API.
- Worker enforces auth, tenant routing, and request shaping.
- Response contract remains SQLRite `kind=query` envelope.

2. WASM read-only mode (targeted profile)
- Run a read-only SQLite-compatible query engine in WASM for local filtering/scoring on pre-synced snapshots.
- No write path in edge runtime.
- Embedding generation is out-of-process; query vectors are supplied by caller.

This keeps API semantics stable while enabling low-latency edge reads.

## Contract

### Stable query envelope

Both edge modes must preserve the query payload contract:

- `kind` (`query`)
- `row_count`
- `rows[]`

### Determinism

For fixed data and fixed runtime/version:

- stable ranking order by score/tie-breaker
- stable row count for same `top_k`/filters

### Feature parity target

v1 edge-read scope must support:

1. `query_text`
2. optional `query_embedding`
3. `top_k`
4. `alpha`
5. `candidate_limit`
6. `metadata_filters`
7. `doc_id`

### Explicit non-goals for v1 edge-read

1. write ingestion at edge runtime
2. background index rebuild in edge runtime
3. control-plane mutation endpoints at edge

## Architecture

### Path A: Edge gateway (primary)

1. Client -> edge worker (`/query`)
2. Worker validates auth and tenant context
3. Worker forwards to SQLRite server `/v1/query`
4. Worker may cache deterministic responses (short TTL)
5. Worker returns unchanged query envelope

Failure handling:

- upstream timeout -> explicit 504 with retry hint
- upstream 5xx -> pass-through error + request id
- auth failure -> 401/403 at worker boundary

### Path B: WASM read-only (secondary)

1. Snapshot bundle downloaded/synced to edge cache
2. WASM runtime opens snapshot read-only
3. Executes SQL/read-query plan on local snapshot
4. Returns envelope-compatible results

Failure handling:

- snapshot unavailable -> route to Path A
- snapshot staleness exceeds SLA -> route to Path A
- wasm init failure -> route to Path A

## Data and snapshot model

1. Snapshot unit: SQLite DB file + manifest checksum.
2. Snapshot metadata includes:
- schema version
- embedding model/version marker
- created_at
- checksum
3. Snapshot freshness policy:
- max age configurable by deployment tier
- stale snapshots trigger fallback to server query path

## Security model

1. Tenant isolation enforced at gateway or signed metadata policy.
2. Edge tokens scoped to read/query operations only.
3. Sensitive headers never forwarded to downstream logs.
4. Query audit fields preserved for upstream observability correlation.

## Rollout Plan

1. S26 (this sprint)
- freeze API contract and compatibility suite
- publish edge-read RFC and runbook

2. S27-S28
- add policy hooks (RBAC/audit) that gateway can enforce
- integrate key-rotation and audit export requirements

3. S29-S31
- ship reference migration docs + SQL retrieval syntax v2 additions
- formalize query profile hints for edge-specific latency/recall tuning

## Acceptance Criteria for XP-05

1. Published, versioned architecture RFC for edge-read/WASM support.
2. Compatibility suite ensures edge-facing APIs remain stable.
3. Runbook describes deployment, fallback, and troubleshooting steps.

## Risks

1. Snapshot staleness can reduce recall consistency if fallback thresholds are too loose.
2. WASM memory limits may constrain large-candidate retrieval profiles.
3. Worker platform timeout limits can impact long-running hybrid queries.

## Mitigations

1. Enforce conservative freshness thresholds and fallback quickly.
2. Provide per-workload `candidate_limit` caps for edge mode.
3. Add request-budget guards and partial-failure telemetry.
