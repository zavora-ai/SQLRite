# Public Dataset Benchmark Results

Dataset: BEIR/SciFact with deterministic hashed embeddings and shared query relevance judgments.

## Workload

- dataset: `BEIR/SciFact`
- corpus size: `5183`
- query count: `100`
- embedding dimension: `128`
- top-k: `10`
- alpha: `0.5`

## Vector Exact Benchmark

| System | QPS | p50 ms | p95 ms | Recall@k | MRR@k | NDCG@k |
|---|---:|---:|---:|---:|---:|---:|
| SQLRite brute_force compact_http | 204.76 | 4.018 | 6.773 | 0.2278 | 0.1424 | 0.1594 |
| sqlite-vec exact | 2297.83 | 0.395 | 0.519 | 0.2278 | 0.1424 | 0.1594 |
| pgvector exact | 569.76 | 1.521 | 3.458 | 0.2278 | 0.1424 | 0.1594 |

## Hybrid Lexical + Vector Benchmark

| System | QPS | p50 ms | p95 ms | Recall@k | MRR@k | NDCG@k |
|---|---:|---:|---:|---:|---:|---:|
| SQLRite hybrid compact_http | 55.60 | 16.296 | 19.511 | 0.4028 | 0.4056 | 0.3973 |
| pgvector hybrid | 23.66 | 30.010 | 112.778 | 0.2278 | 0.1442 | 0.1609 |
