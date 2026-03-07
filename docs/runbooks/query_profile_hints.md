# SQLRite Query Profile Hints Runbook (Sprint 29)

Status: S29 baseline  
Date: March 7, 2026

## Purpose

Provide deterministic query tuning profiles for agent callers that need a simple latency/recall tradeoff without manually setting retrieval knobs.

## Profiles

1. `balanced`
- default behavior
- keeps the requested `candidate_limit`
- keeps the standard hybrid text expansion behavior

2. `latency`
- caps `candidate_limit` at `max(top_k * 8, 32)`
- keeps hybrid text expansion equal to the resolved `candidate_limit`
- intended for interactive agent loops where predictable response time matters more than maximum candidate fan-out

3. `recall`
- raises `candidate_limit` to at least `max(top_k * 32, 200)`
- expands hybrid text candidate collection to `resolved_candidate_limit * 4`
- intended for slower offline or high-recall retrieval stages

## CLI usage

```bash
sqlrite query \
  --db sqlrite.db \
  --text "agent memory retrieval" \
  --top-k 5 \
  --candidate-limit 1000 \
  --query-profile latency
```

Example output prefix:

```text
query_profile=latency resolved_candidate_limit=40
results=5
```

## HTTP query API

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory retrieval","top_k":5,"candidate_limit":1000,"query_profile":"recall"}' \
  http://127.0.0.1:8099/v1/query | jq
```

## gRPC client

```bash
sqlrite-grpc-client \
  --addr 127.0.0.1:50051 \
  query \
  --text "agent memory retrieval" \
  --top-k 5 \
  --candidate-limit 1000 \
  --query-profile recall
```

## Future SQL hint mapping

The SQL-facing design target is:

```sql
/*+ latency */ SELECT ...;
/*+ recall */ SELECT ...;
```

S29 freezes the profile mapping so that future SQL comment parsing can reuse the same resolved retrieval behavior.

## Validation

```bash
bash scripts/run-s29-query-profile-hints.sh
```
