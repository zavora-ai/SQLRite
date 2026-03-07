# SQLRite Audit Export and Key Rotation Runbook (Sprint 28)

Status: S28 baseline  
Date: March 7, 2026

## Purpose

Operate audit export and verified tenant key rotation in secure SQLRite deployments.

## Export audit logs from CLI

```bash
cargo run --bin sqlrite-security -- export-audit \
  --input .sqlrite/audit/server_audit.jsonl \
  --output audit_export.jsonl \
  --format jsonl \
  --tenant demo \
  --operation query \
  --allowed true
```

## Export audit logs from control plane

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: secret" \
  -d '{"tenant_id":"demo","output_path":"project_plan/reports/s28_audit_export.jsonl","format":"jsonl"}' \
  http://127.0.0.1:8099/control/v1/security/audit/export | jq
```

## Rotate tenant key with verification report

```bash
cargo run --bin sqlrite-security -- rotate-key \
  --db sqlrite.db \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --field secret_payload \
  --new-key-id k2 \
  --json
```

## Verify tenant key coverage after rotation

```bash
cargo run --bin sqlrite-security -- verify-key \
  --db sqlrite.db \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --field secret_payload \
  --key-id k2
```

Success condition:

1. `verified_all_target_key=true`
2. `stale_key_ids=[]`

## Rerank hook request

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-actor-id: reader-1" \
  -H "x-sqlrite-tenant-id: demo" \
  -H "x-sqlrite-roles: reader" \
  -d '{"query_text":"agent memory","candidate_count":10}' \
  http://127.0.0.1:8099/v1/rerank-hook | jq
```

This endpoint is intended for external rerankers and preserves the same tenant and RBAC policy checks as `/v1/query`.

## Smoke validation

```bash
bash scripts/run-s28-security-audit-hardening.sh
```
