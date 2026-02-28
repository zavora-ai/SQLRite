# S07 Quarterly Competitor Baseline Review (GV-03 / N90-04)

Date: February 28, 2026

## Purpose

Establish reproducible baseline workflow and current SQLRite benchmark posture for competitor recalibration.

## Baseline Harness

Implemented and exercised:

1. `sqlrite-bench`
2. `sqlrite-bench-matrix`
3. SQL cookbook conformance runner (`scripts/run-sql-cookbook-conformance.sh`)

Primary sprint artifact:

- `project_plan/reports/s07_bench_matrix.json`

## Current SQLRite Baseline (Quick Profile)

From `project_plan/reports/s07_bench_matrix.json`:

1. `weighted + brute_force`: `qps=80.48`, `p95=17.564ms`, `top1=1.0000`
2. `weighted + lsh_ann`: `qps=196.99`, `p95=8.721ms`, `top1=1.0000`
3. `weighted + disabled_index`: `qps=49.92`, `p95=35.312ms`, `top1=1.0000`

## External Competitor Calibration Status

1. Baseline schema and metric format are now fixed in repository artifacts.
2. SQLRite internal scenarios are captured and reproducible.
3. External system runs (pgvector/libSQL/vector DB engines) require external service deployment and are tracked for follow-on sprints.

## Recalibration Notes

1. Keep `weighted + lsh_ann` as throughput reference profile for Phase B/C transition.
2. Keep `weighted + brute_force` as correctness-first reference profile.
3. Use SQL cookbook conformance plus `EXPLAIN RETRIEVAL` output as semantic parity checks before cross-engine comparison.
