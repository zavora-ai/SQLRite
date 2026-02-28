# SQLRite

SQLRite is a Rust-first SQLite adaptation for AI agent retrieval workloads.

It is designed for developers who want:

- local-first deployment (`.db` file, no distributed infra)
- hybrid retrieval (vector + text)
- predictable behavior (deterministic ranking/tie-breaks)
- production-oriented tooling (ingestion, reindex, health, backups, security hooks, benchmarks)

## What You Get

- SQLite-backed chunk/document storage with migrations (`schema_migrations`)
- hybrid search with:
  - vector similarity
  - FTS5 lexical ranking (or lexical fallback)
  - weighted and RRF fusion
- pluggable vector index modes: `brute_force`, `lsh_ann`, `hnsw_baseline`, `disabled`
- vector storage profiles: `f32`, `f16`, `int8` (ANN snapshot quantization support)
- ingestion worker with durable checkpoints and idempotent chunk IDs
- embedding provider abstraction:
  - deterministic local provider
  - OpenAI-compatible HTTP provider
  - custom HTTP provider
- reindex pipeline for embedding model/version migration
- tenant-aware secure wrapper with audit logging and key-rotation workflow
- operations tooling: backup, verify, health checks, compaction
- benchmark/eval CLIs with CI-gate integration
- ANN snapshot persistence for faster ANN index warm-start on file-backed databases
- SQLite mmap/cache tuning controls for performance experiments and profile hardening

## 5-Minute Start

### 0) Inspect the unified CLI

```bash
cargo run -- --help
```

### 1) One-command quickstart (init -> query + timing gates)

```bash
cargo run -- quickstart \
  --db sqlrite_quickstart.db \
  --runs 5 \
  --max-median-ms 180000 \
  --min-success-rate 0.95 \
  --json \
  --output project_plan/reports/quickstart_local.json
```

Sample output:

```json
{
  "runs": 5,
  "successful_runs": 5,
  "success_rate": 1.0,
  "median_total_ms": 2.36,
  "gate_max_median_ms_passed": true,
  "gate_min_success_rate_passed": true
}
```

### 2) Seed a local DB (explicit init path)

```bash
cargo run
```

This creates `sqlrite_demo.db` with 3 chunks (`demo-1`, `demo-2`, `demo-3`).

### 3) Run a query from CLI

```bash
cargo run -- query --db sqlrite_demo.db --text "agents local memory" --top-k 3
```

Sample output:

```text
results=3
1. demo-1 | doc=doc-a | hybrid=1.000 | vector=0.000 | text=1.000
   Rust and SQLite are ideal for local-first AI agents.
2. demo-2 | doc=doc-b | hybrid=0.000 | vector=0.000 | text=0.000
   Hybrid retrieval mixes vector search with keyword signals.
3. demo-3 | doc=doc-c | hybrid=0.000 | vector=0.000 | text=0.000
   Batching and metadata filters keep RAG pipelines deterministic.
```

## Global CLI Install (No Cargo Run)

Install globally (default path: `~/.local/bin/sqlrite`):

```bash
bash scripts/sqlrite-global-install.sh
```

Then run directly:

```bash
sqlrite --help
sqlrite init --db sqlrite_demo.db --seed-demo
sqlrite quickstart --db sqlrite_quickstart.db --runs 5 --max-median-ms 180000 --min-success-rate 0.95
sqlrite query --db sqlrite_demo.db --text "local" --top-k 3
```

If `sqlrite` is not found, add this to your shell config:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

## Global Update With Tests

Update the global install while validating progress:

```bash
bash scripts/sqlrite-global-update.sh
```

Default update flow runs:

1. `cargo fmt --all --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test`
4. global reinstall + smoke tests

Quick update (skip full gates, still reinstall + smoke test):

```bash
bash scripts/sqlrite-global-update.sh --quick
```

Install from GitHub Release assets (curl-friendly):

```bash
bash scripts/sqlrite-install.sh --version 0.5.0
```

## Quickstart Gate (Sprint 3)

`sqlrite quickstart` runs `init -> query` and reports timing/success telemetry.

Gate command (fails with non-zero exit on threshold miss):

