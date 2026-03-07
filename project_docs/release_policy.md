# SQLRite Release Policy

This document defines the branch, support, and release-governance rules for SQLRite `v1.x`.

## Release Channels

SQLRite uses three release channels:

1. `main`
- integration branch for the next release candidate
- every merge must preserve a green `cargo test` baseline and sprint-specific CI

2. `release/v1.0`
- long-term support branch for the `v1.x` line
- accepts only release-blocker fixes, security fixes, documentation corrections, and narrowly scoped backports
- feature work stays on `main` until the next minor release branch is cut

3. version tags
- immutable tags in the form `vMAJOR.MINOR.PATCH`
- only cut after the release-candidate audit passes with zero open `P0` and `P1` defects

## Support Windows

For `v1.x`, SQLRite keeps two supported tracks:

1. Current stable
- latest `v1.x` patch release on `main`
- receives routine bug fixes and documentation updates

2. LTS branch
- `release/v1.0`
- receives critical fixes only:
  - security vulnerabilities
  - data-loss or corruption defects
  - correctness defects with `P0` or `P1` severity
  - packaging regressions that block installation on supported platforms

Backports to the LTS branch must be minimal, reviewed, and linked to a defect record.

## Severity Model

1. `P0`
- data loss, unrecoverable corruption, authentication bypass, or total service outage
- blocks release immediately

2. `P1`
- correctness or availability defect with material user impact and no acceptable workaround
- blocks release immediately

3. `P2`
- important defect with workaround or limited blast radius
- may ship only with explicit risk acceptance in release notes

4. `P3`
- minor defect, paper cut, or documentation gap

## Release Gates

A release candidate may be tagged only when all of the following are true:

1. `cargo fmt --all --check` passes.
2. `cargo test` passes.
3. API compatibility freeze checks pass.
4. security/RBAC/audit/key-hardening suites pass.
5. migration suites for SQL and API-first systems pass.
6. benchmark suite artifacts are regenerated for the release candidate.
7. zero open `P0` and `P1` defects are recorded in `project_plan/release/defect_register.json`.
8. rollback instructions and release notes draft are updated.

The canonical automated gate is:

```bash
bash scripts/run-s32-release-candidate-audit.sh
```

## Governance Cadence

1. Weekly burn-down and triage
- review roadmap carry-over, benchmark drift, and open defects
- update `project_plan/release/defect_register.json` if severity or status changes

2. Monthly release-gate review
- rerun release-quality, performance, and security evidence generation
- compare metrics against the previous release-candidate bundle

3. Pre-tag sign-off
- verify platform packaging jobs are green
- verify rollback assets exist
- confirm changelog and release notes reflect shipped behavior only

## Backport Rules

A backport to `release/v1.0` must:

1. reference a specific defect entry or security advisory
2. avoid broad refactors and schema changes unless mandatory for safety
3. include tests that fail before the patch and pass after it
4. preserve API compatibility for frozen `v1` surfaces

## Rollback Policy

If a release candidate fails post-cut validation:

1. stop promotion immediately
2. revert to the prior stable tag for packaged channels
3. restore data from the latest validated backup or snapshot if data correctness is affected
4. record the incident, blast radius, and remediation in the risk register and sprint report

## Supported Platform Statement

The `v1.x` line targets:

- Linux `x86_64` and `arm64`
- macOS `x86_64` and `arm64`
- Windows `x86_64` and `arm64`

A release cannot be declared ready if packaging or install flows are known-broken on a supported platform.
