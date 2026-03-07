# SQLRite

SQLRite is a Rust-first, SQLite-based retrieval engine for AI-agent and RAG workloads.

It is built for developers who want SQL-native retrieval, local-first deployment, predictable ranking, and production-ready operational tooling without standing up a separate vector database.

## Why SQLRite

- Single-file local database by default.
- Hybrid retrieval in one place: vector, text, and fused ranking.
- SQL-first interface with retrieval-aware operators, functions, and `SEARCH(...)` syntax.
- Multiple runtime modes: embedded CLI, HTTP server, gRPC, and MCP tool server.
- Migration paths from SQLite, libSQL, pgvector, Qdrant, Weaviate, and Milvus export patterns.
- Security and operations tooling for real applications: RBAC, audit export, key rotation, backup, restore, compaction, and health checks.

## Core Capabilities

- SQLite-backed chunk and document storage with schema migrations.
- Vector index modes: `brute_force`, `lsh_ann`, `hnsw_baseline`, `disabled`.
- Vector storage profiles: `f32`, `f16`, `int8`.
- Retrieval features:
  - vector similarity
  - FTS5 lexical ranking
  - weighted fusion
  - reciprocal-rank fusion (RRF)
  - deterministic tie-breaking
- SQL retrieval surface:
  - distance operators: `<->`, `<=>`, `<#>`
  - retrieval helpers: `vector(...)`, `embed(...)`, `bm25_score(...)`, `hybrid_score(...)`
  - retrieval planning insight with `EXPLAIN RETRIEVAL`
  - concise hybrid retrieval with `SEARCH(...)`
- Ingestion and maintenance:
  - direct chunk ingest from CLI
  - durable ingestion worker with checkpoints
  - reindex pipeline for embedding model changes
  - compaction and backup workflows
- Server and integrations:
  - HTTP query and SQL endpoints
  - native gRPC query service
  - MCP tool server mode
  - Rust, Python, and TypeScript SDKs

## Supported Platforms

SQLRite targets:

- Linux `x86_64` and `arm64`
- macOS `x86_64` and `arm64`
- Windows `x86_64` and `arm64`

## Install

Commands below assume you want `sqlrite` on your `PATH`.

### Option 1: Install from source with Cargo

This works on macOS, Linux, and Windows anywhere Rust is available.

From scratch:

```bash
git clone https://github.com/zavora-ai/SQLRite.git
cd SQLRite
cargo install --path .
```

If you already have the repository locally:

```bash
cargo install --path .
```

### Option 2: Build and install from this repo

macOS and Linux:

```bash
bash scripts/sqlrite-global-install.sh
```

If `sqlrite` is not found after install, add the default user bin directory to your shell profile:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Option 3: Install from a GitHub release

Unix-friendly installer:

```bash
bash scripts/sqlrite-install.sh --version 1.0.0
```

Release artifacts and checksums are published on GitHub Releases.

## 5-Minute Start

If you are running directly from a source checkout instead of installing the binaries first, replace:

- `sqlrite` with `cargo run --`
- `sqlrite-security` with `cargo run --bin sqlrite-security --`
- `sqlrite-reindex` with `cargo run --bin sqlrite-reindex --`
- `sqlrite-grpc-client` with `cargo run --bin sqlrite-grpc-client --`

### 1. Inspect the CLI

```bash
sqlrite --help
```

### 2. Create a local database with demo data

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
```

### 3. Run your first query

```bash
sqlrite query --db sqlrite_demo.db --text "agents local memory" --top-k 3
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

### 4. Run an end-to-end quickstart

```bash
sqlrite quickstart \
  --db sqlrite_quickstart.db \
  --runs 5 \
  --json \
  --output quickstart.json
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

## Common Query Patterns

### Text-only retrieval

```bash
sqlrite query --db sqlrite_demo.db --text "keyword signals retrieval" --top-k 3
```

### Vector-only retrieval

```bash
sqlrite query --db sqlrite_demo.db --vector 0.95,0.05,0.0 --top-k 3
```

### Hybrid retrieval

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --vector 0.95,0.05,0.0 \
  --alpha 0.65 \
  --top-k 3
```

