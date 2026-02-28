# SQLRite Runtime Profile Contract

Status: Active
Date: February 28, 2026
Owner: SQLRite core

## Objective

Define stable runtime configuration profiles for operator-safe defaults across embedded and server workflows.

## S01 Contract

Runtime profile is selected with:

- `--profile balanced|durable|fast_unsafe`

Vector index mode is selected with:

- `--index-mode brute_force|lsh_ann|disabled`

## Profile Definitions

### `balanced` (default)

Use when:
- General production/dev use where durability and performance are both required.

Configuration:
- `journal_mode = WAL`
- `synchronous = NORMAL`
- `foreign_keys = ON`
- `temp_store = MEMORY`

### `durable`

Use when:
- Stronger durability guarantees are required (accepting lower write throughput).

Configuration:
- `journal_mode = WAL`
- `synchronous = FULL`
- `foreign_keys = ON`
- `temp_store = MEMORY`

### `fast_unsafe`

Use when:
- Benchmarking or ephemeral development speed is prioritized over durability.

Configuration:
- `journal_mode = WAL`
- `synchronous = OFF`
- `foreign_keys = ON`
- `temp_store = MEMORY`

## Index Mode Definitions

### `brute_force`

- Exact cosine search.
- Deterministic quality baseline.
- Default for correctness-sensitive workflows.

### `lsh_ann`

- Approximate nearest-neighbor mode.
- Lower latency at larger corpus sizes.
- Must preserve fallback behavior and deterministic tie-breaking in planner output.

### `disabled`

- No in-memory vector index.
- Useful for text-only, low-memory, or debugging workflows.

## Stability Guarantees

1. Option names (`--profile`, `--index-mode`) are stable in v0.x and carried to v1.0.
2. Profile semantics are documented release-to-release; any change requires release note callout.
3. Default profile remains `balanced` unless major-version policy says otherwise.

## Validation Requirements

Per CI and release checks:

1. `balanced` profile must pass unit/integration tests.
2. `durable` profile must pass write/read consistency tests.
3. `fast_unsafe` profile is allowed only in non-production benchmark profiles.
