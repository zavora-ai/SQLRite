# RFC 0001: Unified SQLRite CLI Command Map

Status: Accepted
Date: February 28, 2026
Owners: SQLRite core team
Sprint: S01

## Context

SQLRite currently exposes multiple focused binaries (`sqlrite-query`, `sqlrite-ingest`, `sqlrite-ops`, etc.).
For mass adoption and predictable operator workflows, Sprint 1 requires a single top-level entrypoint with a stable command contract.

## Decision

Adopt one umbrella binary (`sqlrite`) with explicit subcommands:

1. `init`
2. `sql`
3. `ingest`
4. `query`
5. `serve`
6. `backup`
7. `benchmark`
8. `doctor`

The `sqlrite` binary is the default run target in Cargo (`default-run = "sqlrite"`).

Post-S01 extension (added in S03):

9. `quickstart`

## Command Contract (S01 Scope)

### `sqlrite init`

Purpose:
- Create or open a SQLRite DB.
- Apply runtime profile and index mode.
- Optionally seed demo chunks.

Flags:
- `--db PATH`
- `--profile balanced|durable|fast_unsafe`
- `--index-mode brute_force|lsh_ann|disabled`
- `--seed-demo`

### `sqlrite sql`

Purpose:
- Run raw SQL directly against a DB.
- Return JSON rows for query statements.
- Support SQL-native retrieval operator syntax in shell mode.

Flags:
- `--db PATH`
- `--profile balanced|durable|fast_unsafe`
- `--execute "SQL"`

S04 operator/function extension:

- Vector operators: `<->`, `<=>`, `<#>`
- Vector helper functions: `vector`, `l2_distance`, `cosine_distance`, `neg_inner_product`, `vec_dims`, `vec_to_json`

### `sqlrite ingest`

Purpose:
- Ingest one chunk from CLI input for quick/manual workflows.

Flags:
- `--db PATH`
- `--profile balanced|durable|fast_unsafe`
- `--index-mode brute_force|lsh_ann|disabled`
- `--id ID`
- `--doc-id ID`
- `--content TEXT`
- `--embedding v1,v2,...`
- `--metadata JSON`
- `--tenant TENANT`
- `--source SRC`

### `sqlrite query`

Purpose:
- Execute text/vector/hybrid retrieval.

Flags:
- `--db PATH`
- `--profile balanced|durable|fast_unsafe`
- `--index-mode brute_force|lsh_ann|disabled`
- `--text QUERY`
- `--vector v1,v2,...`
- `--top-k N`
- `--alpha F`
- `--candidate-limit N`
- `--doc-id ID`
- `--filter key=value`
- `--fusion weighted|rrf`
- `--rrf-k F`

### `sqlrite serve`

Purpose:
- Start health server for `/healthz`, `/readyz`, `/metrics`.

Flags:
- `--db PATH`
- `--bind HOST:PORT`
- `--profile balanced|durable|fast_unsafe`
- `--index-mode brute_force|lsh_ann|disabled`

### `sqlrite quickstart` (S03 extension)

Purpose:
- Execute first-run `init -> query` path in one command.
- Produce machine-readable timing and success telemetry.
- Enforce quickstart quality gates in CI (`--max-median-ms`, `--min-success-rate`).

Flags:
- `--db PATH`
- `--profile balanced|durable|fast_unsafe`
- `--index-mode brute_force|lsh_ann|disabled`
- `--seed-demo|--no-seed-demo`
- `--reset|--no-reset`
- `--text QUERY`
- `--vector v1,v2,...`
- `--top-k N`
- `--alpha F`
- `--candidate-limit N`
- `--fusion weighted|rrf`
- `--rrf-k F`
- `--runs N`
- `--max-median-ms F`
- `--min-success-rate F`
- `--json`
- `--output PATH`

### `sqlrite backup`

Purpose:
- Create and verify file-level backups.

Flags:
- `--source <db_path>`
- `--dest <backup_path>`
- `verify --path <backup_path>`

### `sqlrite benchmark`

Purpose:
- Run synthetic benchmark profile from one command.

Flags:
- `--corpus N`
- `--queries N`
- `--warmup N`
- `--embedding-dim N`
- `--top-k N`
- `--candidate-limit N`
- `--batch-size N`
- `--alpha F`
- `--fusion weighted|rrf`
- `--rrf-k F`
- `--profile balanced|durable|fast_unsafe`
- `--index-mode brute_force|lsh_ann|disabled`

### `sqlrite doctor`

Purpose:
- Validate runtime environment and DB health.

Flags:
- `--db PATH`
- `--profile balanced|durable|fast_unsafe`
- `--index-mode brute_force|lsh_ann|disabled`

## Backward Compatibility

- Existing specialized binaries remain available in Sprint 1.
- `cargo run` now resolves to `sqlrite` by default via Cargo manifest.

## Follow-up (Sprint 2+)

1. Lift full ingest/reindex/security workflows into umbrella CLI parity.
2. Add machine-readable command schema output (`sqlrite doctor --json` and `sqlrite help --json`).
3. Add stable long-option aliases and deprecation policy for flag evolution.
