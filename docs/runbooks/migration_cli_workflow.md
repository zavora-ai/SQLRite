# Runbook: Migration CLI Workflow

## Goal

Move a retrieval corpus into SQLRite with deterministic verification and rollback-safe validation.

## Supported source paths in S30 and S31

- `sqlrite migrate sqlite`
- `sqlrite migrate libsql`
- `sqlrite migrate pgvector`
- `sqlrite migrate qdrant`
- `sqlrite migrate weaviate`
- `sqlrite migrate milvus`

## Preconditions

- source dataset is frozen for the migration window
- the embedding model is unchanged between source and target
- expected chunk count is known before import
- target path points to a fresh or disposable SQLRite database

## Execution flow

1. Prepare the source export or schema mapping.
2. Run the migration command with explicit source and target paths.
3. Capture the migration report with `--json` for auditability.
4. Run `sqlrite doctor --json` against the target database.
5. Run at least one known-answer query against the migrated corpus.
6. If counts or retrieval regress, discard the target database and rerun after fixing the mapping.

## SQLite or libSQL example

```bash
cargo run -- migrate sqlite \
  --source legacy.db \
  --target sqlrite.db \
  --embedding-format blob_f32le \
  --create-indexes \
  --json
```

## pgvector export example

```bash
cargo run -- migrate pgvector \
  --input export.jsonl \
  --target sqlrite.db \
  --batch-size 512 \
  --create-indexes \
  --json
```

## API-first export example

```bash
cargo run -- migrate qdrant \
  --input qdrant_export.jsonl \
  --target sqlrite.db \
  --batch-size 512 \
  --create-indexes \
  --json
```

## Validation checklist

- migration report shows the expected `chunks_migrated`
- doctor report shows `integrity_ok=true`
- doctor report chunk count matches the migration report
- query path returns known migrated content
- `SEARCH(...)` query returns known migrated rows for cutover validation
- optional vector/text indexes were created if requested

## Rollback

Migration in S30 is target-side only. Rollback is simple:

1. stop reads/writes pointed at the migrated target
2. delete the target SQLRite database files
3. rerun migration after fixing the mapping or source export

No source mutation is performed by the migration command.

## Troubleshooting

- source schema mismatch
  - override the table or column names with the provided CLI flags
- malformed metadata JSON
  - normalize invalid JSON before import
- blob embedding byte mismatch
  - verify the dimension column and that source bytes are little-endian `f32`
- missing document table
  - pass `--doc-table none` and import chunk rows only

## Evidence generation

Run the reproducible harness to generate sprint-grade evidence:

```bash
bash scripts/run-s30-migration-suite.sh
```
