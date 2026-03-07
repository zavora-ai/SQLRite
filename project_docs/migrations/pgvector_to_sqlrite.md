# Migration Guide: pgvector to SQLRite

SQLRite supports pgvector migrations through a JSONL import path designed for deterministic export/import workflows.

## Query syntax mapping

| pgvector/Postgres | SQLRite |
| --- | --- |
| `embedding <-> '[...]'` | `embedding <-> vector('...')` |
| `embedding <=> '[...]'` | `embedding <=> vector('...')` |
| `embedding <#> '[...]'` | `embedding <#> vector('...')` |
| `ts_rank` or BM25-style lexical score | `bm25_score(query, content)` |
| weighted fusion in SQL | `hybrid_score(vector_score, text_score, alpha)` |

## Export format

Export one JSON object per line with:

- `id`
- `doc_id`
- `content`
- `metadata`
- `embedding`
- optional `source`
- optional `doc_metadata`
- optional `doc_source`

Example:

```json
{"id":"chunk-1","doc_id":"doc-1","content":"local-first memory chunk","metadata":{"tenant":"acme"},"embedding":[0.95,0.05,0.0],"source":"docs/doc-1.md","doc_metadata":{"tenant":"acme"}}
```

## Import into SQLRite

```bash
cargo run -- migrate pgvector \
  --input export.jsonl \
  --target sqlrite.db \
  --batch-size 512 \
  --create-indexes
```

JSON report mode:

```bash
cargo run -- migrate pgvector \
  --input export.jsonl \
  --target sqlrite.db \
  --json
```

Example report:

```json
{
  "kind": "pgvector_jsonl",
  "source_path": "export.jsonl",
  "target_path": "sqlrite.db",
  "documents_upserted": 245,
  "chunks_migrated": 8124,
  "batch_size": 512,
  "embedding_format": "json_array",
  "create_indexes": true,
  "vector_index_mode": "brute_force",
  "duration_ms": 173.41
}
```

## Suggested export query from Postgres

Shape the export so every row is self-contained:

```sql
SELECT json_build_object(
  'id', chunk_id,
  'doc_id', doc_id,
  'content', content,
  'metadata', metadata,
  'embedding', embedding,
  'source', source_path
)
FROM chunk_store;
```

Then write each JSON object as one line in `export.jsonl`.

## Validate SQL semantics after migration

```bash
cargo run -- sql --db sqlrite.db --execute "
SELECT id,
       1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score,
       bm25_score('local-first memory', content) AS text_score,
       hybrid_score(
           1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')),
           bm25_score('local-first memory', content),
           0.65
       ) AS hybrid
FROM chunks
ORDER BY hybrid DESC, id ASC
LIMIT 10;"
```

Planner inspection:

```bash
cargo run -- sql --db sqlrite.db --execute "
EXPLAIN RETRIEVAL
SELECT id,
       1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score,
       bm25_score('local-first memory', content) AS text_score,
       hybrid_score(
           1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')),
           bm25_score('local-first memory', content),
           0.65
       ) AS hybrid
FROM chunks
ORDER BY hybrid DESC, id ASC
LIMIT 10;"
```

Check:

- `execution_path.vector`
- score attribution fields
- deterministic `ORDER BY ... id ASC`

## Reproducible validation

Run:

```bash
bash scripts/run-s30-migration-suite.sh
```
