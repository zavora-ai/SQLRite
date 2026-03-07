# SQLRite Threat Model (Sprint 28)

Status: Updated baseline  
Date: March 7, 2026

## Assets

1. chunk content and metadata
2. tenant encryption keys
3. audit logs
4. query and SQL API surfaces
5. rerank candidate payloads

## Trust Boundaries

1. client to server API boundary
2. control-plane operator boundary
3. on-disk audit and key-registry storage
4. external reranker integration boundary

## Primary Threats

1. Cross-tenant data exposure
- mitigated by tenant header enforcement and RBAC policy checks in secure mode

2. Unauthorized SQL execution
- mitigated by `sql_admin` role requirement in secure mode

3. Audit log leakage
- mitigated by redaction support and filtered export pipeline

4. Weak tenant key material
- mitigated by minimum key length enforcement

5. Incomplete key rotation
- mitigated by post-rotation verification report with stale key detection

6. External reranker data overexposure
- mitigated by reusing authenticated query policy for rerank-hook requests and preserving tenant scoping

## Rerank Hook Security Review

The rerank hook returns scored retrieval candidates for external cross-encoders or rerankers.

Security requirements:

1. It must use the same authenticated tenant context as `/v1/query`.
2. It must reject conflicting tenant filters.
3. It must not bypass RBAC to expose SQL or cross-tenant data.
4. Audit logging must record allowed and denied rerank-hook access when audit logging is enabled.

## Deferred Risks

1. No remote attestation for audit export artifacts.
2. No hardware-backed key custody.
3. No content-level field masking inside rerank payloads beyond tenant isolation.
