# SQLRite Roadmap Coverage Matrix

Source roadmap: `/Users/jameskaranja/Developer/projects/SQLRight/ROADMAP_COMPETITIVE_2026.md`

Coverage status: **93/93 requirements mapped**.
Coverage guarantee: **100%** (no unmapped roadmap requirements).

## Sprint Execution Status

| Sprint | Status | Report |
| --- | --- | --- |
| S01 | Completed | `project_plan/reports/S01.md` |
| S02 | Completed | `project_plan/reports/S02.md` |
| S03 | Completed | `project_plan/reports/S03.md` |
| S04 | Completed | `project_plan/reports/S04.md` |
| S05 | Completed | `project_plan/reports/S05.md` |
| S06 | Completed | `project_plan/reports/S06.md` |
| S07 | Completed | `project_plan/reports/S07.md` |
| S08 | Completed | `project_plan/reports/S08.md` |
| S09 | Completed | `project_plan/reports/S09.md` |
| S10 | Completed | `project_plan/reports/S10.md` |
| S11 | Completed | `project_plan/reports/S11.md` |
| S12 | Completed | `project_plan/reports/S12.md` |
| S13 | Completed | `project_plan/reports/S13.md` |
| S14 | Completed | `project_plan/reports/S14.md` |
| S15 | Completed | `project_plan/reports/S15.md` |
| S16 | Completed | `project_plan/reports/S16.md` |
| S17 | Completed | `project_plan/reports/S17.md` |
| S18 | Completed | `project_plan/reports/S18.md` |
| S19 | Completed | `project_plan/reports/S19.md` |
| S20 | Completed | `project_plan/reports/S20.md` |
| S21 | Completed | `project_plan/reports/S21.md` |
| S22 | Completed | `project_plan/reports/S22.md` |
| S23 | Completed | `project_plan/reports/S23.md` |
| S24 | Completed | `project_plan/reports/S24.md` |
| S25 | Completed | `project_plan/reports/S25.md` |