```bash
cargo run -- quickstart \
  --db sprint3_quickstart.db \
  --runs 5 \
  --max-median-ms 180000 \
  --min-success-rate 0.95 \
  --json \
  --output project_plan/reports/quickstart_local.json
```

Human-readable mode:

```bash
cargo run -- quickstart --db sprint3_quickstart.db --runs 3
```

Key flags:

- `--runs N` repeated init/query runs for stability checks
- `--max-median-ms F` median total time gate (Phase A target: `< 180000`)
- `--min-success-rate F` required run success ratio (Phase A target: `>= 0.95`)
- `--json` machine-readable output for CI/reporting
- `--output PATH` write report payload to disk

## Interactive SQL Shell

Start shell mode (no `--execute` needed):

```bash
cargo run -- sql --db sqlrite_demo.db
```

Shell helpers:

- `.help`
- `.tables`
- `.schema [table]`
- `.example`
- `.example lexical --run`
- `.example hybrid --run`
- `.example vector_ddl --run`
- `.example index_catalog --run`
- `.exit`

One-shot SQL still works:

```bash
cargo run -- sql --db sqlrite_demo.db --execute "SELECT id, doc_id FROM chunks LIMIT 3;"
```

## SQL-Native Vector Operators

`sqlrite sql` now supports pgvector-style distance operators and vector literal helpers:

- `<->` L2 distance
- `<=>` cosine distance
- `<#>` negative inner product
- `vector('0.1,0.2,0.3')` or `vector('[0.1,0.2,0.3]')`

Example:

```bash
cargo run -- sql --db sqlrite_demo.db --execute "
SELECT id,
       embedding <-> vector('0.95,0.05,0.0') AS l2,
       embedding <=> vector('0.95,0.05,0.0') AS cosine_distance,
       embedding <#> vector('0.95,0.05,0.0') AS neg_inner
FROM chunks
ORDER BY l2 ASC, id ASC
LIMIT 3;"
```

Sample output:

```text
[
  {"id":"demo-1","l2":0.0424,"cosine_distance":0.0006,"neg_inner":-0.8780},
  {"id":"demo-2","l2":0.4243,"cosine_distance":0.0958,"neg_inner":-0.6350},
  {"id":"demo-3","l2":0.9192,"cosine_distance":0.5583,"neg_inner":-0.3200}
]
```

Helper SQL functions:

- `l2_distance(lhs, rhs)`
- `cosine_distance(lhs, rhs)`
- `neg_inner_product(lhs, rhs)`
- `vec_dims(vector_expr)`
- `vec_to_json(vector_expr)`

## SQL Retrieval Functions and Index DDL (Sprint 5)

Additional retrieval SQL functions:

- `embed(text)` deterministic text embedding (16 dimensions)
- `bm25_score(query, document)` lexical relevance score
- `hybrid_score(vector_score, text_score, alpha)` weighted fusion (`alpha` between `0.0` and `1.0`)

Example:

```bash
cargo run -- sql --db sqlrite_demo.db --execute "
SELECT vec_dims(embed('agent local memory')) AS dims,
       bm25_score('agent memory', 'agent systems keep local memory') AS bm25,
       hybrid_score(0.8, 0.2, 0.75) AS hybrid;"
```

Sample output:

```text
[
  {
    "bm25": 4.4489545822143555,
    "dims": 16,
    "hybrid": 0.6500000000000001
  }
]
```

Retrieval index DDL support:

- `CREATE VECTOR INDEX ... USING HNSW [WITH (...)]`
- `CREATE TEXT INDEX ... USING FTS5 [WITH (...)]`
- `DROP VECTOR INDEX [IF EXISTS] ...`
- `DROP TEXT INDEX [IF EXISTS] ...`
- Metadata catalog view: `retrieval_index_catalog`

Example:

```bash
cargo run -- sql --db sqlrite_demo.db --execute \
  "CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw ON chunks(embedding) USING HNSW WITH (m=16, ef_construction=64);"

cargo run -- sql --db sqlrite_demo.db --execute \
  "CREATE TEXT INDEX IF NOT EXISTS idx_chunks_content_fts ON chunks(content) USING FTS5 WITH (tokenizer=unicode61);"

cargo run -- sql --db sqlrite_demo.db --execute \
  "SELECT name, index_kind, table_name, column_name, using_engine, options_json, status FROM retrieval_index_catalog ORDER BY name;"
```

