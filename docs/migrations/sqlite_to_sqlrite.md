# Migration Guide: SQLite or libSQL to SQLRite

This guide assumes a file-backed SQLite or libSQL replica source and a target SQLRite database.

## Source schema contract

`sqlrite migrate sqlite` and `sqlrite migrate libsql` expect:

- one optional document table
- one required chunk table
- one embedding column per chunk row

Default source mapping:

- document table: `legacy_documents`
- document id column: `doc_id`
- document source column: `source_path`
- document metadata column: `metadata_json`
- chunk table: `legacy_chunks`
- chunk id column: `chunk_id`
- chunk doc-id column: `doc_id`
- chunk content column: `chunk_text`
- chunk metadata column: `metadata_json`
- chunk embedding column: `embedding_blob`
- chunk embedding-dim column: `embedding_dim`
- chunk source column: `source_path`

If your schema differs, override any of those names with CLI flags.

## Embedding formats

Supported source encodings:

- `blob_f32le`
  - little-endian `f32` bytes
  - requires `--chunk-embedding-dim-col`
- `json_array`
  - text column containing JSON arrays such as `[0.12, 0.98, ...]`
- `csv`
  - text column containing comma-separated floats such as `0.12,0.98,...`

## Fast-path migration command

```bash
cargo run -- migrate sqlite \
  --source legacy.db \
  --target sqlrite.db \
  --doc-table legacy_documents \
  --doc-id-col doc_id \
  --doc-source-col source_path \
  --doc-metadata-col metadata_json \
  --chunk-table legacy_chunks \
  --chunk-id-col chunk_id \
  --chunk-doc-id-col doc_id \
  --chunk-content-col chunk_text \
  --chunk-metadata-col metadata_json \
  --chunk-embedding-col embedding_blob \
  --chunk-embedding-dim-col embedding_dim \
  --chunk-source-col source_path \
  --embedding-format blob_f32le \
  --batch-size 512 \
  --create-indexes
```

Equivalent libSQL/local-replica invocation:

```bash
cargo run -- migrate libsql \
  --source local-replica.db \
  --target sqlrite.db \
  --batch-size 512 \
  --create-indexes
```

Disable optional source fields with `none`:

```bash
cargo run -- migrate sqlite \
  --source legacy.db \
  --target sqlrite.db \
  --doc-table none \
  --chunk-source-col none \
  --chunk-metadata-col none \
  --embedding-format csv
```

## What the migration does

1. Opens the source database read-only through SQLite compatibility.
2. Applies SQLRite schema/bootstrap to the target database.
3. Upserts source documents into `documents`.
4. Reads all chunk rows, parses embeddings, normalizes metadata, and ingests in batches.
5. Optionally creates:
   - `CREATE VECTOR INDEX ... USING HNSW`
   - `CREATE TEXT INDEX ... USING FTS5`

## JSON report mode

```bash
cargo run -- migrate sqlite \
  --source legacy.db \
  --target sqlrite.db \
  --json
```

Example report:

```json
{
  "kind": "sqlite",
  "source_path": "legacy.db",
  "target_path": "sqlrite.db",
  "documents_upserted": 245,
  "chunks_migrated": 8124,
  "batch_size": 256,
  "embedding_format": "blob_f32le",
  "create_indexes": false,
  "vector_index_mode": "brute_force",
  "duration_ms": 184.72
}
```

## Validate after migration

```bash
cargo run -- doctor --db sqlrite.db --json
cargo run -- query --db sqlrite.db --text "agent memory" --top-k 5
cargo run -- sql --db sqlrite.db --execute "SELECT COUNT(*) AS chunks FROM chunks;"
```

Recommended checks:

- `doctor.db.integrity_ok == true`
- `doctor.db.chunk_count` matches the expected migrated row count
- query results return known migrated chunks

## Failure modes

- `invalid identifier`
  - one of the table/column override names contains unsafe characters
- `embedding column cannot be null`
  - a source chunk row has a null embedding
- `blob_f32le embedding format requires embedding_dim column`
  - BLOB embeddings were selected without a dimension column
- `InvalidEmbeddingBytes`
  - the BLOB byte length does not match `embedding_dim * 4`
- JSON parse errors
  - malformed metadata or `json_array` embedding payloads

## Reproducible validation

Run the sprint harness:

```bash
bash scripts/run-s30-migration-suite.sh
```
