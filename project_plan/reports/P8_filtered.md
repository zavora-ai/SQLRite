# P8 Filtered Benchmark Report

Tenant filters enabled: `True` with `4` tenants and filter mode `tenant`.

| Mode | QPS | p95 ms | Top1 hit rate |
|---|---:|---:|---:|
| brute_force | 362.79 | 3.4637 | 1.0000 |
| hnsw_baseline | 436.23 | 2.7437 | 1.0000 |

HNSW QPS delta vs brute force: 73.44

HNSW p95 gain vs brute force (ms): 0.7200