Sample output:

```text
created vector retrieval index `idx_chunks_embedding_hnsw` on chunks(embedding) using HNSW
created text retrieval index `idx_chunks_content_fts` on chunks(content) using FTS5
[
  {
    "column_name": "content",
    "index_kind": "text",
    "name": "idx_chunks_content_fts",
    "options_json": "{\"tokenizer\":\"unicode61\"}",
    "status": "active",
    "table_name": "chunks",
    "using_engine": "fts5"
  },
  {
    "column_name": "embedding",
    "index_kind": "vector",
    "name": "idx_chunks_embedding_hnsw",
    "options_json": "{\"ef_construction\":64,\"m\":16}",
    "status": "active",
    "table_name": "chunks",
    "using_engine": "hnsw"
  }
]
```

Planner fallback behavior (Sprint 6):

- If ANN/index candidates are unavailable or unhealthy, SQLRite falls back to brute-force vector scoring from stored embeddings.
- Deterministic tie-break order is always by `chunk_id` when scores are equal.

Fallback smoke command:

```bash
cargo run -- query \
  --db s06_fallback.db \
  --profile balanced \
  --index-mode disabled \
  --vector 1,0 \
  --top-k 1 \
  --candidate-limit 1
```

Sample output:

```text
results=1
1. best | doc=d1 | hybrid=1.000 | vector=1.000 | text=0.000
   best match
```

## EXPLAIN RETRIEVAL (Sprint 7)

SQLRite supports retrieval-aware explain output with score attribution and path breakdown:

```bash
cargo run -- sql --db sqlrite_demo.db --execute "
EXPLAIN RETRIEVAL
SELECT id,
       1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score,
       bm25_score('local agent memory', content) AS text_score,
       hybrid_score(
           1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')),
           bm25_score('local agent memory', content),
           0.65
       ) AS hybrid
FROM chunks
ORDER BY hybrid DESC, id ASC
LIMIT 5;"
```

Sample fields in output:

- `execution_path.vector`: `ann_index` or `brute_force_fallback`
- `execution_path.text`: text execution mode
- `score_attribution`: vector/text/fusion and `hybrid_alpha`
- `determinism`: order-by tie-break diagnostics
- `sqlite_query_plan`: raw `EXPLAIN QUERY PLAN` rows

## SQL Cookbook and Conformance (Sprint 7)

SQL-only cookbook:

- `docs/sql_cookbook.md`

Migration guides:

- `docs/migrations/sqlite_to_sqlrite.md`
- `docs/migrations/pgvector_to_sqlrite.md`

Run SQL-only conformance for cookbook patterns:

```bash
bash scripts/run-sql-cookbook-conformance.sh
```

Artifacts:

- `project_plan/reports/s07_sql_conformance.log`
- `project_plan/reports/s07_sql_conformance.json`

## Query Cookbook (Real Use Cases)

All commands below assume `sqlrite_demo.db` from `cargo run`.

### Text-only retrieval

```bash
cargo run -- query --db sqlrite_demo.db --text "keyword signals retrieval" --top-k 3
```

### Vector-only retrieval

```bash
cargo run -- query --db sqlrite_demo.db --vector 0.95,0.05,0.0 --top-k 3
```

### Hybrid retrieval (text + vector)

```bash
cargo run -- query \
  --db sqlrite_demo.db \
  --text "local" \
  --vector 0.95,0.05,0.0 \
  --alpha 0.65 \
  --top-k 3
```

Sample output:

```text
results=3
1. demo-1 | doc=doc-a | hybrid=0.806 | vector=0.701 | text=1.000
2. demo-2 | doc=doc-b | hybrid=0.516 | vector=0.794 | text=0.000
3. demo-3 | doc=doc-c | hybrid=0.299 | vector=0.459 | text=0.000
```

### Metadata filter (tenant + topic)

```bash
cargo run -- query \
  --db sqlrite_demo.db \
  --text "retrieval" \
  --filter tenant=demo \
  --filter topic=retrieval \
  --top-k 5
```

### Doc-scoped retrieval

```bash
cargo run -- query \
  --db sqlrite_demo.db \
  --text "deterministic" \
  --doc-id doc-c \
  --top-k 5
```

