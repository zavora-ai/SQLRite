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
- pluggable vector index modes: `brute_force`, `lsh_ann`, `disabled`
- ingestion worker with durable checkpoints and idempotent chunk IDs
- embedding provider abstraction:
  - deterministic local provider
  - OpenAI-compatible HTTP provider
  - custom HTTP provider
- reindex pipeline for embedding model/version migration
- tenant-aware secure wrapper with audit logging and key-rotation workflow
- operations tooling: backup, verify, health checks
- benchmark/eval CLIs with CI-gate integration

## 5-Minute Start

### 1) Seed a local DB

```bash
cargo run
```

This creates `sqlrite_demo.db` with 3 chunks (`demo-1`, `demo-2`, `demo-3`).

### 2) Run a query from CLI

```bash
cargo run --bin sqlrite-query -- --db sqlrite_demo.db --text "agents local memory" --top-k 3
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

## Query Cookbook (Real Use Cases)

All commands below assume `sqlrite_demo.db` from `cargo run`.

### Text-only retrieval

```bash
cargo run --bin sqlrite-query -- --db sqlrite_demo.db --text "keyword signals retrieval" --top-k 3
```

### Vector-only retrieval

```bash
cargo run --bin sqlrite-query -- --db sqlrite_demo.db --vector 0.95,0.05,0.0 --top-k 3
```

### Hybrid retrieval (text + vector)

```bash
cargo run --bin sqlrite-query -- \
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
cargo run --bin sqlrite-query -- \
  --db sqlrite_demo.db \
  --text "retrieval" \
  --filter tenant=demo \
  --filter topic=retrieval \
  --top-k 5
```

### Doc-scoped retrieval

```bash
cargo run --bin sqlrite-query -- \
  --db sqlrite_demo.db \
  --text "deterministic" \
  --doc-id doc-c \
  --top-k 5
```

### RRF fusion

```bash
cargo run --bin sqlrite-query -- \
  --db sqlrite_demo.db \
  --text "hybrid" \
  --vector 0.60,0.40,0.0 \
  --fusion rrf \
  --rrf-k 60 \
  --top-k 3
```

### Candidate-limit tuning

```bash
cargo run --bin sqlrite-query -- \
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
  --batch-size 64
```

Output shape:

```text
SQLRite ingestion complete
chunks(total=..., processed=..., failed=..., resumed_from=...)
provider=... model=...
source=...
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
cargo run --bin sqlrite-ops -- health --db sqlrite_demo.db
```

Sample output:

```text
health:
- integrity_ok=true
- chunk_count=3
- schema_version=2
- index_mode=brute_force
- index_entries=3
```

### Backup + verify

```bash
cargo run --bin sqlrite-ops -- backup --source sqlrite_demo.db --dest sqlrite_backup.db
cargo run --bin sqlrite-ops -- verify --path sqlrite_backup.db
```

## Server Mode (Health/Readiness/Metrics)

```bash
cargo run --bin sqlrite-serve -- --db sqlrite_demo.db --bind 127.0.0.1:8099
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
{"ready":true,"schema_version":2}
```

## Benchmarks and Performance

### Single benchmark run

```bash
cargo run --bin sqlrite-bench -- \
  --corpus 3000 \
  --queries 200 \
  --warmup 50 \
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
SQLRite benchmark: corpus=3000, queries=200, index=lsh_ann, fusion=weighted
ingest_ms=297.69, query_ms=830.41, qps=240.85, top1_hit_rate=1.0000
ingest_chunks_per_sec=10077.46, dataset_payload_bytes=923265, index_estimated_bytes=1245544, approx_working_set_bytes=2168809
latency_ms: avg=4.1436, p50=4.0659, p95=4.6379, p99=5.1522, min=3.7147, max=5.1905
```

### Matrix run

```bash
cargo run --bin sqlrite-bench-matrix -- --profile quick --durability balanced --output bench_matrix_quick_readme.json
```

Sample output:

```text
SQLRite benchmark matrix profile=quick
scenario                            qps    p50(ms)    p95(ms)       top1   query_ms   ingest_cps    work_mb
weighted + brute_force           164.94      5.931      6.691     1.0000     1212.5      30581.1       1.77
rrf(k=60) + brute_force          160.81      6.194      6.516     0.0950     1243.7      31399.1       1.77
weighted + lsh_ann               259.32      3.834      4.035     1.0000      771.2      10398.1       2.07
weighted + disabled_index        218.81      4.537      4.810     0.8050      914.0      33118.2       0.88
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

For historical trend context, see:

- `BENCHMARK_STATUS.md`
- `.github/workflows/ci.yml`
- `.github/workflows/perf-nightly.yml`

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

## Development Workflow

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --examples
```

## Repository Map

- `src/lib.rs` - core DB API and retrieval pipeline
- `src/ingest.rs` - ingestion worker + embedding providers
- `src/reindex.rs` - reindex orchestration
- `src/security.rs` - tenant policy/audit/encryption workflow
- `src/ops.rs` - health/backup/verify
- `src/server.rs` - health/readiness/metrics HTTP server
- `src/bin/` - operational CLIs
- `examples/` - runnable workflows

## Notes

- Benchmark numbers vary by CPU, memory pressure, and background load.
- CLI/query examples assume seeded `sqlrite_demo.db` unless specified otherwise.
- For larger corpora and trend analysis, use matrix runs plus `BENCHMARK_STATUS.md`.