### Metadata-filtered retrieval

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "agent memory" \
  --filter tenant=demo \
  --filter topic=memory \
  --top-k 5
```

### Document-scoped retrieval

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --doc-id doc-a \
  --top-k 3
```

### Query profile hints

Use `balanced`, `latency`, or `recall` to trade candidate set size for speed or coverage.

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "agent memory" \
  --query-profile latency \
  --top-k 5
```

### RRF fusion

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "agent memory" \
  --vector 0.95,0.05,0.0 \
  --fusion rrf \
  --rrf-k 60 \
  --top-k 5
```

## Interactive SQL Shell

Start the SQL shell:

```bash
sqlrite sql --db sqlrite_demo.db
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

Run a one-shot SQL statement:

```bash
sqlrite sql --db sqlrite_demo.db --execute "SELECT id, doc_id FROM chunks LIMIT 3;"
```

## Retrieval SQL

### Vector operators

SQLRite supports pgvector-style distance operators:

- `<->` L2 distance
- `<=>` cosine distance
- `<#>` negative inner product

Example:

```bash
sqlrite sql --db sqlrite_demo.db --execute "
SELECT id,
       embedding <-> vector('0.95,0.05,0.0') AS l2,
       embedding <=> vector('0.95,0.05,0.0') AS cosine_distance,
       embedding <#> vector('0.95,0.05,0.0') AS neg_inner
FROM chunks
ORDER BY l2 ASC, id ASC
LIMIT 3;"
```

### Retrieval functions

Available helpers include:

- `vector('0.1,0.2,0.3')`
- `embed(text)`
- `bm25_score(query, document)`
- `hybrid_score(vector_score, text_score, alpha)`
- `vec_dims(vector_expr)`
- `vec_to_json(vector_expr)`

Example:

```bash
sqlrite sql --db sqlrite_demo.db --execute "
SELECT vec_dims(embed('agent local memory')) AS dims,
       bm25_score('agent memory', 'agent systems keep local memory') AS bm25,
       hybrid_score(0.8, 0.2, 0.75) AS hybrid;"
```

### Retrieval index DDL

```bash
sqlrite sql --db sqlrite_demo.db --execute "
CREATE VECTOR INDEX idx_chunks_embedding ON chunks(embedding) USING HNSW;
CREATE TEXT INDEX idx_chunks_content ON chunks(content) USING FTS5;"
```

### SEARCH(...)

Use `SEARCH(...)` when you want a concise SQL-native hybrid retrieval form.

```bash
sqlrite sql --db sqlrite_demo.db --execute "
SELECT chunk_id, doc_id, hybrid_score
FROM SEARCH(
       'agent memory',
       vector('0.95,0.05,0.0'),
       5,
       0.65,
       500,
       'balanced',
       '{\"tenant\":\"demo\"}',
       NULL
     )
ORDER BY hybrid_score DESC, chunk_id ASC;"
```

Sample output:

```json
[
  {
    "chunk_id": "chunk-1",
    "doc_id": "doc-1",
    "hybrid_score": 0.8124017715454102
  }
]
```

### EXPLAIN RETRIEVAL

```bash
sqlrite sql --db sqlrite_demo.db --execute "
EXPLAIN RETRIEVAL
SELECT id,
       hybrid_score(
         1.0 - (embedding <=> vector('0.95,0.05,0.0')),
         bm25_score('local memory', content),
         0.65
       ) AS score
FROM chunks
ORDER BY score DESC, id ASC
LIMIT 3;"
```

What it shows:

- vector execution path (`ann_index` or `brute_force_fallback`)
- text execution mode
- score attribution
- deterministic ordering hints
- raw SQLite query-plan rows

## Ingestion

### Ingest a single chunk directly

