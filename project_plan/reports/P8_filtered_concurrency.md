# P8 Filtered Concurrency Report

## Low Selectivity (`tenant_count=2`)

| Concurrency | brute_force QPS | hnsw QPS | HNSW delta QPS | brute_force p95 ms | hnsw p95 ms | HNSW p95 gain ms |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 158.19 | 162.12 | 3.94 | 14.9890 | 13.0244 | 1.9646 |
| 2 | 284.06 | 435.42 | 151.36 | 8.9897 | 6.0752 | 2.9145 |
| 4 | 535.21 | 992.73 | 457.52 | 9.8498 | 4.9062 | 4.9436 |
| 8 | 690.52 | 1175.14 | 484.63 | 14.4086 | 8.6841 | 5.7245 |

## High Selectivity (`tenant_count=8`)

| Concurrency | brute_force QPS | hnsw QPS | HNSW delta QPS | brute_force p95 ms | hnsw p95 ms | HNSW p95 gain ms |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 355.89 | 548.54 | 192.65 | 3.6994 | 2.3880 | 1.3115 |
| 2 | 259.36 | 1127.57 | 868.21 | 11.1496 | 2.0416 | 9.1080 |
| 4 | 1057.46 | 2089.23 | 1031.76 | 4.8466 | 2.1931 | 2.6535 |
| 8 | 1116.93 | 1457.31 | 340.38 | 8.6702 | 6.4625 | 2.2078 |

