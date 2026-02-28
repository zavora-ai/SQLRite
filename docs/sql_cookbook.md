# SQLRite SQL Cookbook (Sprint 7)

This cookbook focuses on SQL-only retrieval patterns runnable through:

```bash
cargo run -- sql --db <db> --execute "<SQL>"
```

## Setup

Seed demo data:

```bash
cargo run -- init --db sqlrite_cookbook.db --seed-demo --profile balanced --index-mode brute_force
```

Register retrieval index metadata (optional but recommended for explainability):

```bash
cargo run -- sql --db sqlrite_cookbook.db --execute \
  "CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw ON chunks(embedding) USING HNSW WITH (m=16, ef_construction=64);"

cargo run -- sql --db sqlrite_cookbook.db --execute \
  "CREATE TEXT INDEX IF NOT EXISTS idx_chunks_content_fts ON chunks(content) USING FTS5 WITH (tokenizer=unicode61);"
```

## 1. Semantic Vector Retrieval

```sql
SELECT id,
       embedding <-> vector('0.95,0.05,0.0') AS l2
FROM chunks
ORDER BY l2 ASC, id ASC
LIMIT 5;
```

## 2. Lexical Retrieval (FTS/BM25)

```sql
SELECT c.id, c.doc_id, bm25(chunks_fts) AS rank
FROM chunks_fts
JOIN chunks AS c ON c.id = chunks_fts.chunk_id
WHERE chunks_fts MATCH 'local OR agent'
ORDER BY rank ASC, c.id ASC
LIMIT 5;
```

## 3. Hybrid Retrieval in One Statement

```sql
SELECT id,
       1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score,
       bm25_score('local agent memory', content) AS text_score,
       hybrid_score(
           1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')),
           bm25_score('local agent memory', content),
           0.65
       ) AS hybrid
FROM chunks
ORDER BY hybrid DESC, id ASC
LIMIT 5;
```

## 4. Tenant-Scoped Retrieval

```sql
SELECT id, doc_id, content
FROM chunks
WHERE json_extract(metadata, '$.tenant') = 'demo'
ORDER BY id ASC
LIMIT 10;
```

## 5. Metadata Filter Retrieval

```sql
SELECT id, doc_id, content
FROM chunks
WHERE json_extract(metadata, '$.topic') = 'retrieval'
ORDER BY id ASC
LIMIT 10;
```

## 6. Doc-Scoped Retrieval

```sql
SELECT id, doc_id, content
FROM chunks
WHERE doc_id = 'doc-a'
ORDER BY id ASC
LIMIT 10;
```

## 7. Rerank-Ready Candidate Export

Use SQLRite to produce candidate features (`vector_score`, `text_score`) for an external reranker:

```sql
SELECT id,
       content,
       1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score,
       bm25_score('local agent memory', content) AS text_score
FROM chunks
ORDER BY vector_score DESC, text_score DESC, id ASC
LIMIT 20;
```

## 8. Explain Retrieval Path and Scoring

```sql
EXPLAIN RETRIEVAL
SELECT id,
       1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score,
       bm25_score('local agent memory', content) AS text_score,
       hybrid_score(
           1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')),
           bm25_score('local agent memory', content),
           0.65
       ) AS hybrid
FROM chunks
ORDER BY hybrid DESC, id ASC
LIMIT 5;
```

`EXPLAIN RETRIEVAL` reports:

- `execution_path.vector` as `ann_index` or `brute_force_fallback`
- `execution_path.text` path detail
- score attribution (`fusion_mode`, `hybrid_alpha`)
- deterministic ordering indicators
- underlying `EXPLAIN QUERY PLAN` rows

## 9. Inspect Retrieval Index Catalog

```sql
SELECT name, index_kind, table_name, column_name, using_engine, options_json, status
FROM retrieval_index_catalog
ORDER BY name;
```

## SQL-Only Conformance

Run all cookbook patterns and generate artifacts:

```bash
bash scripts/run-sql-cookbook-conformance.sh
```

Artifacts:

- `project_plan/reports/s07_sql_conformance.log`
- `project_plan/reports/s07_sql_conformance.json`