```bash
sqlrite ingest \
  --db sqlrite_demo.db \
  --id chunk-100 \
  --doc-id doc-100 \
  --content "SQLRite keeps retrieval local and easy to reason about." \
  --vector 0.9,0.1,0.0
```

### Use the ingestion worker

```bash
sqlrite-ingest \
  --db sqlrite_demo.db \
  --job-id docs-import \
  --doc-id guide-1 \
  --file ./docs/guide.md \
  --checkpoint ingest.checkpoint.json \
  --batch-size 64 \
  --json
```

Use the ingestion worker when you need:

- resumable jobs
- adaptive batching
- deterministic chunk IDs
- embedding model/version tracking

## Reindexing

Reindex when you change embedding model, model version, or embedding provider.

### Deterministic local provider

```bash
sqlrite-reindex \
  --db sqlrite_demo.db \
  --provider deterministic \
  --target-model-version local-v2 \
  --batch-size 64
```

### OpenAI-compatible provider

```bash
sqlrite-reindex \
  --db sqlrite_demo.db \
  --provider openai \
  --endpoint https://api.openai.com/v1/embeddings \
  --model text-embedding-3-small \
  --api-key-env OPENAI_API_KEY \
  --target-model-version openai-v1 \
  --batch-size 32
```

### Custom HTTP provider

```bash
sqlrite-reindex \
  --db sqlrite_demo.db \
  --provider custom \
  --endpoint http://localhost:8080/embed \
  --input-field inputs \
  --embeddings-field embeddings \
  --target-model-version internal-v1
```

## Migration

SQLRite includes a first-class migration command for SQL databases and API-first vector database export shapes.

### SQLite

```bash
sqlrite migrate sqlite \
  --source legacy.db \
  --target sqlrite.db \
  --doc-table legacy_documents \
  --doc-id-col doc_id \
  --chunk-table legacy_chunks \
  --chunk-id-col chunk_id \
  --chunk-doc-id-col doc_id \
  --chunk-content-col chunk_text \
  --chunk-embedding-col embedding_blob \
  --chunk-embedding-dim-col embedding_dim \
  --embedding-format blob_f32le \
  --batch-size 512 \
  --create-indexes
```

### libSQL

```bash
sqlrite migrate libsql \
  --source libsql-replica.db \
  --target sqlrite.db \
  --create-indexes
```

### pgvector-style JSONL

```bash
sqlrite migrate pgvector \
  --input export.jsonl \
  --target sqlrite.db \
  --batch-size 512 \
  --create-indexes \
  --json
```

### Qdrant, Weaviate, or Milvus export patterns

```bash
sqlrite migrate qdrant --input qdrant_export.jsonl --target sqlrite.db --create-indexes
sqlrite migrate weaviate --input weaviate_export.jsonl --target sqlrite.db --create-indexes
sqlrite migrate milvus --input milvus_export.jsonl --target sqlrite.db --create-indexes
```

### Validate a migrated database

```bash
sqlrite doctor --db sqlrite.db --json
sqlrite query --db sqlrite.db --text "agent memory" --top-k 5
```

Detailed migration documentation:

- `docs/migrations/sqlite_to_sqlrite.md`
- `docs/migrations/pgvector_to_sqlrite.md`
- `docs/migrations/api_first_vector_db_patterns.md`
- `docs/runbooks/migration_cli_workflow.md`

## Security and Multi-Tenant Operation

### Generate a default RBAC policy

```bash
sqlrite-security init-policy --path .sqlrite/rbac-policy.json
```

### Add a tenant key

```bash
sqlrite-security add-key \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --key-id k1 \
  --key-material demo-secret-material \
  --active
```

### Rotate encrypted metadata to a new key

```bash
sqlrite-security rotate-key \
  --db sqlrite_demo.db \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --field secret_payload \
  --new-key-id k2 \
  --json
```

### Verify tenant key coverage

```bash
sqlrite-security verify-key \
  --db sqlrite_demo.db \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --field secret_payload \
  --key-id k2
```

### Export audit logs

