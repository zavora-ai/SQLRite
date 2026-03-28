# P8 Filtered Sweep Report

## Tenant Count Sweep

| Tenants | brute_force QPS | hnsw QPS | HNSW delta QPS | brute_force p95 ms | hnsw p95 ms | HNSW p95 gain ms |
|---:|---:|---:|---:|---:|---:|---:|
| 2 | 302.06 | 287.90 | -14.16 | 3.5578 | 3.8128 | -0.2550 |
| 4 | 354.11 | 396.78 | 42.67 | 3.1612 | 3.2870 | -0.1258 |
| 8 | 429.42 | 599.51 | 170.09 | 2.5768 | 1.9124 | 0.6643 |
| 16 | 467.29 | 730.06 | 262.77 | 2.2261 | 1.5266 | 0.6995 |

## Filter Mode Sweep (8 tenants)

| Filter mode | brute_force QPS | hnsw QPS | HNSW delta QPS | brute_force p95 ms | hnsw p95 ms | HNSW p95 gain ms |
|---|---:|---:|---:|---:|---:|---:|
| tenant | 429.42 | 599.51 | 170.09 | 2.5768 | 1.9124 | 0.6643 |
| tenant_and_topic | 395.91 | 532.52 | 136.61 | 2.7889 | 2.0170 | 0.7719 |
| topic | 433.19 | 586.73 | 153.54 | 2.4636 | 1.9006 | 0.5630 |