### RRF fusion

```bash
cargo run -- query \
  --db sqlrite_demo.db \
  --text "hybrid" \
  --vector 0.60,0.40,0.0 \
  --fusion rrf \
  --rrf-k 60 \
  --top-k 3
```

### Candidate-limit tuning

```bash
cargo run -- query \
  --db sqlrite_demo.db \
  --text "agents" \
  --vector 0.90,0.10,0.0 \
  --candidate-limit 25 \
  --top-k 3
```

## Runnable Examples

Run these directly with `cargo run --example <name>`.

```bash
cargo run --example basic_search
cargo run --example ingestion_worker
cargo run --example secure_tenant
cargo run --example tool_adapter
cargo run --example query_use_cases
```

### `basic_search`

```text
c3 | doc=doc-sqlite | score=0.997
c2 | doc=doc-rag | score=0.576
```

### `ingestion_worker`

```text
ingested chunks: total=2, processed=2
search results: 2
```

### `secure_tenant`

```text
secure results: 1
top chunk: chunk-sec-1
```

### `tool_adapter`

```text
named tool response: { ... }
tools exposed: 4
```

### `query_use_cases`

Demonstrates 7 retrieval patterns end-to-end (text, vector, hybrid, filters, doc-scope, RRF, candidate-limit) with detailed score breakdown.

## Ingestion Workflow

Ingest text from file/URL/direct payload with checkpointing.

```bash
cargo run --bin sqlrite-ingest -- \
  --db sqlrite_demo.db \
  --job-id ingest-001 \
  --doc-id doc-001 \
  --source-id docs/readme \
  --tenant acme \
  --file README.md \
  --checkpoint .sqlrite/checkpoints/ingest-001.json \
  --chunking heading \
  --max-chars 1200 \
  --overlap-chars 120 \
  --batch-size 64 \
  --adaptive-batching \
  --max-batch-size 1024 \
  --target-batch-ms 80 \
  --json \
  --output ingest_report.json
```

Output shape:

```json
{
  "total_chunks": 21286,
  "processed_chunks": 21286,
  "duration_ms": 1435.283041,
  "throughput_chunks_per_minute": 889831.4572923321,
  "average_batch_size": 788.3703703703703,
  "peak_batch_size": 1024,
  "batch_count": 27,
  "adaptive_batching": true
}
```

## Security Workflow

### Add tenant key

```bash
cargo run --bin sqlrite-security -- add-key \
  --registry .sqlrite/tenant_keys.json \
  --tenant acme \
  --key-id k1 \
  --key-material super-secret-k1 \
  --active
```

### Rotate encrypted metadata to a key

```bash
cargo run --bin sqlrite-security -- rotate-key \
  --db sqlrite_demo.db \
  --registry .sqlrite/tenant_keys.json \
  --tenant acme \
  --field secret_payload \
  --new-key-id k1
```

## Reindex Workflow

Use this when you change embedding model/provider and need to re-embed stored chunks.

### Deterministic provider (local)

```bash
cargo run --bin sqlrite-reindex -- \
  --db sqlrite_demo.db \
  --provider deterministic \
  --target-model-version det-v2 \
  --batch-size 256 \
  --checkpoint .sqlrite/checkpoints/reindex.json
```

Sample output:

```text
reindex complete
scanned=3, updated=3, skipped=0, failed=0, resumed_from=0
provider=deterministic_local model=det-v2
```

### OpenAI-compatible provider

```bash
cargo run --bin sqlrite-reindex -- \
  --db sqlrite_demo.db \
  --provider openai \
  --endpoint https://api.openai.com/v1/embeddings \
  --model text-embedding-3-small \
  --api-key-env OPENAI_API_KEY \
  --target-model-version text-embedding-3-small
```

### Custom HTTP provider

```bash
cargo run --bin sqlrite-reindex -- \
  --db sqlrite_demo.db \
  --provider custom \
  --endpoint http://localhost:8080/embed \
  --input-field inputs \
  --embeddings-field embeddings \
  --target-model-version internal-v1
```

## Ops Workflow

### Health

```bash
cargo run -- doctor --db sqlrite_demo.db
```

Sample output:

