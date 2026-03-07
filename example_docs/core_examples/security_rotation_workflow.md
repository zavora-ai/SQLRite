# Example: `security_rotation_workflow`

Source file:

- `examples/security_rotation_workflow.rs`

## Purpose

This example prepares a real file-backed fixture for key-rotation workflows.

Use it when you want a reproducible setup for:

- encrypted tenant metadata
- key registry seeding
- audit-log generation
- `sqlrite-security rotate-key`
- `sqlrite-security verify-key`

## Run It

```bash
cargo run --example security_rotation_workflow -- /tmp/sqlrite-rotation.db /tmp/sqlrite-rotation-keys.json /tmp/sqlrite-rotation-audit.jsonl
```

If you omit the arguments, it writes into `project_plan/reports/` by default.

## What the Example Does

| Step | Description |
|---|---|
| open file-backed database | creates a persistent database |
| create audit logger | configures JSONL audit logging |
| load or create key registry | prepares tenant keys on disk |
| register `k1` and `k2` | seeds the current and next keys |
| ingest encrypted chunk | inserts a chunk with encrypted metadata |
| print paths | tells you where the fixture files were written |

## Observed Output

```text
seeded encrypted tenant chunk into /tmp/sqlrite-doc-rotation.db
registry saved to /tmp/sqlrite-doc-rotation-keys.json
```

## Follow-On Commands

Rotate to `k2`:

```bash
sqlrite-security rotate-key \
  --db /tmp/sqlrite-rotation.db \
  --registry /tmp/sqlrite-rotation-keys.json \
  --tenant demo \
  --field secret_payload \
  --new-key-id k2 \
  --json
```

Verify with `k2`:

```bash
sqlrite-security verify-key \
  --db /tmp/sqlrite-rotation.db \
  --registry /tmp/sqlrite-rotation-keys.json \
  --tenant demo \
  --field secret_payload \
  --key-id k2
```

## What to Notice

- this example is primarily a fixture generator, not a full rotation command by itself
- it gives you a realistic encrypted dataset that the seeded demo database does not provide
