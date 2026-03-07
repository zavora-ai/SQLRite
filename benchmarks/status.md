# SQLRite Benchmark Status

Last updated: February 28, 2026

## Current benchmark posture

SQLRite now has:

- deterministic synthetic benchmark harness (`sqlrite-bench`)
- profile matrix runner (`sqlrite-bench-matrix`)
- reproducible benchmark/eval suite runner (`sqlrite-bench-suite`)
- reproducible reports in JSON for comparison over time
- throughput and memory estimate fields in benchmark output
- explicit benchmark concurrency field in benchmark output
- assertion CLI (`sqlrite-bench-assert`) for automated regression gates
- CI workflow gate (`.github/workflows/ci.yml`) that runs quick profile assertions
- CI OS smoke matrix for benchmark CLI (`ubuntu`/`macos`/`windows`)
- CI target matrix checks for Linux/macOS/Windows x64/arm64 targets
- scheduled perf workflow (`.github/workflows/perf-nightly.yml`) for 10k/100k trend checks plus suite artifact generation
- phase-C benchmark bundle script (`scripts/run-benchmark-bundle.sh`) for reproducible S13 artifacts
- suite gate assertion CLI (`sqlrite-bench-suite-assert`) for threshold checks from suite JSON
- `hnsw_baseline` benchmark matrix scenario
- vector storage telemetry (`f32`/`f16`/`int8`) in benchmark JSON
- sqlite runtime telemetry (`sqlite_mmap_size_bytes`, `sqlite_cache_size_kib`) in benchmark JSON

## Sprint 8-10 snapshot

Sources:

1. `project_plan/reports/s08_bench_matrix.json`
2. `project_plan/reports/s09_benchmark_f32.json`
3. `project_plan/reports/s09_benchmark_f16.json`
4. `project_plan/reports/s09_benchmark_int8.json`
5. `project_plan/reports/s10_benchmark_default.json`
6. `project_plan/reports/s10_benchmark_tuned.json`

| Scenario | QPS | p95 (ms) | top1_hit_rate |
|---|---:|---:|---:|
| weighted + hnsw_baseline (quick matrix) | 221.50 | 5.38 | 1.0000 |
| weighted + lsh_ann (quick matrix) | 243.34 | 4.30 | 1.0000 |
| weighted + brute_force (quick matrix) | 152.46 | 8.05 | 1.0000 |

Storage-kind impact (`hnsw_baseline`, corpus=5000, queries=250):

| Storage | QPS | p95 (ms) | index_estimated_memory_bytes |
|---|---:|---:|---:|
| f32 | 190.30 | 5.44 | 2236392 |
| f16 | 186.93 | 5.85 | 1596392 |
| int8 | 161.18 | 7.54 | 1296392 |

SQLite tuning comparison (`hnsw_baseline`, corpus=8000, queries=350):

| Profile | mmap_size_bytes | cache_size_kib | QPS | p95 (ms) |
|---|---:|---:|---:|---:|
| default | 268435456 | 65536 | 134.16 | 9.06 |
| tuned | 536870912 | 131072 | 141.45 | 7.74 |

## Sprint 11 ingestion/compaction snapshot

Sources:

1. `project_plan/reports/s11_ingest_no_adaptive.json`
2. `project_plan/reports/s11_ingest_adaptive.json`
3. `project_plan/reports/s11_compaction.json`
4. `project_plan/reports/s11_compaction_dedupe.json`
5. `project_plan/reports/s11_benchmark.json`

Ingestion throughput (fixed chunking, 4.6MB source):

| Mode | total_chunks | duration_ms | chunks_per_min | avg_batch | peak_batch |
|---|---:|---:|---:|---:|---:|
| no adaptive | 21286 | 1678.90 | 760711.65 | 63.92 | 64 |
| adaptive | 21286 | 1435.28 | 889831.46 | 788.37 | 1024 |

Observed:

1. Adaptive batching improved throughput by ~16.97%.
2. Current run exceeds the S11 floor (`50000 chunks/min`) by ~17.8x.

Compaction evidence:

| Scenario | before_chunks | after_chunks | deduplicated_chunks | reclaimed_bytes | duration_ms |
|---|---:|---:|---:|---:|---:|
| maintenance run | 21286 | 21286 | 0 | 704512 | 190.39 |
| dedupe smoke | 3 | 2 | 1 | 0 | 5.98 |

