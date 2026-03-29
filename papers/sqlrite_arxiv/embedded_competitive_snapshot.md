# Embedded Competitive Snapshot

This snapshot is the current benchmark evidence used by the March 2026 rewrite of the SQLRite paper.

## Workload

- deterministic filtered cosine workload
- corpus size: `5000`
- query count: `120`
- embedding dimension: `64`
- tenants: `8`
- top-k: `10`
- filter: exact tenant match

## SQLRite deployment-path results

| Mode | QPS | p95 ms | Recall@10 |
|---|---:|---:|---:|
| `brute_force` embedded | `3380.07` | `0.3543` | `1.0` |
| `hnsw_baseline` embedded | `3530.96` | `0.3327` | `1.0` |
| `brute_force` compact HTTP | `1807.27` | `0.7538` | `1.0` |
| `hnsw_baseline` compact HTTP | `1828.17` | `0.7070` | `1.0` |

## Comparator snapshot on the same workload

### Exact filtered cosine

| System | QPS |
|---|---:|
| SQLRite `brute_force` embedded | `3380.07` |
| sqlite-vec exact | `3163.27` |
| Qdrant exact | `2576.75` |
| pgvector exact | `1739.64` |
| LanceDB exact | `1063.08` |

### Approximate filtered cosine

| System | QPS | Recall@10 |
|---|---:|---:|
| SQLRite `hnsw_baseline` embedded | `3530.96` | `1.0` |
| Qdrant HNSW | `2661.91` | `1.0` |
| pgvector HNSW | `1924.01` | `0.5740` |
| LanceDB IVF_FLAT | `1331.18` | `0.9510` |

## Interpretation

The main conclusion is narrow and intentional:

- SQLRite is strongest in embedded mode.
- Compact HTTP narrows the service-transport penalty materially.
- On this benchmark, SQLRite leads the exact and approximate embedded filtered workload snapshot.
- This does not establish universal dominance across corpus sizes, datasets, or deployment shapes.
