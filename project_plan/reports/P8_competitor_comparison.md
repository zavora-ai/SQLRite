# P8 Competitor Comparison Report

This report compares SQLRite with external systems on the same deterministic filtered cosine workload.

## Workload

- corpus size: `5000`
- query count: `120`
- embedding dimension: `64`
- tenants: `8`
- top-k: `10`
- warmup: `16`
- seed: `20260328`

## Exact Filtered Cosine

| System | QPS | p50 ms | p95 ms | Top1 hit | Recall@k | Setup s |
|---|---:|---:|---:|---:|---:|---:|
| SQLRite brute_force | 178.93 | 5.565 | 5.931 | 1.0000 | 1.0000 | 0.110 |
| Qdrant exact | 3014.26 | 0.318 | 0.411 | 1.0000 | 1.0000 | 0.161 |
| pgvector exact | 1558.78 | 0.478 | 1.862 | 1.0000 | 1.0000 | 0.202 |

## Approx Filtered Cosine

| System | QPS | p50 ms | p95 ms | Top1 hit | Recall@k | Setup s |
|---|---:|---:|---:|---:|---:|---:|
| SQLRite hnsw_baseline | 510.38 | 1.827 | 2.793 | 1.0000 | 1.0000 | 0.110 |
| Qdrant HNSW | 2560.09 | 0.342 | 0.695 | 1.0000 | 1.0000 | 0.161 |
| pgvector HNSW | 1566.68 | 0.542 | 1.379 | 1.0000 | 0.5702 | 0.202 |

