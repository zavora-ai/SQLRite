# Migration Guide: SQLite to SQLRite

This guide assumes an existing SQLite database and a target SQLRite database file.

## 1. Initialize SQLRite

```bash
cargo run -- init --db sqlrite.db --profile balanced --index-mode brute_force
```

## 2. Map source schema to SQLRite core tables

SQLRite stores retrieval data in:

- `documents(id, source, metadata, created_at)`
- `chunks(id, doc_id, content, metadata, embedding, embedding_dim, created_at)`

Recommended mapping:

- source document table -> `documents`
- source text segments/chunks -> `chunks`
- source JSON metadata -> `chunks.metadata`

## 3. Load document rows

Example:

```sql
INSERT INTO documents (id, source, metadata)
SELECT doc_id, source_path, COALESCE(metadata_json, '{}')
FROM legacy_documents;
```

## 4. Load chunk rows

If legacy embeddings are already available, convert to SQLRite little-endian float blob format in your migration pipeline.

```sql
INSERT INTO chunks (id, doc_id, content, metadata, embedding, embedding_dim)
SELECT chunk_id, doc_id, chunk_text, COALESCE(metadata_json, '{}'), embedding_blob, embedding_dim
FROM legacy_chunks;
```

## 5. Register retrieval index metadata

```bash
cargo run -- sql --db sqlrite.db --execute \
  "CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw ON chunks(embedding) USING HNSW WITH (m=16, ef_construction=64);"

cargo run -- sql --db sqlrite.db --execute \
  "CREATE TEXT INDEX IF NOT EXISTS idx_chunks_content_fts ON chunks(content) USING FTS5 WITH (tokenizer=unicode61);"
```

## 6. Validate retrieval behavior

```bash
cargo run -- sql --db sqlrite.db --execute \
  "EXPLAIN RETRIEVAL SELECT id, 1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score, bm25_score('agent memory', content) AS text_score, hybrid_score(1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')), bm25_score('agent memory', content), 0.65) AS hybrid FROM chunks ORDER BY hybrid DESC, id ASC LIMIT 5;"
```

## 7. Regression checks

Run quality and integrity checks after migration:

```bash
cargo test
cargo run -- doctor --db sqlrite.db --json
```
