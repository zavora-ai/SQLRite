# SQLRite Benchmark Profiles

This directory publishes versioned benchmark profile definitions used by the S12/S13 benchmark tooling.

## Profiles

- `profiles/10k.json`
- `profiles/100k.json`
- `profiles/1m.json`
- `profiles/10m.json`

Each profile file captures the synthetic corpus/query dimensions and scoring parameters used by:

- `sqlrite-bench-matrix`
- `sqlrite-bench-suite`

## Reproducible Bundle Run

Use the bundle script to generate a reproducible benchmark artifact set:

```bash
bash scripts/run-benchmark-bundle.sh \
  --profiles 100k,1m \
  --concurrency-profile quick \
  --concurrency-levels 1,2 \
  --strict-phase-c-gate
```

Default bundle scenarios target S13 gates:

- `weighted + lsh_ann`
- `weighted + hnsw_baseline`

Run full scenario matrix when needed:

```bash
bash scripts/run-benchmark-bundle.sh --full-scenarios
```

Artifacts are emitted to `project_plan/reports/s13_bundle/`:

- `bench_suite.json`
- `bench_suite.log`
- `phase_c_gate.log`
- `manifest.json`
- `benchmark_bundle.tar.gz`

## Phase C Gate Assertions

`sqlrite-bench-suite-assert` checks benchmark and eval constraints directly from a suite JSON report.

Example:

```bash
cargo run --bin sqlrite-bench-suite-assert -- \
  --suite project_plan/reports/s13_bundle/bench_suite.json \
  --rule "profile=100k,scenario=weighted + lsh_ann,max_p95_ms=40,min_top1=0.99,min_ingest_cpm=50000" \
  --rule "profile=1m,scenario=weighted + hnsw_baseline,max_p95_ms=90,min_top1=0.75" \
  --eval-rule "index_mode=lsh_ann,min_recall_k1=0.80,min_mrr_k1=0.95,min_ndcg_k1=0.95"
```