## Sprint 12 benchmark/eval suite snapshot

Sources:

1. `project_plan/reports/s12_bench_suite.json`
2. `project_plan/reports/s12_bench_suite.log`
3. `project_plan/reports/s12_quality_gates.log`

Local suite configuration:

- profiles: `quick,10k`
- concurrency sweep profile: `10k`
- concurrency levels: `1,2,4`
- dataset: `examples/eval_dataset.json`
- embedding model label: `deterministic-local-v1`
- hardware class label: `local-Darwin-arm64`

Selected observations:

1. 10k profile (`weighted + brute_force`) reported `qps=76.50`, `p95_ms=16.511`, `top1_hit_rate=1.0000`.
2. 10k profile (`weighted + lsh_ann`) reported `qps=86.39`, `p95_ms=14.288`, `top1_hit_rate=0.9980`.
3. Concurrency sweep (10k, weighted + brute_force): `conc=1 qps=87.36`, `conc=2 qps=35.14`, `conc=4 qps=37.26`.
4. Eval metrics stayed stable across `brute_force`, `lsh_ann`, `hnsw_baseline` on this dataset (`k=1 recall=0.8333`, `mrr=1.0000`, `ndcg=1.0000`).

Notes:

1. 100k/1m matrix runs are wired in nightly/dispatch workflows for reproducible CI artifacts.
2. 10m profile is now available in benchmark profile selection (`sqlrite-bench-matrix`, `sqlrite-bench-suite`) for large-scale phase continuation in S13.
3. S13 introduces profile definition files and bundle automation for publishing reproducible benchmark packages.

## Sprint 13 phase-C gate snapshot

Sources:

1. `project_plan/reports/s13_bundle_local/bench_suite.json`
2. `project_plan/reports/s13_bundle_local/bench_suite.log`
3. `project_plan/reports/s13_bundle_local/phase_c_gate.log`
4. `project_plan/reports/s13_quality_gates.log`

Gate-focused profile/scenario set:

- profiles: `100k,1m`
- scenarios: `weighted + lsh_ann`, `weighted + hnsw_baseline`
- strict gate result: `passed`

Selected observations:

1. 100k (`weighted + lsh_ann`): `qps=40.82`, `p95_ms=26.241`, `top1_hit_rate=1.0000`
2. 1m (`weighted + hnsw_baseline`): `qps=15.89`, `p95_ms=81.519`, `top1_hit_rate=0.8655`
3. 100k ingest throughput from gated scenario: `ingest_chunks_per_sec=11498.56` (`689,913 chunks/min`)
4. Quick concurrency sweep evidence (`quick`, brute_force): `conc=1 qps=144.27`, `conc=2 qps=76.20`

Interpretation:

1. PC-G01 target is met (`100k p95 < 40ms`).
2. PC-G02 target is met (`1m p95 < 90ms`).
3. PC-G03 target is met (`ingest >= 50k chunks/min`).

## 10k profile progression

Configuration:

- corpus: 10,000 chunks
- queries: 500
- warmup: 100
- embedding dim: 128
- top_k: 10
- candidate_limit: 500
- durability: balanced

### Baseline (v1)

Source: `benchmarks/results/legacy/bench_matrix_10k.json`

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate |
|---|---:|---:|---:|---:|
| weighted + brute_force | 16.17 | 54.73 | 100.44 | 1.0000 |
| rrf(k=60) + brute_force | 19.76 | 49.27 | 54.44 | 0.0560 |
| weighted + disabled_index | 89.26 | 10.56 | 11.44 | 0.0720 |

### After normalized vectors + improved FTS/text scoring (v2)

Source: `benchmarks/results/legacy/bench_matrix_10k_v2.json`

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate |
|---|---:|---:|---:|---:|
| weighted + brute_force | 40.61 | 23.95 | 25.70 | 1.0000 |
| rrf(k=60) + brute_force | 39.27 | 25.14 | 26.79 | 0.0560 |
| weighted + disabled_index | 92.88 | 10.52 | 11.06 | 0.0720 |

### After parallelized brute-force scan (v3)

