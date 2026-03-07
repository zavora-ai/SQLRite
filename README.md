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

### Option 1: Install from Source with Cargo (Recommended)

This method works on **macOS**, **Linux**, and **Windows** — anywhere Rust is available.

> New to Rust? Install Cargo first. See [Prerequisites](#prerequisites) below.

---

#### Prerequisites

##### Install Rust & Cargo

If you don't have Rust installed yet, get it from [rustup.rs](https://rustup.rs):

**macOS / Linux:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**Windows:**  
Download and run [`rustup-init.exe`](https://win.rustup.rs) from [rustup.rs](https://rustup.rs).

After installation, restart your terminal, then confirm Rust is ready:
```bash
rustc --version
cargo --version
```

You should see version numbers printed for both. If you do, you're good to go.

---

#### Step 1 — Clone the Repository

Download the SQLRite source code to your machine:

```bash
git clone https://github.com/zavora-ai/SQLRite.git
cd SQLRite
```

> This creates a folder called `SQLRite` and moves you into it.

---

#### Step 2 — Build & Install

Compile and install the SQLRite CLI binaries with Cargo:

```bash
cargo install --path .
```

This installs `sqlrite` and companion tools such as `sqlrite-security`, `sqlrite-reindex`, `sqlrite-grpc-client`, `sqlrite-serve`, and `sqlrite-mcp`.

> This may take a minute or two on first run. Cargo is downloading and compiling dependencies.

---

#### Step 3 — Add SQLRite to Your PATH

After install, Cargo places the binary in `~/.cargo/bin`. You need this directory on your `PATH` so your terminal can find `sqlrite`.

**Check which `sqlrite` your shell resolves:**
```bash
command -v sqlrite
sqlrite --help
```
If `command -v sqlrite` points to `~/.cargo/bin/sqlrite` and `sqlrite --help` prints usage info, you're done.

If `sqlrite` is not found, or `command -v sqlrite` points to an older install, follow the instructions for your OS below.

<details>
<summary><strong>macOS / Linux</strong></summary>

Add the following line to your shell config file.

- Using **bash**? Edit `~/.bashrc` or `~/.bash_profile`
- Using **zsh** (default on modern macOS)? Edit `~/.zshrc`

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Then reload your shell:
```bash
source ~/.zshrc   # or ~/.bashrc, depending on your shell
```

</details>

<details>
<summary><strong>Windows</strong></summary>

1. Open **Start** → search for **"Environment Variables"** → click **"Edit the system environment variables"**
2. Click **"Environment Variables..."**
3. Under **User variables**, find and select `Path`, then click **Edit**
4. Click **New** and add:
   ```
   %USERPROFILE%\.cargo\bin
   ```
5. Click **OK** to save, then **restart your terminal**

</details>

---

#### Step 4 — Verify the Installation

Run these three commands to confirm everything works:

```bash
# 1. Check the CLI is reachable
sqlrite --help

# 2. Create a test database and seed it with demo data
sqlrite init --db sqlrite_verify.db --seed-demo

# 3. Run a sample query
sqlrite query --db sqlrite_verify.db --text "local memory" --top-k 1
```

**A successful install looks like this:**

| Command | Expected output |
|---|---|
| `sqlrite --help` | CLI usage and available commands are printed |
| `sqlrite init ...` | Database file is created with no errors |
| `sqlrite query ...` | At least one result is returned |

---

#### Troubleshooting

**`cargo: command not found`**  
Rust isn't installed or wasn't added to your PATH. Re-run the Rust installer and restart your terminal.

**`sqlrite: command not found` after install**  
`~/.cargo/bin` is not on your PATH. Follow [Step 3](#step-3--add-sqlrite-to-your-path) above.

**Build errors during `cargo install`**  
Make sure your Rust toolchain is up to date:
```bash
rustup update
```

**Still stuck?**  
Open an issue at [github.com/zavora-ai/SQLRite/issues](https://github.com/zavora-ai/SQLRite/issues).

### Option 2: Build and Install from This Repo

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

Commands below assume you want `sqlrite` on your `PATH`.

From a source checkout, replace:

- `sqlrite` with `cargo run --`
- `sqlrite-security` with `cargo run --bin sqlrite-security --`
- `sqlrite-reindex` with `cargo run --bin sqlrite-reindex --`
- `sqlrite-grpc-client` with `cargo run --bin sqlrite-grpc-client --`

What this section does:

| Step | Command | Result |
|---|---|---|
| Inspect the CLI | `sqlrite --help` | Confirms the CLI is installed and reachable |
| Create a demo database | `sqlrite init --db sqlrite_demo.db --seed-demo` | Creates a local `.db` file with sample data |
| Run a query | `sqlrite query --db sqlrite_demo.db --text "agents local memory" --top-k 3` | Returns the most relevant chunks |
| Run a quick health/perf smoke test | `sqlrite quickstart ...` | Produces a JSON report with timing and success stats |

### 1. Inspect the CLI

```bash
sqlrite --help
```

Result:

- You should see the top-level commands such as `init`, `query`, `sql`, `serve`, and `doctor`.

### 2. Create a local database with demo data

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
```

Result:

- SQLRite creates `sqlrite_demo.db`.
- The CLI prints the schema version, active profile, index mode, and seeded chunk count.

### 3. Run your first query

```bash
sqlrite query --db sqlrite_demo.db --text "agents local memory" --top-k 3
```

Sample output:

```text
query_profile=balanced resolved_candidate_limit=500
results=3
1. demo-1 | doc=doc-a | hybrid=1.000 | vector=0.000 | text=1.000
   Rust and SQLite are ideal for local-first AI agents.
2. demo-2 | doc=doc-b | hybrid=0.000 | vector=0.000 | text=0.000
   Hybrid retrieval mixes vector search with keyword signals.
3. demo-3 | doc=doc-c | hybrid=0.000 | vector=0.000 | text=0.000
   Batching and metadata filters keep RAG pipelines deterministic.
```

What to look for:

- `results=3` confirms the database has searchable content.
- The first hit should be `demo-1` for this demo dataset.

### 4. Run an end-to-end quickstart

```bash
sqlrite quickstart \
  --db sqlrite_quickstart.db \
  --runs 5 \
  --json \
  --output quickstart.json
```

Sample output (abridged):

```json
{
  "version": "1.0.0",
  "runs": 5,
  "successful_runs": 5,
  "success_rate": 1.0,
  "median_total_ms": 1.44,
  "median_query_ms": 0.09,
  "p95_total_ms": 2.37,
  "max_total_ms": 2.37
}
```

What to look for:

- `successful_runs` should match `runs`.
- `success_rate` should be `1.0` on the seeded demo database.
- `quickstart.json` should be written to the current directory.

## Common Query Patterns

Use these once `sqlrite_demo.db` exists.

If you want the sample outputs in this section to stay close to the examples, re-run:

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
```

| Pattern | Use when | Main flags |
|---|---|---|
| Text-only | You want BM25/FTS-style keyword retrieval | `--text` |
| Vector-only | You already have embeddings | `--vector` |
| Hybrid | You want lexical and vector scoring together | `--text`, `--vector`, `--alpha` |
| Metadata filter | You need tenant/topic constraints | `--filter` |
| Document scope | You want retrieval within one document | `--doc-id` |
| Query profile | You want to bias toward latency or recall | `--query-profile` |
| RRF fusion | You want reciprocal-rank fusion instead of weighted fusion | `--fusion rrf` |

### Text-only retrieval

```bash
sqlrite query --db sqlrite_demo.db --text "keyword signals retrieval" --top-k 3
```

Result:

- Returns chunks ranked by text match quality.

### Vector-only retrieval

```bash
sqlrite query --db sqlrite_demo.db --vector 0.95,0.05,0.0 --top-k 3
```

Result:

- Returns chunks ranked only by vector distance.

### Hybrid retrieval

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --vector 0.95,0.05,0.0 \
  --alpha 0.65 \
  --top-k 3
```

Result:

- Combines lexical and vector signals into a single `hybrid` score.

### Metadata-filtered retrieval

Use a separate scratch database for this pattern so the other query examples stay reproducible:

```bash
sqlrite init --db sqlrite_filter_demo.db --seed-demo
sqlrite ingest \
  --db sqlrite_filter_demo.db \
  --id chunk-meta-1 \
  --doc-id doc-meta-1 \
  --content "Agent memory stays local for demo tenants." \
  --embedding 0.95,0.05,0.0 \
  --metadata '{"tenant":"demo","topic":"memory"}'
```

Then query with metadata filters:

```bash
sqlrite query \
  --db sqlrite_filter_demo.db \
  --text "agent memory" \
  --filter tenant=demo \
  --filter topic=memory \
  --top-k 5
```

Result:

- Only chunks with matching metadata are considered.
- On a fresh demo database after the setup command above, this should return `chunk-meta-1`.

### Document-scoped retrieval

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --doc-id doc-a \
  --top-k 3
```

Result:

- Limits retrieval to chunks from `doc-a`.

### Query profile hints

Use `balanced`, `latency`, or `recall` to trade candidate set size for speed or coverage.

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --query-profile latency \
  --top-k 5
```

Rule of thumb:

| Profile | Best for |
|---|---|
| `latency` | low-latency interactive agent calls |
| `balanced` | default general-purpose retrieval |
| `recall` | offline evaluation and broader candidate search |

### RRF fusion

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --vector 0.95,0.05,0.0 \
  --fusion rrf \
  --rrf-k 60 \
  --top-k 5
```

Use this when:

- weighted fusion is overfitting one signal
- you want a stable score merge between text and vector ranks

## Interactive SQL Shell

Use the SQL shell when you want to inspect the schema, prototype retrieval SQL, or run one-off statements.

Start the SQL shell:

```bash
sqlrite sql --db sqlrite_demo.db
```

Useful shell helpers:

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

Result:

- Prints table rows directly to the terminal without entering the interactive shell.

## Retrieval SQL

Use SQL retrieval when you want one query surface for application data and retrieval logic.

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

Result:

- Returns the same rows with multiple distance metrics so you can compare ranking behavior.

### Retrieval functions

Available helpers:

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

Result:

- Shows vector dimension count plus example lexical and hybrid scores.

### Retrieval index DDL

```bash
sqlrite sql --db sqlrite_demo.db --execute "
CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw
ON chunks(embedding)
USING HNSW
WITH (m=16, ef_construction=64);"

sqlrite sql --db sqlrite_demo.db --execute "
CREATE TEXT INDEX IF NOT EXISTS idx_chunks_content_fts
ON chunks(content)
USING FTS5;"
```

Use this when:

- you are creating a database from SQL instead of via `sqlrite init`
- you want explicit retrieval indexes under SQL control

### SEARCH(...)

Use `SEARCH(...)` when you want a concise SQL-native hybrid retrieval form.

```bash
sqlrite sql --db sqlrite_demo.db --execute "
SELECT chunk_id, doc_id, hybrid_score
FROM SEARCH(
       'local memory',
       vector('0.95,0.05,0.0'),
       5,
       0.65,
       500,
       'balanced',
       NULL,
       NULL
     )
ORDER BY hybrid_score DESC, chunk_id ASC;"
```

Sample output (abridged):

```json
[
  {
    "chunk_id": "demo-1",
    "doc_id": "doc-a",
    "hybrid_score": 1.3808426976203918
  }
]
```

Result:

- Returns ranked rows with `chunk_id`, `doc_id`, and `hybrid_score`.

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

Choose the simplest path that matches your workload.

| Mode | Use when | Tool |
|---|---|---|
| Direct single-chunk ingest | You are testing, scripting, or loading a small number of records | `sqlrite ingest` |
| Worker ingest | You need resumable file-based jobs with checkpoints | `sqlrite-ingest` |

### Ingest a single chunk directly

```bash
sqlrite ingest \
  --db sqlrite_demo.db \
  --id chunk-100 \
  --doc-id doc-100 \
  --content "SQLRite keeps retrieval local and easy to reason about." \
  --embedding 0.9,0.1,0.0
```

Result:

- Adds one chunk to `sqlrite_demo.db` immediately.

### Use the ingestion worker

```bash
sqlrite-ingest \
  --db sqlrite_demo.db \
  --job-id docs-import \
  --doc-id guide-1 \
  --file ./README.md \
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

| Provider | Use when |
|---|---|
| `deterministic` | local development and stable test fixtures |
| `openai` | OpenAI-compatible hosted embeddings |
| `custom` | an internal or self-hosted HTTP embedding service |

### Deterministic local provider

```bash
sqlrite-reindex \
  --db sqlrite_demo.db \
  --provider deterministic \
  --target-model-version local-v2 \
  --batch-size 64
```

### OpenAI-compatible provider

Requires a reachable OpenAI-compatible embeddings endpoint and `OPENAI_API_KEY`.

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

Requires a running embedding service that accepts the configured request and response field names.

```bash
sqlrite-reindex \
  --db sqlrite_demo.db \
  --provider custom \
  --endpoint http://localhost:8080/embed \
  --input-field inputs \
  --embeddings-field embeddings \
  --target-model-version internal-v1
```

Result:

- Existing chunks are re-embedded and tagged with the new model version.

## Migration

SQLRite includes a first-class migration command for SQL databases and API-first vector database export shapes.

Pick the source that matches your current system:

| Source | Command family | Typical input |
|---|---|---|
| SQLite | `sqlrite migrate sqlite` | existing app/local `.db` file |
| libSQL | `sqlrite migrate libsql` | libSQL replica or SQLite-compatible file |
| pgvector export | `sqlrite migrate pgvector` | JSONL export |
| Qdrant, Weaviate, Milvus | `sqlrite migrate qdrant|weaviate|milvus` | JSONL export |

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

What to look for:

- `doctor` should report integrity as healthy.
- `query` should return rows from the migrated corpus.

Detailed migration documentation:

- `docs/migrations/sqlite_to_sqlrite.md`
- `docs/migrations/pgvector_to_sqlrite.md`
- `docs/migrations/api_first_vector_db_patterns.md`
- `docs/runbooks/migration_cli_workflow.md`

## Security and Multi-Tenant Operation

Use these commands when SQLRite is handling multiple tenants, encrypted metadata, or auditable server access.

### Generate a default RBAC policy

```bash
sqlrite-security init-policy --path .sqlrite/rbac-policy.json
```

Result:

- Creates a starter authorization policy you can review and commit.

### Add a tenant key

```bash
sqlrite-security add-key \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --key-id k1 \
  --key-material demo-secret-material \
  --active
```

Result:

- Adds an active encryption key entry for tenant `demo`.

Before rotating, add the new target key:

```bash
sqlrite-security add-key \
  --registry .sqlrite/tenant_keys.json \
  --tenant demo \
  --key-id k2 \
  --key-material demo-secret-material-v2 \
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

Result:

- Confirms encrypted rows for tenant `demo` can be resolved with key `k2`.
- On the seeded demo database, `rotated_chunks` will be `0` unless you already store encrypted metadata in `secret_payload`.

### Export audit logs

```bash
sqlrite-security export-audit \
  --input .sqlrite/audit/server_audit.jsonl \
  --output audit_export.jsonl \
  --format jsonl \
  --tenant demo
```

Result:

- Filters and exports tenant-specific audit records to `audit_export.jsonl`.

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

What this enables:

- secure-default server profile
- RBAC policy enforcement
- audit logging
- authenticated control-plane actions

In secure mode, query and SQL requests must include auth context headers such as:

```text
x-sqlrite-actor-id
x-sqlrite-tenant-id
x-sqlrite-roles
```

Example authenticated query:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-actor-id: reader-1" \
  -H "x-sqlrite-tenant-id: demo" \
  -H "x-sqlrite-roles: reader" \
  -d '{"query_text":"agent memory","top_k":3}' \
  http://127.0.0.1:8099/v1/query
```

Security documentation:

- `docs/security/threat_model.md`
- `docs/security/compliance_posture.md`
- `docs/runbooks/security_rbac_defaults.md`
- `docs/runbooks/audit_export_key_rotation.md`

## Server Mode

Use server mode when multiple clients need shared access over HTTP.

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

Endpoint guide:

| Endpoint | Use for | Result |
|---|---|---|
| `GET /healthz` | liveness checks | process health status |
| `GET /readyz` | readiness checks | database/service readiness |
| `GET /metrics` | Prometheus scraping | metrics text output |
| `POST /v1/query` | retrieval | JSON result rows |
| `POST /v1/sql` | SQL over HTTP | statement result rows |
| `POST /v1/rerank-hook` | custom rerank integration | rerank-ready response payload |

Basic query request:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":3}' \
  http://127.0.0.1:8099/v1/query
```

Expected result:

- A JSON array or object containing ranked retrieval results.
- In secure mode, include `x-sqlrite-actor-id`, `x-sqlrite-tenant-id`, and `x-sqlrite-roles`.

SQL endpoint example:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"statement":"SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;"}' \
  http://127.0.0.1:8099/v1/sql
```

Expected result:

- JSON rows for the requested SQL statement.

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

Use MCP mode when you want to expose SQLRite as a tool to an agent runtime.

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

Use gRPC when you want a typed network API instead of shelling out to the CLI.

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

Result:

- The companion client confirms health, retrieval, and SQL requests against the running service.

gRPC documentation:

- `docs/runbooks/grpc_query_service.md`

## SDKs

Use the SDKs when you want application-level access without building raw HTTP requests.

### Python

Install from the SDK directory:

```bash
cd sdk/python
python -m pip install -e .
```

Example:

```python
from sqlrite_sdk import SqlRiteClient

client = SqlRiteClient("http://127.0.0.1:8099")
response = client.query(query_text="agent memory", top_k=3)
print(response["row_count"])
print(response["rows"][0]["chunk_id"])
```

Result:

- `response` is a JSON envelope with `row_count` and `rows`.

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

const client = new SqlRiteClient("http://127.0.0.1:8099");
const response = await client.query({ query_text: "agent memory", top_k: 3 });
console.log(response.row_count);
console.log(response.rows[0].chunk_id);
```

Result:

- `response` is the same query envelope returned by the HTTP API.

## Benchmarks and Evaluation

Use these commands to measure throughput, latency, and retrieval quality on your own datasets.

| Tool | Purpose | Output |
|---|---|---|
| `sqlrite benchmark` | one benchmark run | one JSON report |
| `sqlrite-bench-suite` | benchmark matrix/suite | suite JSON across profiles |
| `sqlrite-eval` | ranking-quality evaluation | eval JSON with metrics |

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

Result:

- Writes a single benchmark report to `bench_report.json`.

### Benchmark suite

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

Result:

- Runs multiple benchmark profiles and writes a consolidated suite report.

### Evaluation report

```bash
sqlrite-eval \
  --dataset examples/eval_dataset.json \
  --output eval_report.json \
  --index-mode hnsw_baseline
```

Result:

- Writes ranking-quality metrics such as recall and MRR to `eval_report.json`.

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

Use these when:

- memory usage needs to be reduced
- candidate expansion needs tuning
- SQLite cache and mmap settings need adjustment

## Backup, Restore, and Maintenance

These commands cover routine operational checks for a local or server deployment.

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

What to look for:

- `integrity_ok=true`
- expected `schema_version`
- expected `index_mode` and `vector_storage`

### Backup and verify

```bash
sqlrite backup --source sqlrite_demo.db --dest sqlrite_backup.db
sqlrite backup verify --path sqlrite_backup.db
```

Result:

- Creates a copy and verifies that the backup is structurally valid.

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

Result:

- Creates snapshot metadata under `backups/` and can restore to `restored.db` for a chosen point in time.

### Compaction

```bash
sqlrite compact --db sqlrite_demo.db --json
```

Result:

- Runs maintenance and prints a JSON summary.

## Examples

Runnable examples live under `examples/`.

| Example | Purpose |
|---|---|
| `basic_search` | minimal retrieval flow |
| `ingestion_worker` | resumable ingest workflow |
| `secure_tenant` | multi-tenant security setup |
| `tool_adapter` | tool-facing integration pattern |
| `query_use_cases` | multiple retrieval/query examples |

```bash
cargo run --example basic_search
cargo run --example ingestion_worker
cargo run --example secure_tenant
cargo run --example tool_adapter
cargo run --example query_use_cases
```

## Packaging and Releases

Use these commands if you are packaging SQLRite for distribution instead of just running it locally.

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

| Need | Documents |
|---|---|
| SQL usage and patterns | `docs/sql_cookbook.md` |
| Migration | `docs/migrations/sqlite_to_sqlrite.md`, `docs/migrations/pgvector_to_sqlrite.md`, `docs/migrations/api_first_vector_db_patterns.md` |
| Operations | `docs/runbooks/migration_cli_workflow.md`, `docs/runbooks/ha_control_plane.md`, `docs/runtime_config_profiles.md` |
| Security | `docs/security/threat_model.md`, `docs/security/compliance_posture.md`, `docs/runbooks/security_rbac_defaults.md`, `docs/runbooks/audit_export_key_rotation.md` |
| Integration surfaces | `docs/runbooks/mcp_tool_server.md`, `docs/runbooks/grpc_query_service.md`, `docs/contracts/api_freeze_v1.json` |

## Development

Core contributor commands:

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

| Path | Purpose |
|---|---|
| `src/` | core engine, CLI, HTTP server, SQL semantics, HA logic, security, migrations, and benchmarking |
| `src/bin/` | companion binaries such as `sqlrite-security`, `sqlrite-reindex`, and `sqlrite-grpc-client` |
| `sdk/python/` | Python SDK |
| `sdk/typescript/` | TypeScript SDK |
| `docs/` | product, operator, and release documentation |
| `examples/` | runnable examples and datasets |
| `scripts/` | install, packaging, and release automation |

## Notes

- SQLRite is designed to be local-first, but it also supports server-mode deployment when you need shared access or integration endpoints.
- Deterministic ordering matters for agent systems. SQLRite uses explicit tie-breaking and planner fallback behavior to keep repeated runs stable on fixed data.
- If you need the lowest operational complexity, start with embedded mode and a single `.db` file before moving to server mode.