```text
sqlrite doctor
- version=0.1.0
- supported_profiles=balanced,durable,fast_unsafe
- supported_index_modes=brute_force,lsh_ann,hnsw_baseline,disabled
- supported_vector_storage=f32,f16,int8
- in_memory_integrity_ok=true
- db_path=sqlrite_demo.db
- integrity_ok=true
- chunk_count=3
- schema_version=3
- index_mode=brute_force
- vector_storage=f32
- index_estimated_memory_bytes=174
- sqlite_mmap_size_bytes=268435456
- sqlite_cache_size_kib=65536
```

### Backup + verify

```bash
cargo run -- backup --source sqlrite_demo.db --dest sqlrite_backup.db
cargo run -- backup verify --path sqlrite_backup.db
```

### Compaction maintenance

```bash
cargo run -- compact --db sqlrite_demo.db --index-mode hnsw_baseline --json
```

Sample output:

```json
{
  "before_chunks": 21286,
  "after_chunks": 21286,
  "deduplicated_chunks": 0,
  "wal_checkpoint_applied": true,
  "analyze_applied": true,
  "vacuum_applied": true,
  "reclaimed_bytes": 704512
}
```

## Server Mode (Health/Readiness/Metrics)

```bash
cargo run -- serve --db sqlrite_demo.db --bind 127.0.0.1:8099
```

Endpoints:

- `GET /healthz` -> JSON health report
- `GET /readyz` -> readiness status
- `GET /metrics` -> Prometheus-style metrics

Example:

```bash
curl -fsS http://127.0.0.1:8099/readyz
```

Response:

```json
{"ready":true,"schema_version":3}
```

## Benchmarks and Performance

### Single benchmark run

```bash
cargo run -- benchmark \
  --corpus 3000 \
  --queries 200 \
  --warmup 50 \
  --concurrency 2 \
  --embedding-dim 64 \
  --top-k 10 \
  --candidate-limit 300 \
  --fusion weighted \
  --index-mode lsh_ann \
  --durability balanced \
  --output bench_report_readme.json
```

Sample output:

```text
SQLRite benchmark: corpus=3000, queries=200, concurrency=2, index=lsh_ann, fusion=weighted
runtime: storage=f32, mmap_size_bytes=268435456, cache_size_kib=65536
ingest_ms=297.69, query_ms=830.41, qps=240.85, top1_hit_rate=1.0000
ingest_chunks_per_sec=10077.46, dataset_payload_bytes=923265, index_estimated_bytes=1245544, approx_working_set_bytes=2168809
latency_ms: avg=4.1436, p50=4.0659, p95=4.6379, p99=5.1522
```

Sample JSON fields (from `--output` report):

```json
{
  "vector_index_mode": "hnsw_baseline",
  "vector_storage_kind": "f32",
  "sqlite_mmap_size_bytes": 268435456,
  "sqlite_cache_size_kib": 65536
}
```

### Matrix run

```bash
cargo run --bin sqlrite-bench-matrix -- \
  --profile quick \
  --concurrency 2 \
  --durability balanced \
  --output bench_matrix_quick_readme.json
```

Sample output:

```text
SQLRite benchmark matrix profile=quick
scenario                      conc        qps    p50(ms)    p95(ms)       top1   query_ms   ingest_cps    work_mb
weighted + brute_force           2      84.57      7.284      9.425     1.0000     1182.4      24780.3       1.77
rrf(k=60) + brute_force          2      81.93      7.642     10.181     0.0950     1220.3      30122.7       1.77
weighted + lsh_ann               2     188.12      4.203      6.012     1.0000      531.6       9981.4       2.07
weighted + hnsw_baseline         2     173.44      4.689      7.435     1.0000      576.6       7520.6       2.18
weighted + disabled_index        2      55.92     17.114     20.889     1.0000     3576.5      28610.1       0.88
```

### Reproducible benchmark/eval suite (S12)

Run one command to capture:

- benchmark matrix runs for each profile (`quick|10k|100k|1m|10m`)
- throughput sweep by concurrency level
- eval metrics (`recall`, `mrr`, `ndcg`) for selected index modes
- metadata (`embedding_model`, `dataset_id`, `hardware_class`, OS/arch/CPU threads)

