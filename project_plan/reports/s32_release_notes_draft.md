# SQLRite v1.0.0-rc1 Release Notes Draft

## Highlights

- SQL-native retrieval now spans embedded CLI SQL, server `/v1/sql`, and API-first migration workflows.
- Frozen v1 API compatibility, secure-default RBAC/audit controls, and migration toolchains are covered by deterministic suites.
- Release-candidate benchmark bundle regenerates latency, throughput, retrieval quality, efficiency, and resilience evidence.

## Release Gate Snapshot

- release_candidate_pass: `true`
- open_p0_count: `0`
- open_p1_count: `0`
- quick_qps: `166.73`
- 10k_p95_ms: `12.3622`
- availability_percent: `100.00`
- observed_rpo_seconds: `0.0050`

## Operator Notes

- Run `bash scripts/run-s32-release-candidate-audit.sh` before tagging or publishing.
- Review `project_plan/reports/s32_release_quality_report.md` and `project_plan/reports/s32_risk_register.md` as the canonical RC package.
