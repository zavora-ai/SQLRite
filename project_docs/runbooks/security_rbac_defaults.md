# SQLRite Security RBAC Runbook (Sprint 27)

Status: S27 baseline  
Date: March 7, 2026

## Purpose

Operate SQLRite with secure defaults enabled:

1. per-request auth context headers
2. tenant-scoped query enforcement
3. RBAC policy checks
4. JSONL audit logging

## Generate default RBAC policy

```bash
cargo run --bin sqlrite-security -- init-policy --path .sqlrite/rbac-policy.json
```

Default roles:

1. `reader`
2. `writer`
3. `tenant_admin`
4. `admin`

## Start secure server mode

```bash
sqlrite serve \
  --db sqlrite.db \
  --bind 127.0.0.1:8099 \
  --secure-defaults \
  --authz-policy .sqlrite/rbac-policy.json \
  --audit-log .sqlrite/audit/server_audit.jsonl
```

Secure defaults enable:

1. auth context requirement for query and SQL APIs
2. default RBAC enforcement
3. audit log output

## Required request headers

1. `x-sqlrite-actor-id`
2. `x-sqlrite-tenant-id`
3. `x-sqlrite-roles`

## Example reader query

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-actor-id: reader-1" \
  -H "x-sqlrite-tenant-id: demo" \
  -H "x-sqlrite-roles: reader" \
  -d '{"query_text":"agent","top_k":2}' \
  http://127.0.0.1:8099/v1/query | jq
```

## Example admin SQL

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-actor-id: admin-1" \
  -H "x-sqlrite-tenant-id: demo" \
  -H "x-sqlrite-roles: admin" \
  -d '{"statement":"SELECT id FROM chunks ORDER BY id ASC LIMIT 1;"}' \
  http://127.0.0.1:8099/v1/sql | jq
```

## Security summary endpoint

```bash
curl -fsS http://127.0.0.1:8099/control/v1/security | jq
```

## Smoke validation

```bash
bash scripts/run-s27-security-rbac-smoke.sh
```

Expected artifacts:

1. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s27_security_rbac_smoke.log`
2. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s27_security_rbac_report.json`
3. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s27_security_audit.jsonl`

## Troubleshooting

1. `missing auth context headers`
- include all three `x-sqlrite-*` headers.

2. `tenant filter mismatch`
- remove conflicting `metadata_filters.tenant`; SQLRite injects the authenticated tenant.

3. `authorization denied`
- verify the caller role includes the requested operation.

4. audit file not created
- ensure `--audit-log` path is writable by the SQLRite process.
