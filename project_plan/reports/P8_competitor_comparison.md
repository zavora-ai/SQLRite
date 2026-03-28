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
| SQLRite brute_force | 176.68 | 5.610 | 6.407 | 1.0000 | 1.0000 | 0.136 |
| sqlite-vec exact | 3431.12 | 0.288 | 0.309 | 1.0000 | 1.0000 | 0.028 |
| LanceDB exact | 1212.08 | 0.785 | 1.051 | 1.0000 | 1.0000 | 0.013 |
| Qdrant exact | 2642.00 | 0.329 | 0.740 | 1.0000 | 1.0000 | 0.190 |
| pgvector exact | 2018.62 | 0.481 | 0.598 | 1.0000 | 1.0000 | 0.194 |

## Approx Filtered Cosine

| System | QPS | p50 ms | p95 ms | Top1 hit | Recall@k | Setup s |
|---|---:|---:|---:|---:|---:|---:|
| SQLRite hnsw_baseline | 396.47 | 1.984 | 5.319 | 1.0000 | 1.0000 | 0.136 |
| LanceDB IVF_FLAT | 855.82 | 0.755 | 2.207 | 1.0000 | 0.9433 | 0.039 |
| Qdrant HNSW | 2431.40 | 0.356 | 0.699 | 1.0000 | 1.0000 | 0.190 |
| pgvector HNSW | 1962.05 | 0.452 | 0.696 | 1.0000 | 0.5731 | 0.194 |