```bash
sqlrite-security export-audit \
  --input .sqlrite/audit/server_audit.jsonl \
  --output audit_export.jsonl \
  --format jsonl \
  --tenant demo
```

### Run the server with secure defaults

```bash
sqlrite serve \
  --db sqlrite_demo.db \
  --bind 127.0.0.1:8099 \
  --secure-defaults \
  --authz-policy .sqlrite/rbac-policy.json \
  --audit-log .sqlrite/audit/server_audit.jsonl \
  --control-token dev-token
```

Security documentation:

- `docs/security/threat_model.md`
- `docs/security/compliance_posture.md`
- `docs/runbooks/security_rbac_defaults.md`
- `docs/runbooks/audit_export_key_rotation.md`

## Server Mode

Start the server:

```bash
sqlrite serve --db sqlrite_demo.db --bind 127.0.0.1:8099
```

Core endpoints:

- `GET /healthz`
- `GET /readyz`
- `GET /metrics`
- `POST /v1/query`
- `POST /v1/sql`
- `POST /v1/rerank-hook`

Basic query request:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":3}' \
  http://127.0.0.1:8099/v1/query | jq
```

SQL endpoint example:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"statement":"SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"}' \
  http://127.0.0.1:8099/v1/sql | jq
```

Control-plane capabilities include:

- replication status
- failover state
- recovery state
- snapshot management
- recent traces
- SLO reporting

HA and control-plane references:

- `docs/architecture/ha_replication_reference.md`
- `docs/runbooks/ha_control_plane.md`

## MCP Tool Server

Print the MCP manifest:

```bash
sqlrite mcp --db sqlrite_demo.db --print-manifest
```

Run MCP over stdio with auth:

```bash
sqlrite mcp --db sqlrite_demo.db --auth-token dev-token
```

MCP documentation:

- `docs/runbooks/mcp_tool_server.md`
- `docs/runbooks/agent_integrations_reference.md`

## Native gRPC Service

Start the gRPC service:

```bash
sqlrite grpc --db sqlrite_demo.db --bind 127.0.0.1:50051
```

Use the companion client:

```bash
sqlrite-grpc-client --addr 127.0.0.1:50051 health
sqlrite-grpc-client --addr 127.0.0.1:50051 query --text "agent memory" --top-k 2
sqlrite-grpc-client --addr 127.0.0.1:50051 sql --statement "SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"
```

gRPC documentation:

- `docs/runbooks/grpc_query_service.md`

## SDKs

### Python

Install from the SDK directory:

```bash
cd sdk/python
python -m pip install -e .
```

Example:

```python
from sqlrite_sdk import SqlRiteClient

client = SqlRiteClient(base_url="http://127.0.0.1:8099")
rows = client.query(query_text="agent memory", top_k=3)
print(rows)
```

### TypeScript

Install from the SDK directory:

```bash
cd sdk/typescript
npm install
npm run build
```

Example:

```ts
import { SqlRiteClient } from "@sqlrite/sdk";

const client = new SqlRiteClient({ baseUrl: "http://127.0.0.1:8099" });
const rows = await client.query({ queryText: "agent memory", topK: 3 });
console.log(rows);
```

## Benchmarks and Evaluation

### Single benchmark run

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

### Benchmark suite

```bash
cargo run --bin sqlrite-bench-suite -- \
  --profiles quick,10k \
  --concurrency-profile quick \
  --concurrency-levels 1,2,4 \
  --dataset examples/eval_dataset.json \
  --dataset-id readme_suite \
  --embedding-model deterministic-local-v1 \
  --hardware-class local-dev \
  --output bench_suite.json
```

### Evaluation report

```bash
cargo run --bin sqlrite-eval -- \
  --dataset examples/eval_dataset.json \
  --output eval_report.json \
  --index-mode hnsw_baseline
```

### Tuning knobs

Environment variables:

- `SQLRITE_VECTOR_STORAGE=f32|f16|int8`
- `SQLRITE_ANN_MIN_CANDIDATES=<int>`
- `SQLRITE_ANN_MAX_HAMMING_RADIUS=<int>`
- `SQLRITE_ANN_MAX_CANDIDATE_MULTIPLIER=<int>`
- `SQLRITE_ENABLE_ANN_PERSISTENCE=true|false`
- `SQLRITE_SQLITE_MMAP_SIZE=<bytes>`
- `SQLRITE_SQLITE_CACHE_SIZE_KIB=<kib>`

Example:

```bash
SQLRITE_VECTOR_STORAGE=int8 \
SQLRITE_SQLITE_MMAP_SIZE=536870912 \
SQLRITE_SQLITE_CACHE_SIZE_KIB=131072 \
sqlrite doctor --db sqlrite_demo.db --index-mode hnsw_baseline --json
```

## Backup, Restore, and Maintenance

### Health

```bash
sqlrite doctor --db sqlrite_demo.db
```

Sample output:

```text
sqlrite doctor
- version=1.0.0
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
```

### Backup and verify

```bash
sqlrite backup --source sqlrite_demo.db --dest sqlrite_backup.db
sqlrite backup verify --path sqlrite_backup.db
```

### Snapshot and PITR

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

### Compaction

```bash
sqlrite compact --db sqlrite_demo.db --json
```

## Examples

Runnable examples live under `examples/`.

```bash
cargo run --example basic_search
cargo run --example ingestion_worker
cargo run --example secure_tenant
cargo run --example tool_adapter
cargo run --example query_use_cases
```

## Packaging and Releases

Create a release archive from source:

```bash
bash scripts/create-release-archive.sh --version 1.0.0
```

Create Linux packages when `nfpm` is installed:

```bash
bash scripts/package-linux.sh --version 1.0.0
```

Build and run the Docker image:

```bash
docker build -t sqlrite:local .
docker run --rm -p 8099:8099 -v "$PWD:/data" sqlrite:local
```

Current release notes:

- `docs/releases/v1.0.0.md`
- `docs/release_policy.md`
- `docs/runbooks/ga_release_train.md`

## Documentation Map

Start here depending on what you need:

- SQL usage and patterns:
  - `docs/sql_cookbook.md`
- Migration:
  - `docs/migrations/sqlite_to_sqlrite.md`
  - `docs/migrations/pgvector_to_sqlrite.md`
  - `docs/migrations/api_first_vector_db_patterns.md`
- Operations:
  - `docs/runbooks/migration_cli_workflow.md`
  - `docs/runbooks/ha_control_plane.md`
  - `docs/runtime_config_profiles.md`
- Security:
  - `docs/security/threat_model.md`
  - `docs/security/compliance_posture.md`
  - `docs/runbooks/security_rbac_defaults.md`
  - `docs/runbooks/audit_export_key_rotation.md`
- Integration surfaces:
  - `docs/runbooks/mcp_tool_server.md`
  - `docs/runbooks/grpc_query_service.md`
  - `docs/contracts/api_freeze_v1.json`

## Development

### Build

```bash
cargo build
```

### Test

```bash
cargo test
```

### Format

```bash
cargo fmt --all
```

### Lint

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

## Repository Layout

- `src/` - core engine, CLI, HTTP server, SQL semantics, HA logic, security, migrations, and benchmarking
- `src/bin/` - companion binaries such as `sqlrite-security`, `sqlrite-reindex`, and `sqlrite-grpc-client`
- `sdk/python/` - Python SDK
- `sdk/typescript/` - TypeScript SDK
- `docs/` - product, operator, and release documentation
- `examples/` - runnable examples and datasets
- `scripts/` - install, packaging, and release automation

## Notes

- SQLRite is designed to be local-first, but it also supports server-mode deployment when you need shared access or integration endpoints.
- Deterministic ordering matters for agent systems. SQLRite uses explicit tie-breaking and planner fallback behavior to keep repeated runs stable on fixed data.
- If you need the lowest operational complexity, start with embedded mode and a single `.db` file before moving to server mode.
