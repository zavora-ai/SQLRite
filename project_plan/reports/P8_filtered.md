# P8 Filtered Benchmark Report

Tenant filters enabled: `True` with `4` tenants.

| Mode | QPS | p95 ms | Top1 hit rate |
|---|---:|---:|---:|
| brute_force | 171.31 | 16.0142 | 1.0000 |
| hnsw_baseline | 304.08 | 6.0075 | 1.0000 |

HNSW QPS delta vs brute force: 132.77

HNSW p95 gain vs brute force (ms): 10.0067
