# SQLRite Benchmarks

This directory contains benchmark profile definitions used by SQLRite benchmark tooling.

## What lives here

| Path | Purpose |
|---|---|
| `benchmarks/profiles/` | reusable benchmark profile definitions |

The benchmark CLIs are product features:

- `sqlrite benchmark`
- `sqlrite-bench-matrix`
- `sqlrite-bench-suite`
- `sqlrite-eval`

## Available profiles

- `profiles/10k.json`
- `profiles/100k.json`
- `profiles/1m.json`
- `profiles/10m.json`

These profiles are useful when you want consistent synthetic corpus sizes across local runs.

## Quick examples

Single benchmark:

```bash
sqlrite benchmark \
  --corpus 8000 \
  --queries 350 \
  --warmup 80 \
  --embedding-dim 64 \
  --top-k 10 \
  --candidate-limit 400 \
  --index-mode hnsw_baseline \
  --output bench_report.json
```

Profile matrix:

```bash
sqlrite-bench-matrix --profile quick --output bench_matrix.json
```

Suite plus evaluation:

```bash
sqlrite-bench-suite \
  --profiles quick,10k \
  --concurrency-profile quick \
  --concurrency-levels 1,2,4 \
  --dataset examples/eval_dataset.json \
  --output bench_suite.json
```

## Public benchmark guidance

For current product-facing benchmark numbers and interpretation, use:

- `/Users/jameskaranja/Developer/projects/SQLRight/docs/performance.md`
- `/Users/jameskaranja/Developer/projects/SQLRight/README.md`
