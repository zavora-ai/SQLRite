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

## 4. SEARCH SQL v2 Prototype

Use `SEARCH(...)` when you want concise hybrid retrieval without repeating the scoring expression:

```sql
SELECT chunk_id, doc_id, hybrid_score
FROM SEARCH(
       'local agent memory',
       vector('0.95,0.05,0.0'),
       5,
       0.65,
       500,
       'balanced',
       '{"tenant":"demo"}',
       NULL
     )
ORDER BY hybrid_score DESC, chunk_id ASC;
```

Argument order:

1. `query_text`
2. `query_embedding`
3. `top_k`
4. `alpha`
5. `candidate_limit`
6. `query_profile`
7. `metadata_filters_json`
8. `doc_id`

## 5. Tenant-Scoped Retrieval

```sql
SELECT id, doc_id, content
FROM chunks
WHERE json_extract(metadata, '$.tenant') = 'demo'
ORDER BY id ASC
LIMIT 10;
```

## 6. Metadata Filter Retrieval

```sql
SELECT id, doc_id, content
FROM chunks
WHERE json_extract(metadata, '$.topic') = 'retrieval'
ORDER BY id ASC
LIMIT 10;
```

## 7. Doc-Scoped Retrieval

```sql
SELECT id, doc_id, content
FROM chunks
WHERE doc_id = 'doc-a'
ORDER BY id ASC
LIMIT 10;
```

## 8. Rerank-Ready Candidate Export

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

## 9. Explain Retrieval Path and Scoring

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

## 10. Inspect Retrieval Index Catalog

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

## API Parity (Sprint 21)

S21 adds OpenAPI + gRPC-style query surfaces mapped to cookbook patterns.

Start server:

```bash
cargo run -- serve --db sqlrite_cookbook.db --bind 127.0.0.1:8099
```

Fetch OpenAPI contract:

```bash
curl -fsS http://127.0.0.1:8099/v1/openapi.json | jq '.paths | keys'
```

Semantic retrieval (`query_text`):

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"local agent memory","top_k":3}' \
  http://127.0.0.1:8099/v1/query | jq
```

Vector retrieval (`query_embedding`):

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_embedding":[0.95,0.05,0.0],"top_k":3}' \
  http://127.0.0.1:8099/v1/query | jq
```

Hybrid tuning (`alpha`) with metadata filter:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent","top_k":3,"alpha":0.65,"metadata_filters":{"tenant":"demo"}}' \
  http://127.0.0.1:8099/v1/query | jq
```

Doc-scoped retrieval:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent","top_k":3,"doc_id":"doc-a"}' \
  http://127.0.0.1:8099/v1/query | jq
```

gRPC-style HTTP JSON bridge (query):

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent","top_k":3}' \
  http://127.0.0.1:8099/grpc/sqlrite.v1.QueryService/Query | jq
```

gRPC-style HTTP JSON bridge (SQL):

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"statement":"SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 3;"}' \
  http://127.0.0.1:8099/grpc/sqlrite.v1.QueryService/Sql | jq
```
