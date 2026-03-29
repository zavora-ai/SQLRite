# Operations

This guide covers health checks, backup and restore, compaction, and benchmarking.

## Health check

```bash
sqlrite doctor --db sqlrite_demo.db
```

Typical output:

```text
sqlrite doctor
- version=1.0.1
- integrity_ok=true
- schema_version=3
- chunk_count=3
- index_mode=brute_force
- vector_storage=f32
```

## Backup and verify

```bash
sqlrite backup --source sqlrite_demo.db --dest sqlrite_backup.db
sqlrite backup verify --path sqlrite_backup.db
```

## Snapshots and point-in-time restore

```bash
sqlrite backup snapshot --source sqlrite_demo.db --backup-dir backups --note manual_snapshot --json
sqlrite backup list --backup-dir backups --json
sqlrite backup pitr-restore --backup-dir backups --target-unix-ms 1772000000000 --dest restored.db --verify
```

## Compaction

```bash
sqlrite compact --db sqlrite_demo.db --json
```

## Benchmark one configuration

```bash
sqlrite benchmark \
  --corpus 8000 \
  --queries 350 \
  --warmup 80 \
  --embedding-dim 64 \
  --top-k 10 \
  --candidate-limit 400 \
  --fusion weighted \
  --index-mode hnsw_baseline \
  --query-profile balanced \
  --output bench_report.json
```

## Benchmark suite

```bash
sqlrite-bench-suite \
  --profiles quick,10k \
  --concurrency-profile quick \
  --concurrency-levels 1,2,4 \
  --dataset examples/eval_dataset.json \
  --dataset-id local_suite \
  --embedding-model deterministic-local-v1 \
  --hardware-class local-dev \
  --output bench_suite.json
```

## Evaluation metrics

```bash
sqlrite-eval --dataset examples/eval_dataset.json --output eval_report.json --index-mode hnsw_baseline
```
