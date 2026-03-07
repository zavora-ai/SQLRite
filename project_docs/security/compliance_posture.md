# SQLRite Compliance Posture (Sprint 28)

Status: Draft baseline  
Date: March 7, 2026

## Scope

This document records the v1 compliance posture that is directly supported by shipped SQLRite features.

## Implemented Controls

1. Tenant isolation
- authenticated query paths enforce tenant-scoped metadata filters in secure server mode
- cross-tenant access requires explicit `admin` role

2. Audit logging
- server mode can emit JSONL audit events for query and SQL access
- audit export supports filtered export by actor, tenant, operation, time range, and allow/deny state

3. Key management
- tenant keys are versioned by `key_id`
- key material must be at least 16 bytes
- key rotation can be executed and then verified against encrypted metadata state

4. Operational traceability
- security summary endpoint advertises active secure defaults and RBAC role catalog
- exported audit artifacts can be retained independently from online server state

5. Deterministic query governance
- query callers can select `balanced`, `latency`, or `recall` profiles instead of ad hoc fan-out tuning
- the profile mapping is documented and testable across CLI, HTTP, gRPC, and SDK surfaces

## Evidence Artifacts

1. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s27_security_audit.jsonl`
2. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s28_audit_export.jsonl`
3. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s28_security_audit_report.json`
4. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s29_query_profile_report.json`

## Current Gaps

1. No formal external KMS integration yet.
2. No signed audit artifact chain yet.
3. No automated retention/immutability policy enforcement yet.
4. Query profiles do not yet map from general SQL comments outside the published RFC/design target.

## Release Guidance

Use secure server mode for production-facing deployments:

```bash
sqlrite serve \
  --db sqlrite.db \
  --secure-defaults \
  --authz-policy .sqlrite/rbac-policy.json \
  --audit-log .sqlrite/audit/server_audit.jsonl
```
