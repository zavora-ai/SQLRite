# Migration Patterns: API-First Vector Databases to SQLRite

This document covers migration patterns for API-first vector databases where the source system is not queried primarily through SQL.

Current SQLRite import path for these systems:

1. export source rows to JSONL
2. normalize each record to the pgvector-style JSONL contract
3. import with `sqlrite migrate pgvector --input ...`

## Canonical JSONL contract

Each line must contain:

- `id`
- `doc_id`
- `content`
- `metadata`
- `embedding`
- optional `source`
- optional `doc_metadata`
- optional `doc_source`

## Qdrant pattern

Map each point to one JSON object:

- point id -> `id`
- payload document/group id -> `doc_id`
- payload text field -> `content`
- payload -> `metadata`
- vector -> `embedding`
- payload source field -> `source`

Example normalized line:

```json
{"id":"pt-1","doc_id":"doc-7","content":"agent memory chunk","metadata":{"tenant":"acme","source":"kb"},"embedding":[0.91,0.09,0.0],"source":"kb/doc-7.md"}
```

## Weaviate pattern

Map each object/class row to:

- object UUID -> `id`
- parent reference or synthetic collection key -> `doc_id`
- text property -> `content`
- object properties -> `metadata`
- vector -> `embedding`

If the source has no stable document id, synthesize one before export.

## Milvus pattern

Map each entity row to:

- primary key -> `id`
- source document key -> `doc_id`
- text field -> `content`
- scalar fields -> `metadata`
- vector field -> `embedding`

## Operational guidance

- keep the original embedding model unchanged during migration
- preserve source ids where possible to avoid downstream cache invalidation
- export metadata as JSON objects, not flattened strings
- include deterministic `doc_id` assignment before import
- validate chunk counts before cutover

## Import command

```bash
cargo run -- migrate pgvector \
  --input normalized-export.jsonl \
  --target sqlrite.db \
  --batch-size 512 \
  --create-indexes \
  --json
```

## Validate after import

```bash
cargo run -- doctor --db sqlrite.db --json
cargo run -- query --db sqlrite.db --text "agent memory" --top-k 5
```

## Scope boundary

S30 delivers the normalized JSONL bridge for API-first vector database patterns.
Native source-specific pull/export connectors remain S31 work.