```bash
cargo run --bin sqlrite-bench-suite -- \
  --profiles quick,10k \
  --concurrency-profile 10k \
  --concurrency-levels 1,2,4 \
  --dataset examples/eval_dataset.json \
  --dataset-id examples/eval_dataset.json \
  --embedding-model deterministic-local-v1 \
  --hardware-class local-dev \
  --durability balanced \
  --output project_plan/reports/s12_bench_suite.json
```

Sample output:

```text
SQLRite benchmark suite: version=s12-v1, host=macos aarch64, cpu_threads=10
metadata: dataset_id=examples/eval_dataset.json, embedding_model=deterministic-local-v1, hardware_class=local-dev
profile=10k
  weighted + brute_force       qps=   76.50 p95_ms=  16.511 top1=1.0000 conc=1
concurrency_sweep profile=10k scenario=weighted + brute_force
  concurrency=1 qps=87.36 p95_ms=12.849
  concurrency=2 qps=35.14 p95_ms=15.005
  concurrency=4 qps=37.26 p95_ms=33.079
eval mode=brute_force k=1 recall=0.8333 mrr=1.0000 ndcg=1.0000
```

### Assert thresholds (perf gate)

```bash
cargo run --bin sqlrite-bench-assert -- \
  --matrix bench_matrix_quick_readme.json \
  --scenario "weighted + brute_force" \
  --scenario "weighted + lsh_ann" \
  --min-qps 100 \
  --max-p95-ms 10 \
  --min-top1 0.99 \
  --min-ingest-cps 8000
```

Sample output:

```text
benchmark assertions passed: profile=quick, checked=2 scenario(s)
```

### Phase C benchmark bundle (S13)

Generate a reproducible bundle (suite JSON/log + manifest + gate log + tarball):

```bash
bash scripts/run-benchmark-bundle.sh \
  --output-dir project_plan/reports/s13_bundle \
  --profiles 100k,1m \
  --concurrency-profile 100k \
  --concurrency-levels 1,2,4 \
  --strict-phase-c-gate
```

Gate assertions against suite output (direct CLI):

```bash
cargo run --bin sqlrite-bench-suite-assert -- \
  --suite project_plan/reports/s13_bundle/bench_suite.json \
  --rule "profile=100k,scenario=weighted + lsh_ann,max_p95_ms=40,min_top1=0.99" \
  --rule "profile=1m,scenario=weighted + lsh_ann,max_p95_ms=90,min_top1=0.99" \
  --rule "profile=100k,scenario=weighted + brute_force,min_ingest_cpm=50000"
```

For historical trend context, see:

- `BENCHMARK_STATUS.md`
- `.github/workflows/ci.yml`
- `.github/workflows/perf-nightly.yml`

## ANN, Storage, and SQLite Tuning Knobs (Sprint 8-10)

`sqlrite` now supports ANN/runtime tuning through environment variables (applies to `init`, `query`, `benchmark`, `quickstart`, `doctor`, `serve`, and SQL bootstrap).

| Variable | Description | Example |
| --- | --- | --- |
| `SQLRITE_VECTOR_STORAGE` | Vector storage kind (`f32`, `f16`, `int8`) | `SQLRITE_VECTOR_STORAGE=int8` |
| `SQLRITE_ANN_MIN_CANDIDATES` | ANN minimum candidate set | `SQLRITE_ANN_MIN_CANDIDATES=256` |
| `SQLRITE_ANN_MAX_HAMMING_RADIUS` | ANN bucket expansion radius | `SQLRITE_ANN_MAX_HAMMING_RADIUS=2` |
| `SQLRITE_ANN_MAX_CANDIDATE_MULTIPLIER` | ANN cap multiplier | `SQLRITE_ANN_MAX_CANDIDATE_MULTIPLIER=8` |
| `SQLRITE_ENABLE_ANN_PERSISTENCE` | Enable/disable ANN snapshot persistence (`true/false`) | `SQLRITE_ENABLE_ANN_PERSISTENCE=true` |
| `SQLRITE_SQLITE_MMAP_SIZE` | SQLite mmap size (bytes) | `SQLRITE_SQLITE_MMAP_SIZE=536870912` |
| `SQLRITE_SQLITE_CACHE_SIZE_KIB` | SQLite page cache target (KiB) | `SQLRITE_SQLITE_CACHE_SIZE_KIB=131072` |

