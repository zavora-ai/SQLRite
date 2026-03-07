# SQL Retrieval Guide

This guide explains SQLRite's SQL-native retrieval surface.

If you want retrieval from SQL instead of composing CLI query flags, this is the guide to start with.

## What SQLRite Adds to SQL

| Feature | Purpose |
|---|---|
| vector distance operators | compare stored vectors to query vectors |
| retrieval helper functions | build retrieval expressions without custom app code |
| retrieval index DDL | declare vector and text indexes |
| `SEARCH(...)` | concise SQL-native hybrid retrieval |
| `EXPLAIN RETRIEVAL` | inspect retrieval planning and scoring |

## Before You Start

Create or reuse a demo database:

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
```

## 1. Open the Interactive SQL Shell

```bash
sqlrite sql --db sqlrite_demo.db
```

Useful shell helpers:

| Helper | Purpose |
|---|---|
| `.help` | list shell helpers |
| `.tables` | list tables |
| `.schema [table]` | inspect schema |
| `.example` | list built-in examples |
| `.example lexical --run` | run a lexical example |
| `.example hybrid --run` | run a hybrid example |
| `.example vector_ddl --run` | run vector index DDL |
| `.example index_catalog --run` | inspect registered retrieval indexes |
| `.exit` | leave the shell |

## 2. Run One-Shot SQL

Use `--execute` when you want a single command rather than an interactive session.

```bash
sqlrite sql --db sqlrite_demo.db --execute "SELECT id, doc_id FROM chunks LIMIT 3;"
```

## 3. Use Vector Distance Operators

| Operator | Meaning | Lower is better? |
|---|---|---|
| `<->` | L2 distance | yes |
| `<=>` | cosine distance | yes |
| `<#>` | negative inner product | yes |

Example:

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

Use these operators when you want explicit SQL control over vector scoring and ordering.

## 4. Use Retrieval Helper Functions

Available helpers:

| Function | Purpose |
|---|---|
| `vector(...)` | construct a vector literal |
| `embed(text)` | embed a text string inside SQL |
| `bm25_score(query, document)` | lexical relevance score |
| `hybrid_score(vector_score, text_score, alpha)` | weighted fusion |
| `vec_dims(vector_expr)` | inspect vector dimensionality |
| `vec_to_json(vector_expr)` | serialize vector values |

Example:

```bash
sqlrite sql --db sqlrite_demo.db --execute "
SELECT vec_dims(embed('agent local memory')) AS dims,
       bm25_score('agent memory', 'agent systems keep local memory') AS bm25,
       hybrid_score(0.8, 0.2, 0.75) AS hybrid;"
```

## 5. Declare Retrieval Indexes

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

Use retrieval DDL when you want declarative index management rather than only runtime flags.

## 6. Use `SEARCH(...)` for Concise Hybrid Retrieval

`SEARCH(...)` is the shortest SQL-native path to hybrid retrieval.

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

Argument map:

| Position | Meaning |
|---|---|
| 1 | query text |
| 2 | query vector |
| 3 | top-k |
| 4 | alpha |
| 5 | candidate limit |
| 6 | query profile |
| 7 | document scope or `NULL` |
| 8 | metadata filter JSON or `NULL` |

## 7. Inspect with `EXPLAIN RETRIEVAL`

Use this when you want to understand how SQLRite plans a retrieval query.

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

What it shows:

- vector execution path
- text execution mode
- scoring attribution
- deterministic ordering hints
- underlying SQLite plan rows

## When to Use CLI vs SQL

| Need | Best tool |
|---|---|
| simple application query | `sqlrite query` |
| ad hoc analysis | `sqlrite sql` |
| fully SQL-native retrieval expressions | SQL functions and operators |
| concise hybrid retrieval in SQL | `SEARCH(...)` |
| plan introspection | `EXPLAIN RETRIEVAL` |

## Next Steps

1. Use `official_docs/ingestion/ingestion_and_reindexing.md` if you need to load your own data.
2. Use `official_docs/integrations/server_and_api_guide.md` if the same SQL needs to run over HTTP or gRPC.
3. Use `project_docs/sql_cookbook.md` for the deeper reference catalog.