| Requirement ID | Roadmap Source | Description | Planned Sprint Coverage |
| --- | --- | --- | --- |
| SD-01 | 3.1 | Ship a single sqlrite umbrella CLI with init/sql/ingest/query/serve/backup/benchmark/doctor commands. | S01 |
| SD-02 | 3.1 | Ship installers for Homebrew, winget, apt/rpm, curl install script, and Docker image. | S02 |
| SD-03 | 3.1 | Ship first-class SDKs for Rust, Python, and TypeScript. | S22, S23, S24 |
| SD-04 | 3.1 | Ship interactive SQL shell with retrieval-aware helpers and examples. | S02, S03 |
| SD-05 | 3.2 | Support vector distance operators (<->, <=>, <#>) in SQL. | S04 |
| SD-06 | 3.2 | Support retrieval SQL functions (vector, embed, bm25_score, hybrid_score). | S05, S06 |
| SD-07 | 3.2 | Support retrieval index DDL for vector and text indexes. | S05 |
| SD-08 | 3.2 | Planner fallback to brute-force when ANN is absent or unhealthy. | S06 |
| SD-09 | 3.2 | Provide EXPLAIN RETRIEVAL with score and execution breakdown. | S07 |
| SD-10 | 3.2 | Provide SEARCH table-valued function for concise hybrid queries. | S31 |
| SD-11 | 3.2 | Provide reranking hooks (cross-encoder optional). | S28, S31 |
| SD-12 | 3.2 | Provide query profile hints for recall/latency tradeoffs. | S29, S31 |
| SD-13 | 3.3 | Embedded mode default: single-file DB, WAL profile, local-first performance. | S01 |
| SD-14 | 3.3 | Server HA profile: multi-node deployment, replicated writes, automatic failover. | S14 |
| SD-15 | 3.3 | Embedded durability target: crash-safe restart with verified recovery. | S17, S18 |
| SD-16 | 3.3 | HA availability target: 99.95% monthly in reference profile. | S19, S33 |
| MR-01 | 4.1 | Install and first query in under five minutes. | S03 |
| MR-02 | 4.1 | SQL cookbook covers at least 80 percent of RAG retrieval patterns. | S07, S21 |
| MR-03 | 4.1 | Built-in agent interoperability (MCP manifest and tool server mode). | S20, S25 |
| MR-04 | 4.1 | Reproducible benchmark and evaluation tooling. | S12, S13 |
| MR-05 | 4.1 | Migration paths from SQLite, pgvector, and API-first vector databases. | S30, S31 |
| MR-06 | 4.1 | Built-in observability: health/readiness/metrics/query traces. | S18, S19 |
| MR-07 | 4.1 | Security defaults: tenant isolation, encryption options, audit logs. | S27, S28 |
| XP-01 | 4.2 | Linux x86_64 and arm64 distribution support. | S02, S12 |
| XP-02 | 4.2 | macOS universal binary distribution support. | S02, S12 |
| XP-03 | 4.2 | Windows x64 and arm64 distribution support. | S02, S12 |
| XP-04 | 4.2 | Docker image support for server mode. | S02, S14 |
| XP-05 | 4.2 | WASM or edge read/query support story. | S26, S27 |
| XP-06 | 4.2 | SDK test matrix green on all supported platforms. | S24, S25 |
| SQ-01 | 4.3 | Hybrid retrieval expressible in one SQL statement. | S06 |
| SQ-02 | 4.3 | Same retrieval SQL semantics in embedded and server modes. | S14, S15 |
| SQ-03 | 4.3 | EXPLAIN indicates ANN or brute-force execution path. | S07, S21 |
| SQ-04 | 4.3 | Deterministic ordering for repeated runs on fixed data/version. | S06 |
| PA-D01 | Phase A | Consolidated sqlrite binary with subcommands. | S01 |
| PA-D02 | Phase A | Packaging pipeline for Homebrew/winget/apt. | S02 |
| PA-D03 | Phase A | sqlrite doctor environment diagnostics. | S02 |
| PA-D04 | Phase A | Quickstart path (init then query). | S03 |
| PA-G01 | Phase A | Median time-to-first-query under 3 minutes. | S03 |
| PA-G02 | Phase A | Install success rate greater than 95 percent across OS matrix. | S03 |
| PB-D01 | Phase B | Distance operators and vector helper functions. | S04 |
| PB-D02 | Phase B | CREATE VECTOR INDEX USING HNSW support. | S05 |
| PB-D03 | Phase B | Hybrid scoring with deterministic tie-breaks. | S06 |
| PB-D04 | Phase B | Retrieval EXPLAIN output with score attribution. | S07 |
| PB-D05 | Phase B | SQL cookbook for semantic/lexical/hybrid/filter/tenant/rerank-ready patterns. | S07 |
| PB-G01 | Phase B | All documented retrieval patterns runnable via SQL only. | S07 |
| PB-G02 | Phase B | Planner correctness across indexed and non-indexed paths. | S07 |
| PC-D01 | Phase C | ANN tuning controls with brute-force fallback. | S08, S09 |
| PC-D02 | Phase C | Vector datatype options and quantization controls. | S09 |
| PC-D03 | Phase C | Memory-mapped index/page optimizations. | S10 |
| PC-D04 | Phase C | Batch ingestion optimizer and compaction tooling. | S11 |
| PC-D05 | Phase C | Public benchmark harness for 10k/100k/1M/10M profiles. | S12, S13 |
| PC-G01 | Phase C | p95 hybrid latency under 40ms at 100k profile. | S13 |
| PC-G02 | Phase C | p95 hybrid latency under 90ms at 1M profile. | S13 |
| PC-G03 | Phase C | Ingestion throughput at or above 50k chunks/min on 8 vCPU reference. | S11, S13 |
| PD-D01 | Phase D | Server replication profile and reference architecture. | S14, S15 |
| PD-D02 | Phase D | Automatic leader failover test harness. | S16 |
| PD-D03 | Phase D | Backup/restore and snapshot policy tooling. | S17 |
| PD-D04 | Phase D | SLO dashboards and alert templates. | S18 |
| PD-D05 | Phase D | Disaster recovery game-day scripts. | S19 |
| PD-G01 | Phase D | Monthly availability at or above 99.95 percent in soak test. | S19 |
| PD-G02 | Phase D | RPO at or below 60 seconds in HA reference profile. | S19 |
| PD-G03 | Phase D | Chaos scenarios validated (node crash, disk-full, partition subset). | S16, S19 |
| PE-D01 | Phase E | Built-in MCP tool server mode. | S20 |
| PE-D02 | Phase E | OpenAPI and gRPC query endpoints. | S21, S22 |
| PE-D03 | Phase E | Python and TypeScript SDK feature parity with Rust core. | S22, S23, S24 |
| PE-D04 | Phase E | First-party integrations and examples for common agent stacks. | S25 |
| PE-D05 | Phase E | Deterministic tool contract tests for agent workflows. | S25 |
| PE-G01 | Phase E | Reference integrations validated in CI. | S25 |
| PE-G02 | Phase E | Agent memory sample setup under 15 minutes. | S25 |
| PF-D01 | Phase F | API freeze and compatibility contract. | S26 |
| PF-D02 | Phase F | Secure multi-tenant policy framework (RBAC hooks/audit/key hardening). | S27 |
| PF-D03 | Phase F | Compliance documentation and updated threat model. | S28, S29 |
| PF-D04 | Phase F | Long-term support branch and release policy. | S32 |
| PF-D05 | Phase F | Migration guides from pgvector/libSQL/Qdrant/Weaviate/Milvus patterns. | S30, S31 |
| PF-G01 | Phase F | Zero open P0/P1 defects at release cut. | S32, S33 |
| PF-G02 | Phase F | Full release quality gates green. | S33 |
| PF-G03 | Phase F | Published v1.0 benchmark and reliability report. | S33 |
| BE-01 | 6.1 | Track latency (p50/p95/p99) by workload profile for each release. | S12, S13, S32 |
| BE-02 | 6.1 | Track throughput (QPS) by concurrency level for each release. | S12, S13, S32 |
| BE-03 | 6.1 | Track retrieval quality metrics (Recall@k, MRR, nDCG). | S12, S13, S32 |
| BE-04 | 6.1 | Track cost efficiency (memory and storage overhead). | S10, S13, S32 |
| BE-05 | 6.1 | Track operational resilience (failover and restore times). | S16, S19, S32 |
| BE-06 | 6.2 | Use the same embedding model and datasets for competitor comparisons. | S12, S30 |
| BE-07 | 6.2 | Use the same hardware classes for competitor comparisons. | S12, S30 |
| BE-08 | 6.2 | Publish benchmark configurations and scripts for reproducibility. | S13, S33 |
| N90-01 | 11 | Deliver unified sqlrite CLI and packaging pipeline in first 90 days. | S01, S02 |
| N90-02 | 11 | Implement SQL operators/functions and retrieval index DDL in first 90 days. | S04, S05, S06 |
| N90-03 | 11 | Publish SQL cookbook with real output examples in first 90 days. | S07 |
| N90-04 | 11 | Stand up benchmark harness and initial competitor baseline runs in first 90 days. | S07 |
| N90-05 | 11 | Ship SQLite and pgvector migration guides in first 90 days. | S07 |
| GV-01 | 9 | Run weekly roadmap burn-down, benchmark drift review, and bug triage. | S01, S02, S03, S04, S05, S06, S07, S08, S09, S10, S11, S12, S13, S14, S15, S16, S17, S18, S19, S20, S21, S22, S23, S24, S25, S26, S27, S28, S29, S30, S31, S32, S33 |
| GV-02 | 9 | Run monthly release-gate review across performance, quality, and security. | S03, S07, S12, S13, S19, S25, S32, S33 |
| GV-03 | 9 | Run quarterly competitor baseline review and target recalibration. | S07, S19, S31 |
