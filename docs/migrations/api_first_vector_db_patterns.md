# Migration Patterns: API-First Vector Databases to SQLRite

S31 adds native JSONL migration entrypoints for API-first vector database export shapes:

- `sqlrite migrate qdrant`
- `sqlrite migrate weaviate`
- `sqlrite migrate milvus`

These commands import source exports directly without requiring a manual pgvector-style normalization step first.

## Shared command shape

```bash
cargo run -- migrate <source> \
  --input export.jsonl \
  --target sqlrite.db \
  --batch-size 512 \
  --create-indexes \
  --json
```

Shared override flags:

- `--id-field`
- `--doc-id-field`
- `--content-field`
- `--embedding-field`
- `--metadata-field`
- `--source-field`
- `--doc-metadata-field`
- `--doc-source-field`

Use `none` for optional fields you want to disable.

## Qdrant

Default field mapping:

- `id_field=id`
- `doc_id_field=payload.doc_id`
- `content_field=payload.content`
- `embedding_field=vector`
- `metadata_field=payload`
- `source_field=payload.source`
- `doc_metadata_field=payload`
- `doc_source_field=payload.source`

Example input line:

```json
{"id":"pt-1","payload":{"doc_id":"doc-1","content":"agent memory chunk","source":"kb/doc-1.md","tenant":"acme"},"vector":[0.91,0.09,0.0]}
```

Command:

```bash
cargo run -- migrate qdrant \
  --input qdrant_export.jsonl \
  --target sqlrite.db \
  --create-indexes
```

If your export uses named vectors, SQLRite accepts an object-valued vector field when that object contains an array-valued vector.

## Weaviate

Default field mapping:

- `id_field=id`
- `doc_id_field=properties.doc_id`
- `content_field=properties.content`
- `embedding_field=vector`
- `metadata_field=properties`
- `source_field=properties.source`
- `doc_metadata_field=properties`
- `doc_source_field=properties.source`

Example input line:

```json
{"id":"wv-1","properties":{"doc_id":"doc-1","content":"weaviate memory chunk","source":"kb/doc-1.md","tenant":"acme"},"vector":[0.77,0.23,0.0]}
```

Command:

```bash
cargo run -- migrate weaviate \
  --input weaviate_export.jsonl \
  --target sqlrite.db \
  --create-indexes
```

## Milvus

Default field mapping:

- `id_field=id`
- `doc_id_field=doc_id`
- `content_field=content`
- `embedding_field=embedding`
- `metadata_field=metadata`
- `source_field=source`
- `doc_metadata_field=metadata`
- `doc_source_field=source`

Example input line:

```json
{"id":"mv-1","doc_id":"doc-1","content":"milvus memory chunk","source":"kb/doc-1.md","metadata":{"tenant":"acme"},"embedding":[0.66,0.34,0.0]}
```

Command:

```bash
cargo run -- migrate milvus \
  --input milvus_export.jsonl \
  --target sqlrite.db \
  --create-indexes
```

## Validation after import

```bash
cargo run -- doctor --db sqlrite.db --json
cargo run -- sql --db sqlrite.db --execute "SELECT chunk_id, hybrid_score FROM SEARCH('agent memory', vector('0.95,0.05,0.0'), 5, 0.65, 500, 'balanced', '{\\\"tenant\\\":\\\"acme\\\"}', NULL) ORDER BY hybrid_score DESC, chunk_id ASC;"
```

## Operational guidance

- keep the original embedding model unchanged during migration
- preserve source ids where possible to avoid downstream cache invalidation
- export metadata as JSON objects, not flattened strings
- validate chunk counts and known-answer queries before cutover
- keep the source export immutable while validating the target database
