# S32 Risk Register

Generated: `2026-03-07`

## Accepted Risks

- `R1` Platform packaging remains dependent on the external GitHub release/publishing path and is validated in CI rather than by a live tag in this sprint.
- `R2` Benchmark suite in S32 is sized for reproducible release checks (`quick`, `10k`) and does not replace longer-duration publication runs planned for S33.

## Mitigated Risks

- `M1` Frozen API drift is covered by the S26 compatibility manifest and suite.
- `M2` Secure-default deployment regressions are covered by S27 and S28 operator suites.
- `M3` Migration regressions are covered by S30 and S31 import suites across SQL and API-first inputs.

## Rollback Plan

1. Stop release promotion and keep packaged channels pinned to the previous stable tag.
2. Restore the latest validated snapshot or backup if correctness/regression affects persisted data.
3. Record the failed gate in `project_plan/release/defect_register.json` and rerun the full S32 audit after the fix.

## Gate Inputs

- `project_plan/reports/s32_quality_gates.log`
- `project_plan/reports/s32_bench_suite.json`
- `project_plan/reports/s32_blocker_audit.json`
