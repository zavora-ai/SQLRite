# GA Release Train Runbook

Objective: produce the final `v1.0.0` GA release bundle for SQLRite, including green release gates, a reproducible benchmark/reliability publication pack, and final sign-off artifacts.

## Primary Command

```bash
bash scripts/run-s33-ga-release-train.sh
```

## What It Does

1. reruns the S32 release-candidate audit
2. builds a host-platform `v1.0.0` release archive and SHA256 file
3. synthesizes the GA checklist, benchmark/reliability publication report, reproducibility manifest, and final sign-off JSON
4. packages the publication evidence into a single tarball for upload/publication

## Inputs

- release blocker ledger: `project_plan/release/defect_register.json`
- RC gate outputs:
  - `project_plan/reports/s32_blocker_audit.json`
  - `project_plan/reports/s32_bench_suite.json`
  - `project_plan/reports/s32_release_quality_report.md`
- reliability evidence:
  - `project_plan/reports/s17_benchmark_recovery.json`
  - `project_plan/reports/s18_benchmark_observability.json`
  - `project_plan/reports/s19_benchmark_dr_gate.json`
  - `project_plan/reports/s19_soak_slo_summary.json`
- release archive tooling:
  - `scripts/create-release-archive.sh`

## Outputs

- `project_plan/reports/s33_quality_gates.log`
- `project_plan/reports/s33_ga_checklist.md`
- `project_plan/reports/s33_benchmark_reliability_report.md`
- `project_plan/reports/s33_benchmark_repro_manifest.json`
- `project_plan/reports/s33_release_train_bundle_manifest.json`
- `project_plan/reports/s33_final_signoff.json`
- `project_plan/reports/sqlrite-v1.0.0-ga-evidence.tar.gz`
- `project_plan/reports/S33.md`
- `dist/sqlrite-v1.0.0-<target>.tar.gz`
- `dist/sqlrite-v1.0.0-<target>.sha256`

## GA Sign-Off Conditions

GA is approved only if all are true:

1. `project_plan/reports/s32_blocker_audit.json` reports `pass=true`
2. open `P0` count is `0`
3. open `P1` count is `0`
4. published availability is at or above `99.95%`
5. published RPO is at or below `60s`
6. release archive and SHA256 exist for the current host target
7. final sign-off JSON reports `signoff_pass=true`

## Publication Notes

- The benchmark/reliability publication report is the GA-facing summary.
- The reproducibility manifest is the machine-readable index of scripts, datasets, hardware metadata, and generated artifacts.
- The evidence tarball should be attached to the release pipeline or retained as the audit archive for the tag.
