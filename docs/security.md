# Security

SQLRite includes RBAC, tenant key management, encrypted metadata rotation, audit export, and secure server defaults.

## Security tools

| Capability | Command |
|---|---|
| create policy | `sqlrite-security init-policy` |
| add tenant key | `sqlrite-security add-key` |
| rotate metadata keys | `sqlrite-security rotate-key` |
| verify key coverage | `sqlrite-security verify-key` |
| export audit logs | `sqlrite-security export-audit` |
| secure server mode | `sqlrite serve --secure-defaults` |

## Starter policy

```bash
sqlrite-security init-policy --path .sqlrite/rbac-policy.json
```

## Add tenant keys

```bash
sqlrite-security add-key \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --key-id k1 \
  --key-material demo-secret-material \
  --active
```

Add the next key before rotation:

```bash
sqlrite-security add-key \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --key-id k2 \
  --key-material demo-secret-material-v2 \
  --active
```

## Rotate encrypted metadata

```bash
sqlrite-security rotate-key \
  --db sqlrite_demo.db \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --field secret_payload \
  --new-key-id k2 \
  --json
```

Note:

- the seeded demo database usually has `rotated_chunks=0`
- use `/Users/jameskaranja/Developer/projects/SQLRight/examples/security_rotation_workflow.rs` for a reproducible encrypted fixture

## Verify key coverage

```bash
sqlrite-security verify-key \
  --db sqlrite_demo.db \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --field secret_payload \
  --key-id k2
```

## Export audit logs

```bash
sqlrite-security export-audit \
  --input .sqlrite/audit/server_audit.jsonl \
  --output audit_export.jsonl \
  --format jsonl \
  --tenant demo
```

## Secure HTTP server

```bash
sqlrite serve \
  --db sqlrite_demo.db \
  --bind 127.0.0.1:8099 \
  --secure-defaults \
  --authz-policy .sqlrite/rbac-policy.json \
  --audit-log .sqlrite/audit/server_audit.jsonl \
  --control-token dev-token
```

Authenticated query example:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-actor-id: reader-1" \
  -H "x-sqlrite-tenant-id: demo" \
  -H "x-sqlrite-roles: reader" \
  -d '{"query_text":"agent memory","top_k":3}' \
  http://127.0.0.1:8099/v1/query
```
