# SQLRite Project Plan

This folder contains executable sprint-level planning for the competitive roadmap.

Roadmap source: `/Users/jameskaranja/Developer/projects/SQLRight/ROADMAP_COMPETITIVE_2026.md`

## Coverage Commitments

- Sprint cadence: 2 weeks (with release-close short sprint for S33).
- Planned sprint files: 33
- Requirement catalog: 93 roadmap requirements
- Coverage matrix: `coverage_matrix.md` (auto-generated; must remain 100%).
- Coverage status at generation time: 93/93 mapped.

## Folder Layout

- `requirements_catalog.tsv`: normalized requirement IDs extracted from roadmap
- `sprint_metadata.tsv`: sprint schedule, scope IDs, and sprint deliverables
- `coverage_matrix.md`: requirement-to-sprint mapping (100% required)
- `sprints/`: detailed sprint execution plans (S01..S33)
- `generate_plan.sh`: generator script for all plan artifacts

## Sprint Schedule

| Sprint | Dates | Phase | Goal |
| --- | --- | --- | --- |
| S01 | 2026-03-02 to 2026-03-15 | A | Unified CLI architecture and command contract |
| S02 | 2026-03-16 to 2026-03-29 | A | Packaging and install channels plus doctor diagnostics |
| S03 | 2026-03-30 to 2026-04-12 | A | Quickstart UX hardening and phase A gate closure |
| S04 | 2026-04-13 to 2026-04-26 | B | SQL parser extension for vector operators and literal helpers |
| S05 | 2026-04-27 to 2026-05-10 | B | Vector index DDL and metadata catalog |
| S06 | 2026-05-11 to 2026-05-24 | B | Hybrid scoring engine and deterministic fallback behavior |
| S07 | 2026-05-25 to 2026-06-07 | B | Retrieval explainability and SQL cookbook completion |
| S08 | 2026-06-08 to 2026-06-21 | C | ANN abstraction refactor and HNSW baseline |
| S09 | 2026-06-22 to 2026-07-05 | C | Index persistence and datatype/quantization options |
| S10 | 2026-07-06 to 2026-07-19 | C | Memory-mapped index pages and cache tuning |
| S11 | 2026-07-20 to 2026-08-02 | C | Ingestion throughput optimization and compaction |
| S12 | 2026-08-03 to 2026-08-16 | C | Benchmark harness at 10k/100k/1M and platform test matrix |
| S13 | 2026-08-17 to 2026-08-30 | C | 10M profile hardening and phase C gate closure |
| S14 | 2026-08-31 to 2026-09-13 | D | HA server architecture and replication profile scaffolding |
| S15 | 2026-09-14 to 2026-09-27 | D | Replication log and leader election reliability |
| S16 | 2026-09-28 to 2026-10-11 | D | Automatic failover and chaos harness |
| S17 | 2026-10-12 to 2026-10-25 | D | Backup/restore and point-in-time recovery tooling |
| S18 | 2026-10-26 to 2026-11-08 | D | Observability dashboards and alert policy templates |
| S19 | 2026-11-09 to 2026-11-22 | D | DR game-day, soak tests, and phase D gate closure |
| S20 | 2026-11-23 to 2026-12-06 | E | MCP tool server mode baseline |
| S21 | 2026-12-07 to 2026-12-20 | E | OpenAPI query surface and cookbook parity |
| S22 | 2026-12-21 to 2027-01-03 | E | gRPC service and shared SDK runtime core |
| S23 | 2027-01-04 to 2027-01-17 | E | Python SDK parity and integration test matrix |
| S24 | 2027-01-18 to 2027-01-31 | E | TypeScript SDK parity and cross-platform SDK CI |
| S25 | 2027-02-01 to 2027-02-14 | E | Reference integrations and phase E gate closure |
| S26 | 2027-02-15 to 2027-02-28 | F | API freeze and compatibility suite kickoff |
| S27 | 2027-03-01 to 2027-03-14 | F | RBAC policy framework and secure defaults |
| S28 | 2027-03-15 to 2027-03-28 | F | Audit export and key-rotation hardening |
| S29 | 2027-03-29 to 2027-04-11 | F | Compliance documentation and query hint design |
| S30 | 2027-04-12 to 2027-04-25 | F | Migration toolchain from SQLite/pgvector/libSQL |
| S31 | 2027-04-26 to 2027-05-09 | F | Migration from API-first vector DB patterns and SQL v2 design |
| S32 | 2027-05-10 to 2027-05-23 | F | Final quality audit and release blocker burn-down |
| S33 | 2027-05-24 to 2027-05-31 | F | GA release train and publication of benchmark/reliability reports |

## How to Regenerate

`bash /Users/jameskaranja/Developer/projects/SQLRight/project_plan/generate_plan.sh`

The script fails if any roadmap requirement is not mapped to at least one sprint.
