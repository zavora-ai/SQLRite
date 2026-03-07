# RFC 0003: Query Profile Hints

Status: Accepted for v1 baseline  
Date: March 7, 2026

## Problem

AI agents need a small, stable query-tuning surface. Exposing only raw knobs like `candidate_limit` is flexible but too low-level for common usage.

## Decision

SQLRite exposes a deterministic `query_profile` hint with three values:

1. `balanced`
2. `latency`
3. `recall`

These hints map to stable retrieval behavior in core search execution.

## Mapping

1. `balanced`
- no candidate-limit rewrite

2. `latency`
- resolved `candidate_limit = min(requested_candidate_limit, max(top_k * 8, 32))`
- hybrid text candidate fetch multiplier = `1x`

3. `recall`
- resolved `candidate_limit = max(requested_candidate_limit, max(top_k * 32, 200))`
- hybrid text candidate fetch multiplier = `4x`

## Transport Surfaces

1. CLI:
- `sqlrite query --query-profile latency|balanced|recall`
- `sqlrite benchmark --query-profile latency|balanced|recall`

2. HTTP:
- `POST /v1/query`
- `POST /v1/rerank-hook`

3. gRPC:
- `QueryRequest.query_profile`

4. SDKs:
- Rust SDK core request field
- TypeScript client request field
- Python client request parameter

## SQL Design Target

Future SQL comment hints should map directly:

```sql
/*+ latency */ SELECT ...;
/*+ recall */ SELECT ...;
```

This RFC intentionally freezes the semantic mapping before full SQL comment parsing is introduced.

## Non-Goals

1. Per-query ANN engine switching.
2. Automatic alpha/fusion rewrites.
3. Cost-based adaptive profile selection inside the engine.

## Risks

1. Users may expect profile hints to change ranking semantics, not just candidate expansion behavior.
2. Excessive recall defaults can raise resource usage in shared deployments.

## Mitigations

1. Surface the resolved candidate limit in CLI output and benchmark artifacts.
2. Keep `balanced` as the default and require explicit opt-in for `recall`.
