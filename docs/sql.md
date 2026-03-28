# SQL Retrieval

SQLRite adds retrieval-aware operators, functions, and `SEARCH(...)` to SQLite.

## What SQLRite Adds

| Feature | Purpose |
|---|---|
| vector distance operators | compare stored vectors to a query vector |
| retrieval helper functions | compute lexical or hybrid scores in SQL |
| retrieval index DDL | declare vector and text indexes |
| `SEARCH(...)` | concise SQL-native hybrid retrieval |
| `EXPLAIN RETRIEVAL` | inspect retrieval planning |

Create a demo database first:

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
```

## Interactive shell

```bash
sqlrite sql --db sqlrite_demo.db
```

Useful helpers:

- `.help`
- `.tables`
- `.schema [table]`
- `.example`
- `.exit`

## One-shot SQL

```bash
sqlrite sql --db sqlrite_demo.db --execute "SELECT id, doc_id FROM chunks LIMIT 3;"
```

## Distance operators

| Operator | Meaning |
|---|---|
| `<->` | L2 distance |
| `<=>` | cosine distance |
| `<#>` | negative inner product |

```bash
sqlrite sql --db sqlrite_demo.db --execute "
SELECT id,
       embedding <-> vector('0.95,0.05,0.0') AS l2,
       embedding <=> vector('0.95,0.05,0.0') AS cosine_distance,
       embedding <#> vector('0.95,0.05,0.0') AS neg_inner
FROM chunks
ORDER BY l2 ASC, id ASC
LIMIT 3;"
```

## Helper functions

| Function | Purpose |
|---|---|
| `vector(...)` | create a vector literal |
| `embed(text)` | embed a text string |
| `bm25_score(query, document)` | lexical score |
| `hybrid_score(vector_score, text_score, alpha)` | weighted fusion |
| `vec_dims(vector_expr)` | inspect vector dimensions |
| `vec_to_json(vector_expr)` | serialize vector data |

## Retrieval index DDL

### Vector index

```bash
sqlrite sql --db sqlrite_demo.db --execute "
CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw
ON chunks(embedding)
USING HNSW
WITH (m=16, ef_construction=64);"
```

### Text index

```bash
sqlrite sql --db sqlrite_demo.db --execute "
CREATE TEXT INDEX IF NOT EXISTS idx_chunks_content_fts
ON chunks(content)
USING FTS5;"
```

## `SEARCH(...)`

```bash
sqlrite sql --db sqlrite_demo.db --execute "
SELECT chunk_id, doc_id, hybrid_score
FROM SEARCH(
       'local memory',
       vector('0.95,0.05,0.0'),
       5,
       0.65,
       500,
       'balanced',
       NULL,
       NULL
     )
ORDER BY hybrid_score DESC, chunk_id ASC;"
```

Sample output:

```json
[
  {
    "chunk_id": "demo-1",
    "doc_id": "doc-a",
    "hybrid_score": 1.3808426976203918
  }
]
```

## `EXPLAIN RETRIEVAL`

```bash
sqlrite sql --db sqlrite_demo.db --execute "
EXPLAIN RETRIEVAL
SELECT id,
       hybrid_score(
         1.0 - (embedding <=> vector('0.95,0.05,0.0')),
         bm25_score('local memory', content),
         0.65
       ) AS score
FROM chunks
ORDER BY score DESC, id ASC
LIMIT 3;"
```