Source: `benchmarks/results/legacy/bench_matrix_10k_v3.json`

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate |
|---|---:|---:|---:|---:|
| weighted + brute_force | 86.67 | 10.89 | 13.32 | 1.0000 |
| rrf(k=60) + brute_force | 84.56 | 11.52 | 12.85 | 0.0560 |
| weighted + disabled_index | 99.83 | 9.98 | 10.09 | 0.0720 |

### After `lsh_ann` backend integration (v4)

Source: `benchmarks/results/legacy/bench_matrix_10k_v4.json`

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate |
|---|---:|---:|---:|---:|
| weighted + brute_force | 85.94 | 11.55 | 12.24 | 1.0000 |
| rrf(k=60) + brute_force | 72.56 | 12.48 | 22.37 | 0.0560 |
| weighted + lsh_ann | 99.95 | 9.96 | 10.94 | 0.9960 |
| weighted + disabled_index | 94.47 | 10.29 | 11.25 | 0.0720 |

### After transactional batch ingest + adaptive FTS planning (v6)

Source: `benchmarks/results/legacy/bench_matrix_10k_v8.json`

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate |
|---|---:|---:|---:|---:|
| weighted + brute_force | 92.66 | 10.69 | 11.17 | 1.0000 |
| rrf(k=60) + brute_force | 86.64 | 11.52 | 11.75 | 0.0560 |
| weighted + lsh_ann | 103.40 | 9.64 | 10.54 | 0.9960 |
| weighted + disabled_index | 87.47 | 11.37 | 11.65 | 0.2980 |

### ANN-tuned telemetry run (v9)

Source: `benchmarks/results/legacy/bench_matrix_10k_v11.json`

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate | ingest_cps | approx_work_mb |
|---|---:|---:|---:|---:|---:|---:|
| weighted + brute_force | 90.42 | 10.95 | 11.40 | 1.0000 | 25,622.1 | 10.78 |
| rrf(k=60) + brute_force | 74.67 | 12.10 | 21.36 | 0.0560 | 27,769.8 | 10.78 |
| weighted + lsh_ann | 101.58 | 9.93 | 10.50 | 0.9980 | 6,053.5 | 11.75 |
| weighted + disabled_index | 88.61 | 11.27 | 11.41 | 0.2980 | 29,137.7 | 5.38 |

### Batch-upsert ingest optimization run (v10, current)

Source: `benchmarks/results/legacy/bench_matrix_10k_v14.json`

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate | ingest_cps | approx_work_mb |
|---|---:|---:|---:|---:|---:|---:|
| weighted + brute_force | 71.70 | 12.62 | 19.96 | 1.0000 | 25,457.6 | 10.78 |
| rrf(k=60) + brute_force | 60.83 | 14.33 | 26.86 | 0.0560 | 22,992.4 | 10.78 |
| weighted + lsh_ann | 95.03 | 10.47 | 11.69 | 0.9980 | 5,544.5 | 11.75 |
| weighted + disabled_index | 83.32 | 11.83 | 12.67 | 0.2980 | 28,535.4 | 5.38 |

## Interpretation

- High-accuracy mode (`weighted + brute_force`) remains well above baseline in repeated runs, with stable quality (`top1_hit_rate = 1.0000`).
- `weighted + lsh_ann` remains the throughput leader with improved quality (`top1_hit_rate = 0.9980`).
- Text-heavy, no-index mode improved materially in relevance quality: `weighted + disabled_index` top1 hit rate increased from **0.0720** to **0.2980**.
- Transactional batch ingestion reduced ingest time in the 10k profile:
  - `weighted + brute_force`: **667.10ms -> 381.90ms** (about **42.7% faster**)
  - `weighted + disabled_index`: **682.70ms -> 337.00ms** (about **50.6% faster**)
- Adaptive FTS planning recovered hybrid latency and ANN tuning keeps `lsh_ann` 10k p95 near **10-12ms** while increasing top1 quality.
- 10k runs can show machine-load variance; perf gates use threshold ranges rather than single-number targets.

## 100k profile progression

Source: `benchmarks/results/legacy/bench_matrix_100k_v1.json`, `benchmarks/results/legacy/bench_matrix_100k_v2.json`, `benchmarks/results/legacy/bench_matrix_100k_v3.json`