### Example: run HNSW baseline with int8 storage

```bash
SQLRITE_VECTOR_STORAGE=int8 \
cargo run -- query \
  --db sqlrite_demo.db \
  --index-mode hnsw_baseline \
  --text "local memory" \
  --vector 0.95,0.05,0.0 \
  --top-k 3
```

### Example: benchmark tuned mmap/cache profile

```bash
SQLRITE_SQLITE_MMAP_SIZE=536870912 \
SQLRITE_SQLITE_CACHE_SIZE_KIB=131072 \
cargo run -- benchmark \
  --corpus 8000 \
  --queries 350 \
  --warmup 80 \
  --embedding-dim 64 \
  --top-k 10 \
  --candidate-limit 400 \
  --fusion weighted \
  --index-mode hnsw_baseline \
  --durability balanced \
  --output project_plan/reports/s10_benchmark_tuned.json
```

### Example: verify active storage/tuning in doctor output

```bash
SQLRITE_VECTOR_STORAGE=int8 \
SQLRITE_SQLITE_MMAP_SIZE=536870912 \
SQLRITE_SQLITE_CACHE_SIZE_KIB=131072 \
cargo run -- doctor --db sqlrite_demo.db --index-mode hnsw_baseline --json
```

## Evaluation (Quality Metrics)

```bash
cargo run --bin sqlrite-eval -- \
  --dataset examples/eval_dataset.json \
  --output eval_report_readme.json \
  --index-mode brute_force \
  --durability balanced
```

Sample output:

```text
SQLRite eval summary: corpus=5, queries=3, ks=[1, 3, 5]
k=1: recall=0.8333, precision=1.0000, mrr=1.0000, ndcg=1.0000, hit_rate=1.0000
k=3: recall=1.0000, precision=0.4444, mrr=1.0000, ndcg=0.9732, hit_rate=1.0000
k=5: recall=1.0000, precision=0.2667, mrr=1.0000, ndcg=0.9732, hit_rate=1.0000
```

## Library API (Minimal)

```rust
use serde_json::json;
use sqlrite::{ChunkInput, Result, SearchRequest, SqlRite};

fn demo() -> Result<()> {
    let db = SqlRite::open_in_memory()?;

    db.ingest_chunk(
        &ChunkInput::new("c1", "d1", "Chunk text", vec![0.1, 0.2, 0.3])
            .with_metadata(json!({"tenant": "acme"}))
    )?;

    let request = SearchRequest::hybrid("chunk", vec![0.12, 0.18, 0.31], 5);
    let _results = db.search(request)?;
    Ok(())
}
```

## Packaging and Distribution

Build release archives:

```bash
bash scripts/create-release-archive.sh --version 0.5.0
```

Build Linux packages (`.deb`, `.rpm`) when `nfpm` is installed:

```bash
bash scripts/package-linux.sh --version 0.5.0
```

Build Docker image:

```bash
docker build -t sqlrite:local .
docker run --rm sqlrite:local --help
```

Generate Homebrew formula and winget manifests:

```bash
bash scripts/generate-homebrew-formula.sh --help
bash scripts/generate-winget-manifests.sh --help
```

Detailed channel documentation:

- `docs/packaging_channels.md`
- `.github/workflows/installer-smoke.yml`
- `.github/workflows/packaging-channels.yml`

## Development Workflow

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --examples
```

## Repository Map

- `CHANGELOG.md` - release and sprint-level change history
- `src/lib.rs` - core DB API and retrieval pipeline
- `src/ingest.rs` - ingestion worker + embedding providers
- `src/reindex.rs` - reindex orchestration
- `src/security.rs` - tenant policy/audit/encryption workflow
- `src/ops.rs` - health/backup/verify
- `src/server.rs` - health/readiness/metrics HTTP server
- `src/bin/` - operational CLIs
- `scripts/` - install, update, packaging, and release tooling
- `packaging/` - Homebrew/winget/nfpm packaging assets
- `examples/` - runnable workflows

## Notes

- Benchmark numbers vary by CPU, memory pressure, and background load.
- CLI/query examples assume seeded `sqlrite_demo.db` unless specified otherwise.
- For larger corpora and trend analysis, use matrix runs plus `BENCHMARK_STATUS.md`.
