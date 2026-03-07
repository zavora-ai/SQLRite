# Migration Guide

This guide covers the supported migration paths into SQLRite.

The goal is to get existing content into SQLRite without forcing you to redesign your entire application first.

## Supported Sources

| Source | Command family | Typical input |
|---|---|---|
| SQLite | `sqlrite migrate sqlite` | existing application database |
| libSQL | `sqlrite migrate libsql` | SQLite-compatible replica or file |
| pgvector export | `sqlrite migrate pgvector` | JSONL export |
| Qdrant export | `sqlrite migrate qdrant` | JSONL export |
| Weaviate export | `sqlrite migrate weaviate` | JSONL export |
| Milvus export | `sqlrite migrate milvus` | JSONL export |

## Migration Decision Table

| Your source system | Start with |
|---|---|
| app-owned SQLite database | `sqlrite migrate sqlite` |
| libSQL file or replica | `sqlrite migrate libsql` |
| Postgres + pgvector export pipeline | `sqlrite migrate pgvector` |
| API-first vector DB export | `sqlrite migrate qdrant`, `weaviate`, or `milvus` |

## 1. Migrate from SQLite

Use this when your current app already stores chunks and embeddings in SQLite tables.

```bash
sqlrite migrate sqlite \
  --source legacy.db \
  --target sqlrite.db \
  --doc-table legacy_documents \
  --doc-id-col doc_id \
  --chunk-table legacy_chunks \
  --chunk-id-col chunk_id \
  --chunk-doc-id-col doc_id \
  --chunk-content-col chunk_text \
  --chunk-embedding-col embedding_blob \
  --chunk-embedding-dim-col embedding_dim \
  --embedding-format blob_f32le \
  --batch-size 512 \
  --create-indexes
```

## 2. Migrate from libSQL

Use this when the source is SQLite-compatible and the main goal is to land it in SQLRite's schema and indexes.

```bash
sqlrite migrate libsql \
  --source libsql-replica.db \
  --target sqlrite.db \
  --create-indexes
```

## 3. Migrate from pgvector-Style JSONL

Use this when embeddings are exported from a Postgres pipeline into JSONL.

```bash
sqlrite migrate pgvector \
  --input export.jsonl \
  --target sqlrite.db \
  --batch-size 512 \
  --create-indexes \
  --json
```

## 4. Migrate from API-First Vector Exports

Use these commands when your current vector store is not SQL-native and you have an export file.

```bash
sqlrite migrate qdrant --input qdrant_export.jsonl --target sqlrite.db --create-indexes
sqlrite migrate weaviate --input weaviate_export.jsonl --target sqlrite.db --create-indexes
sqlrite migrate milvus --input milvus_export.jsonl --target sqlrite.db --create-indexes
```

## 5. Validate the Migrated Database

After migration, run both a structural check and a retrieval check.

```bash
sqlrite doctor --db sqlrite.db --json
sqlrite query --db sqlrite.db --text "agent memory" --top-k 5
```

What to look for:

| Check | Healthy sign |
|---|---|
| `doctor` | integrity is healthy |
| `doctor` | chunk count is non-zero |
| `query` | returns results from the migrated corpus |

## Migration Checklist

| Item | Why it matters |
|---|---|
| source IDs are preserved | easier cutover and reconciliation |
| embeddings are decoded correctly | retrieval quality depends on this |
| document mapping is correct | query scoping depends on this |
| indexes are created | performance depends on this |
| query smoke tests pass | validates end-to-end usefulness |

## Recommended Cutover Flow

```mermaid
flowchart LR
  A["Export or connect to source"] --> B["Run migration command"]
  B --> C["Run doctor"]
  C --> D["Run retrieval smoke tests"]
  D --> E["Switch application traffic"]
```

## Deeper References

- `project_docs/migrations/sqlite_to_sqlrite.md`
- `project_docs/migrations/pgvector_to_sqlrite.md`
- `project_docs/migrations/api_first_vector_db_patterns.md`
- `project_docs/runbooks/migration_cli_workflow.md`
