# Migrations

SQLRite can ingest existing SQLite-style datasets and exported vector-store data.

## Supported sources

| Source | Command |
|---|---|
| SQLite | `sqlrite migrate sqlite` |
| libSQL | `sqlrite migrate libsql` |
| pgvector JSONL | `sqlrite migrate pgvector` |
| Qdrant JSONL | `sqlrite migrate qdrant` |
| Weaviate JSONL | `sqlrite migrate weaviate` |
| Milvus JSONL | `sqlrite migrate milvus` |

## SQLite migration

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

## libSQL migration

```bash
sqlrite migrate libsql --source libsql-replica.db --target sqlrite.db --create-indexes
```

## pgvector JSONL migration

```bash
sqlrite migrate pgvector \
  --input export.jsonl \
  --target sqlrite.db \
  --batch-size 512 \
  --create-indexes \
  --json
```

## API-first vector exports

```bash
sqlrite migrate qdrant --input qdrant_export.jsonl --target sqlrite.db --create-indexes
sqlrite migrate weaviate --input weaviate_export.jsonl --target sqlrite.db --create-indexes
sqlrite migrate milvus --input milvus_export.jsonl --target sqlrite.db --create-indexes
```

## Validate the migrated database

```bash
sqlrite doctor --db sqlrite.db --json
sqlrite query --db sqlrite.db --text "agent memory" --top-k 5
```
