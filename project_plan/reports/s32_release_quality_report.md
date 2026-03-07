# S32 Release Quality Report

Release target: `v1.0.0`
Generated: `2026-03-07`

## Gate Summary

- overall_release_candidate_pass: `true`
- open_p0_count: `0`
- open_p1_count: `0`
- largest_observed_test_pass_count: `184`

## Required Suite Status

- cargo_fmt: `true`
- cargo_test: `true`
- s26_api_compatibility: `true`
- s27_security_rbac: `true`
- s28_security_audit: `true`
- s30_migration_toolchain: `true`
- s31_sql_v2_api_migrations: `true`

## Performance And Efficiency

- quick_qps: `162.57`
- quick_p50_ms: `6.1107`
- quick_p95_ms: `6.4154`
- quick_p99_ms: `6.6484`
- 10k_qps: `89.58`
- 10k_p95_ms: `11.4753`
- 10k_top1_hit_rate: `1.0000`
- 10k_approx_working_set_bytes: `11300140`
- 10k_vector_index_estimated_memory_bytes: `5660000`
- 10k_sqlite_mmap_size_bytes: `268435456`
- 10k_sqlite_cache_size_kib: `65536`

## Retrieval Quality

- brute_force @k=5: recall_at_k=`1.0000`, mrr=`1.0000`, ndcg_at_k=`0.9732`
- lsh_ann @k=5: recall_at_k=`1.0000`, mrr=`1.0000`, ndcg_at_k=`0.9732`
- hnsw_baseline @k=5: recall_at_k=`1.0000`, mrr=`1.0000`, ndcg_at_k=`0.9732`

## Throughput By Concurrency

- concurrency 1: qps=`161.20`, p95_ms=`6.4437`
- concurrency 2: qps=`76.60`, p95_ms=`6.9354`
- concurrency 4: qps=`44.17`, p95_ms=`7.5024`

## Operational Resilience

- availability_percent: `100.00`
- availability_target_percent: `99.95`
- availability_pass: `true`
- observed_rpo_seconds: `0.0050`
- rpo_target_seconds: `60.0000`
- rpo_pass: `true`
- dr_qps: `68.53`
- dr_p95_ms: `6.5572`
- restore_qps: `286.51`
- restore_p95_ms: `4.4195`
- observability_qps: `73.02`
- observability_p95_ms: `6.3222`

## Closed Defect Ledger

- REL-001 `P1` `closed`: Frozen API contract drift risk across CLI, OpenAPI, gRPC, and MCP surfaces
- REL-002 `P1` `closed`: Tenant and audit control gaps in secure-default server deployments
- REL-003 `P1` `closed`: Migration parity gap for SQLite, pgvector, and API-first vector database users
- REL-004 `P2` `closed`: Release policy and LTS branch rules were undocumented for v1 cut
