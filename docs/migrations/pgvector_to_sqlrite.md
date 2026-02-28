# Migration Guide: pgvector to SQLRite

This guide maps common pgvector retrieval patterns to SQLRite SQL and data model.

## Query syntax mapping

| pgvector/Postgres | SQLRite |
| --- | --- |
| `embedding <-> '[...]'` | `embedding <-> vector('...')` |
| `embedding <=> '[...]'` | `embedding <=> vector('...')` |
| `embedding <#> '[...]'` | `embedding <#> vector('...')` |
| `ts_rank`/`BM25` style lexical score | `bm25_score(query, content)` or FTS5 `bm25(chunks_fts)` |
| weighted fusion in SQL | `hybrid_score(vector_score, text_score, alpha)` |

## 1. Initialize SQLRite target

```bash
cargo run -- init --db sqlrite_from_pgvector.db --profile balanced --index-mode brute_force
```

## 2. Export from Postgres

Export chunks with:

- stable chunk id
- doc id
- content
- metadata JSON
- vector embedding values

Use CSV/NDJSON in your ETL, then insert with `sqlrite ingest` or direct SQL load.

## 3. Ingest into SQLRite

Single-row example:

```bash
cargo run -- ingest \
  --db sqlrite_from_pgvector.db \
  --id chunk-001 \
  --doc-id doc-001 \
  --content "local-first memory chunk" \
  --embedding 0.95,0.05,0.0 \
  --metadata '{"tenant":"acme","topic":"retrieval"}'
```

## 4. Register index metadata

```bash
cargo run -- sql --db sqlrite_from_pgvector.db --execute \
  "CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw ON chunks(embedding) USING HNSW;"
```

## 5. Port hybrid retrieval SQL

pgvector-style hybrid query in SQLRite:

```sql
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
LIMIT 10;
```

## 6. Verify planner path

```sql
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
LIMIT 10;
```

Check:

- `execution_path.vector` (`ann_index` or fallback)
- score attribution block
- deterministic tie-break presence in `ORDER BY ... id ASC`