Configuration:

- corpus: 100,000 chunks
- queries: 1,000
- warmup: 200
- embedding dim: 256
- top_k: 10
- candidate_limit: 1000
- durability: balanced

### 100k baseline (v1)

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate | ingest_cps | approx_work_mb |
|---|---:|---:|---:|---:|---:|---:|
| weighted + brute_force | 9.39 | 104.57 | 110.89 | 1.0000 | 22,214.0 | 205.52 |
| rrf(k=60) + brute_force | 9.21 | 106.60 | 113.17 | 0.0580 | 21,790.7 | 205.52 |
| weighted + lsh_ann | 10.81 | 80.18 | 165.93 | 0.9950 | 4,781.6 | 217.04 |
| weighted + disabled_index | 12.00 | 81.29 | 88.64 | 0.0770 | 23,700.8 | 102.71 |

### After LSH candidate-planner tuning (v2)

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate | ingest_cps | approx_work_mb |
|---|---:|---:|---:|---:|---:|---:|
| weighted + brute_force | 9.50 | 103.70 | 112.61 | 1.0000 | 22,857.3 | 205.52 |
| rrf(k=60) + brute_force | 9.24 | 105.98 | 117.22 | 0.0580 | 22,008.1 | 205.52 |
| weighted + lsh_ann | 15.77 | 61.68 | 70.03 | 1.0000 | 3,424.5 | 224.37 |
| weighted + disabled_index | 12.13 | 81.01 | 88.21 | 0.0770 | 23,851.3 | 102.71 |

### After ID-compacted ANN bucket storage (v3)

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate | ingest_cps | approx_work_mb |
|---|---:|---:|---:|---:|---:|---:|
| weighted + brute_force | 9.30 | 106.98 | 110.43 | 1.0000 | 22,207.4 | 205.52 |
| rrf(k=60) + brute_force | 7.54 | 113.40 | 180.39 | 0.0580 | 22,048.1 | 205.52 |
| weighted + lsh_ann | 16.00 | 61.59 | 65.43 | 1.0000 | 3,280.6 | 214.83 |
| weighted + disabled_index | 11.99 | 82.56 | 87.82 | 0.0770 | 24,210.4 | 102.71 |

### After batch-upsert ingest path (v4, current)

Source: `benchmarks/results/legacy/bench_matrix_100k_v4.json`

| Scenario | QPS | p50 (ms) | p95 (ms) | top1_hit_rate | ingest_cps | approx_work_mb |
|---|---:|---:|---:|---:|---:|---:|
| weighted + brute_force | 8.90 | 110.55 | 120.16 | 1.0000 | 22,823.5 | 205.52 |
| rrf(k=60) + brute_force | 8.17 | 110.71 | 173.25 | 0.0580 | 22,995.5 | 205.52 |
| weighted + lsh_ann | 16.23 | 61.34 | 65.91 | 1.0000 | 11,859.1 | 214.83 |
| weighted + disabled_index | 11.25 | 84.52 | 112.40 | 0.0770 | 23,948.2 | 102.71 |

Initial read:

- `weighted + lsh_ann` now leads clearly on 100k throughput and latency while preserving perfect top1 (`1.0000`) in this workload.
- 100k `weighted + lsh_ann` improved from **10.81 -> 16.23 QPS** (about **50.1%**), and p95 improved from **165.93ms -> 65.91ms**.
- ID-compacted bucket storage reduced 100k `lsh_ann` working set from **224.37 MB -> 214.83 MB**.
- Batch-upsert path increased 100k `lsh_ann` ingest throughput from **3,280.6 -> 11,859.1 chunks/sec** (about **3.62x**).

## Remaining gap to “SQLite-class” performance target

The current implementation is much faster than baseline, but this is still a Rust-side retrieval layer, not equivalent to SQLite C engine internals for all workloads.
To continue closing the gap for larger corpora:

1. tune `lsh_ann` ingest throughput while preserving the new 100k query gains (currently lower than brute-force/disabled)
2. run and analyze `1m` profile with the same memory/throughput telemetry
3. tighten CI perf thresholds over time (quick now, then add 10k/100k scheduled runs)
4. start Phase 3 ingestion worker and checkpoint pipeline work
