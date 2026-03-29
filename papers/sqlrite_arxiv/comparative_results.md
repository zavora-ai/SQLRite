# Comparative Benchmark Results

These results come from a single-host localhost benchmark using the same deterministic cosine+tenant-filter workload across all tested systems.

## Workload

- corpus size: `20000`
- query count: `200`
- embedding dimension: `32`
- tenants: `8`
- top-k: `10`
- seed: `20260308`

## Exact Filtered Cosine

| System | QPS | p50 ms | p95 ms | Top1 hit | Recall@k | Setup s |
|---|---:|---:|---:|---:|---:|---:|
| SQLRite brute_force compact_http | 771.21 | 1.194 | 1.866 | 1.0000 | 1.0000 | 0.574 |
| Qdrant exact | 1914.29 | 0.418 | 1.160 | 1.0000 | 1.0000 | 0.543 |
| pgvector exact | 600.23 | 1.069 | 3.169 | 1.0000 | 1.0000 | 0.240 |

## Approx Filtered Cosine

| System | QPS | p50 ms | p95 ms | Top1 hit | Recall@k | Setup s |
|---|---:|---:|---:|---:|---:|---:|
| SQLRite hnsw_baseline compact_http | 34.12 | 27.869 | 35.944 | 0.9944 | 0.9817 | 0.574 |
| Qdrant HNSW | 2561.25 | 0.342 | 0.536 | 1.0000 | 1.0000 | 0.543 |
| pgvector HNSW | 2122.26 | 0.410 | 0.807 | 1.0000 | 0.5861 | 0.240 |

## Caveats

- This is a single-host benchmark on one machine, not a cluster-scale study.
- The workload measures filtered cosine vector search only; it does not yet cover lexical or hybrid retrieval parity.
- SQLRite is compared here against one SQL-first competitor (`pgvector`) and one network-native vector database (`Qdrant`).
