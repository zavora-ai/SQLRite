# Operations and Benchmarks Guide

This guide covers health checks, backup and restore, compaction, and performance measurement.

## Operations Overview

| Task | Command |
|---|---|
| inspect health | `sqlrite doctor` |
| create a backup | `sqlrite backup` |
| create snapshots and PITR restore | `sqlrite backup snapshot`, `sqlrite backup pitr-restore` |
| compact storage | `sqlrite compact` |
| run one benchmark | `sqlrite benchmark` |
| run a benchmark suite | `sqlrite-bench-suite` |
| run evaluation metrics | `sqlrite-eval` |

## 1. Check Health

```bash
sqlrite doctor --db sqlrite_demo.db
```

Sample output:

```text
sqlrite doctor
- version=1.0.0
- integrity_ok=true
- schema_version=3
- chunk_count=3
- index_mode=brute_force
- vector_storage=f32
```

What the fields mean:

| Field | Meaning |
|---|---|
| `integrity_ok` | database integrity result |
| `schema_version` | active schema version |
| `chunk_count` | stored chunks |
| `index_mode` | retrieval index mode |
| `vector_storage` | vector storage profile |

## 2. Create and Verify a Backup

```bash
sqlrite backup --source sqlrite_demo.db --dest sqlrite_backup.db
sqlrite backup verify --path sqlrite_backup.db
```

Use this before risky maintenance or deployment changes.

## 3. Create Snapshots and Restore by Point in Time

```bash
sqlrite backup snapshot \
  --source sqlrite_demo.db \
  --backup-dir backups \
  --note "manual_snapshot" \
  --json

sqlrite backup list --backup-dir backups --json

sqlrite backup pitr-restore \
  --backup-dir backups \
  --target-unix-ms 1772000000000 \
  --dest restored.db \
  --verify
```

Use this when you need a more operational restore flow than a single flat copy.

## 4. Compact the Database

```bash
sqlrite compact --db sqlrite_demo.db --json
```

Use compaction after deletes, rewrite-heavy workloads, or repeated ingest/reindex cycles.

## 5. Run a Single Benchmark

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

What this gives you:

- throughput
- p95 latency
- recall-oriented benchmark metadata
- a reusable JSON report artifact

## 6. Run the Benchmark Suite

```bash
sqlrite-bench-suite \
  --profiles quick,10k \
  --concurrency-profile quick \
  --concurrency-levels 1,2,4 \
  --dataset examples/eval_dataset.json \
  --dataset-id readme_suite \
  --embedding-model deterministic-local-v1 \
  --hardware-class local-dev \
  --output bench_suite.json
```

## 7. Run Evaluation Metrics

```bash
sqlrite-eval \
  --dataset examples/eval_dataset.json \
  --output eval_report.json \
  --index-mode hnsw_baseline
```

## Performance Tuning Knobs

Useful environment variables:

| Variable | Purpose |
|---|---|
| `SQLRITE_VECTOR_STORAGE` | choose vector storage profile |
| `SQLRITE_ANN_MIN_CANDIDATES` | lower ANN candidate floor |
| `SQLRITE_ANN_MAX_HAMMING_RADIUS` | ANN search radius |
| `SQLRITE_ANN_MAX_CANDIDATE_MULTIPLIER` | cap candidate expansion |
| `SQLRITE_ENABLE_ANN_PERSISTENCE` | persist ANN state |
| `SQLRITE_SQLITE_MMAP_SIZE` | mmap tuning |
| `SQLRITE_SQLITE_CACHE_SIZE_KIB` | SQLite cache tuning |

Example:

```bash
SQLRITE_VECTOR_STORAGE=int8 \
SQLRITE_SQLITE_MMAP_SIZE=536870912 \
SQLRITE_SQLITE_CACHE_SIZE_KIB=131072 \
sqlrite doctor --db sqlrite_demo.db --index-mode hnsw_baseline --json
```

## Suggested Operational Loop

```mermaid
flowchart LR
  A["Run doctor"] --> B["Backup or snapshot"]
  B --> C["Change workload or index settings"]
  C --> D["Benchmark and evaluate"]
  D --> E["Compact or restore if needed"]
```

## Deeper References

- `benchmarks/README.md`
- `benchmarks/status.md`
- `project_docs/runtime_config_profiles.md`
- `project_docs/runbooks/release_candidate_hardening.md`
