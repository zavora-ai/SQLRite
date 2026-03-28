# Performance

SQLRite is optimized first for embedded retrieval.

## Current benchmark snapshot

Deterministic filtered cosine workload:

- `5k` records
- `120` measured queries
- `64` dimensions
- `8` tenants
- `top_k=10`

### SQLRite results

| Mode | QPS | p95 latency | Recall@10 |
|---|---:|---:|---:|
| `brute_force` embedded | `3380.07` | `0.3543 ms` | `1.0` |
| `hnsw_baseline` embedded | `3530.96` | `0.3327 ms` | `1.0` |
| `brute_force` HTTP compact | `1807.27` | `0.7538 ms` | `1.0` |
| `hnsw_baseline` HTTP compact | `1828.17` | `0.7070 ms` | `1.0` |

### Comparator snapshot on the same workload

| Engine | Mode | QPS | Recall@10 |
|---|---|---:|---:|
| SQLRite | `brute_force` embedded | `3380.07` | `1.0` |
| sqlite-vec | exact | `3163.27` | `1.0` |
| Qdrant | exact | `2576.75` | `1.0` |
| pgvector | exact | `1739.64` | `1.0` |
| SQLRite | `hnsw_baseline` embedded | `3530.96` | `1.0` |
| Qdrant | HNSW | `2661.91` | `1.0` |
| pgvector | HNSW | `1924.01` | `0.5740` |
| LanceDB | IVF_FLAT | `1331.18` | `0.9510` |

## What these numbers mean

- embedded mode is the strongest SQLRite deployment path
- compact HTTP reduces transport overhead materially compared with the older full-response path
- the product-facing recommendation remains: embed SQLRite when you can, serve it only when you need a process boundary

## Tuning knobs

| Variable | Purpose |
|---|---|
| `SQLRITE_VECTOR_STORAGE` | choose `f32`, `f16`, or `int8` |
| `SQLRITE_SQLITE_MMAP_SIZE` | raise SQLite mmap size |
| `SQLRITE_SQLITE_CACHE_SIZE_KIB` | raise SQLite cache size |
| `SQLRITE_ENABLE_ANN_PERSISTENCE` | persist ANN sidecar state |
| `SQLRITE_ANN_MIN_CANDIDATES` | control ANN floor |
| `SQLRITE_ANN_MAX_CANDIDATE_MULTIPLIER` | cap ANN expansion |

## Practical guidance

| Goal | Recommendation |
|---|---|
| fastest local retrieval | embed SQLRite directly |
| low memory footprint | try `SQLRITE_VECTOR_STORAGE=int8` |
| faster service path | use `/v1/query-compact` |
| tighter filtered search | keep metadata filters narrow and explicit |
