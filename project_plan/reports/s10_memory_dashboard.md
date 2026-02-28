# S10 Memory and Tuning Dashboard

Date: February 28, 2026  
Scope: PC-D03, BE-04

## Runtime Tuning Comparison (`hnsw_baseline`, `f32`)

Source artifacts:

1. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s10_benchmark_default.json`
2. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s10_benchmark_tuned.json`

| Profile | mmap_size_bytes | cache_size_kib | qps | p95_ms | index_estimated_memory_bytes | approx_working_set_bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| default | 268435456 | 65536 | 134.16 | 9.0555 | 3551800 | 6015690 |
| tuned | 536870912 | 131072 | 141.45 | 7.7359 | 3551800 | 6015690 |

Observed delta:

1. `qps` improved from `134.16` to `141.45` (+5.43%).
2. `p95_ms` improved from `9.0555` to `7.7359` (-14.57%).

## Storage Kind Efficiency Snapshot (`hnsw_baseline`)

Source artifacts:

1. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s09_benchmark_f32.json`
2. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s09_benchmark_f16.json`
3. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s09_benchmark_int8.json`

| Storage | qps | p95_ms | index_estimated_memory_bytes | approx_working_set_bytes |
| --- | ---: | ---: | ---: | ---: |
| f32 | 190.30 | 5.4363 | 2236392 | 3775907 |
| f16 | 186.93 | 5.8491 | 1596392 | 3135907 |
| int8 | 161.18 | 7.5415 | 1296392 | 2835907 |

Observed delta vs `f32`:

1. `f16` index memory: -28.62%.
2. `int8` index memory: -42.03%.

## Operational Validation

Source artifacts:

1. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s10_doctor_tuned.json`
2. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s10_quickstart_default.json`
3. `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s10_quickstart_tuned.json`

Key checks:

1. Doctor confirms tuned runtime (`sqlite_mmap_size_bytes=536870912`, `sqlite_cache_size_kib=131072`).
2. Quickstart gate passes in both default and tuned profiles with `success_rate=1.0`.
