# SQLRite

SQLRite is an embedded, SQLite-based retrieval engine for AI agents and RAG workloads.

The primary use case is local, in-process retrieval with a single database file, SQL-native query syntax, and production-grade operational tooling when you need it.

## Why SQLRite

- Embedded first: start with a local `.db` file, no extra service required.
- SQL-native retrieval: use CLI, Rust APIs, SQL operators, or `SEARCH(...)`.
- One engine for lexical, vector, and hybrid retrieval.
- Deterministic ranking and tenant-aware filtering.
- Optional server surfaces: HTTP, compact HTTP, gRPC, and MCP.
- Packaging paths for source builds, release archives, and Docker.

## Embedded Performance Snapshot

Current benchmark snapshot on a deterministic filtered cosine workload (`5k` records, `120` measured queries, `64` dimensions, `8` tenants, `top_k=10`):

| Mode | QPS | p95 latency | Recall@10 |
|---|---:|---:|---:|
| `brute_force` embedded | `3380.07` | `0.3543 ms` | `1.0` |
| `hnsw_baseline` embedded | `3530.96` | `0.3327 ms` | `1.0` |
| `brute_force` HTTP compact | `1807.27` | `0.7538 ms` | `1.0` |
| `hnsw_baseline` HTTP compact | `1828.17` | `0.7070 ms` | `1.0` |

These numbers are strongest in embedded mode, which is the main SQLRite deployment model.

## Install

### Option 1: Install from crates.io

This is the fastest way to get the main `sqlrite` CLI on your machine.

```bash
cargo install sqlrite
```

Verify the install:

```bash
sqlrite --help
sqlrite init --db sqlrite_verify.db --seed-demo
sqlrite query --db sqlrite_verify.db --text "local memory" --top-k 1
```

Important detail:

- `cargo install sqlrite` installs the main `sqlrite` binary
- if you want the companion tools too, use the source install path below
- if you are embedding SQLRite in a Rust application instead of installing the CLI, use `cargo add sqlrite`

### Option 2: Install from source with Cargo

This is the best path if you want the full CLI toolchain.

#### Prerequisites

Install Rust and Cargo from [rustup.rs](https://rustup.rs), then confirm:

```bash
rustc --version
cargo --version
```

#### Download the repo

```bash
git clone https://github.com/zavora-ai/SQLRite.git
cd SQLRite
```

#### Install the binaries

```bash
cargo install --path .
```

This installs:

- `sqlrite`
- `sqlrite-security`
- `sqlrite-reindex`
- `sqlrite-ingest`
- `sqlrite-serve`
- `sqlrite-grpc-client`
- `sqlrite-mcp`
- benchmark and evaluation helpers

#### Verify the install

```bash
command -v sqlrite
sqlrite --help
sqlrite init --db sqlrite_verify.db --seed-demo
sqlrite query --db sqlrite_verify.db --text "local memory" --top-k 1
```

A successful install looks like this:

| Command | Expected result |
|---|---|
| `command -v sqlrite` | points to your installed binary |
| `sqlrite --help` | prints CLI usage |
| `sqlrite init ...` | creates and seeds a local database |
| `sqlrite query ...` | returns at least one result |

If Cargo's bin directory is not on your `PATH`, add it:

- macOS / Linux: `export PATH="$HOME/.cargo/bin:$PATH"`
- Windows: add `%USERPROFILE%\.cargo\bin` to your user `Path`

### Option 3: Install from this repo with the helper script

```bash
bash scripts/sqlrite-global-install.sh
```

This is a Unix-oriented convenience flow for local checkouts.

### Option 4: Install from a GitHub release

```bash
bash scripts/sqlrite-install.sh --version 1.0.2
```

Important detail:

- the release installer currently installs `sqlrite`
- if you want the companion tools too, use the Cargo install path

## 5-Minute Start

Commands below assume `sqlrite` is on your `PATH`.

From a source checkout, replace:

- `sqlrite` with `cargo run --`
- `sqlrite-security` with `cargo run --bin sqlrite-security --`
- `sqlrite-reindex` with `cargo run --bin sqlrite-reindex --`
- `sqlrite-grpc-client` with `cargo run --bin sqlrite-grpc-client --`
- `sqlrite-serve` with `cargo run --bin sqlrite-serve --`

### 1. Create a local demo database

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
```

Expected output:

```text
initialized SQLRite database
- path=sqlrite_demo.db
- schema_version=4
- chunk_count=3
- profile=balanced
- index_mode=brute_force
```

### 2. Run your first query

```bash
sqlrite query --db sqlrite_demo.db --text "agents local memory" --top-k 3
```

Expected output shape:

```text
query_profile=balanced resolved_candidate_limit=500
results=3
1. demo-1 | doc=doc-a | hybrid=1.000 | vector=0.000 | text=1.000
   Rust and SQLite are ideal for local-first AI agents.
```

### 3. Run the quick health/perf smoke path

```bash
sqlrite quickstart --db sqlrite_quickstart.db --runs 5 --json --output quickstart.json
```

Look for:

- `successful_runs` equal to `runs`
- finite `median_total_ms`
- finite `p95_total_ms`

## Embedded Rust Example

The embedded path is the core product. Use `SqlRite` directly for simple in-process cases:

Add the crate to your project:

```bash
cargo add sqlrite
```

```rust
use serde_json::json;
use sqlrite::{ChunkInput, Result, SearchRequest, SqlRite};

fn main() -> Result<()> {
    let db = SqlRite::open_in_memory()?;

    db.ingest_chunks(&[
        ChunkInput::new(
            "c1",
            "doc-rust",
            "Rust and SQLite work well for local-first retrieval.",
            vec![0.95, 0.05, 0.0],
        )
        .with_metadata(json!({"tenant": "acme", "topic": "rust"})),
    ])?;

    let results = db.search(SearchRequest::hybrid(
        "local-first retrieval",
        vec![0.9, 0.1, 0.0],
        3,
    ))?;

    println!("results={}", results.len());
    Ok(())
}
```

For concurrent async applications, prefer `SqlRiteHandle` so each operation opens its own short-lived connection instead of sharing one `SqlRite` behind a global mutex:

```rust
use sqlrite::{DocumentIngestOptions, Result, SearchRequest, SqlRiteHandle};

fn main() -> Result<()> {
    let db = SqlRiteHandle::open("app.db")?;

    db.ingest_document_text(
        "doc-rust",
        "Rust and SQLite work well for local-first retrieval.",
        DocumentIngestOptions::default(),
    )?;

    let results = db.search(SearchRequest::text_only("local-first retrieval", 3))?;
    let diagnostics = db.diagnostics()?;

    println!("results={}", results.len());
    println!("documents={}", diagnostics.document_count);
    Ok(())
}
```

If your embedding provider is external, ingest text first and backfill vectors later:

```rust
use sqlrite::{Result, SqlRite, TextChunkInput};

fn main() -> Result<()> {
    let db = SqlRite::open_in_memory()?;
    db.ingest_text_chunk(&TextChunkInput::new(
        "c1",
        "doc-rust",
        "Text first, embeddings later.",
    ))?;
    db.update_chunk_embedding("c1", vec![1.0, 0.0])?;
    Ok(())
}
```

See [`examples/basic_search.rs`](examples/basic_search.rs) and [`docs/embedded.md`](docs/embedded.md) for fuller embedded flows.

## Common Query Patterns

### Text-only query

```bash
sqlrite query --db sqlrite_demo.db --text "keyword signals retrieval" --top-k 3
```

### Vector-only query

```bash
sqlrite query --db sqlrite_demo.db --vector 0.95,0.05,0.0 --top-k 3
```

### Hybrid query

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --vector 0.95,0.05,0.0 \
  --alpha 0.65 \
  --top-k 3
```

### Filtered query

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "agent memory" \
  --filter tenant=demo \
  --top-k 3
```

## SQL-Native Retrieval

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

See [`docs/sql.md`](docs/sql.md) for operators, helper functions, and index DDL.

## Server Mode

Embedded is the primary deployment path. When you need a service boundary, use HTTP, gRPC, or MCP.

### Start HTTP

```bash
sqlrite serve --db sqlrite_demo.db --bind 127.0.0.1:8099
```

### Query over compact HTTP

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":3}' \
  http://127.0.0.1:8099/v1/query-compact
```

Use `/v1/query-compact` when you want lower-overhead, array-oriented responses for agents or benchmarks.

## Security

SQLRite supports:

- RBAC policy files
- tenant key registries
- encrypted metadata rotation
- audit export
- secure server defaults

Starter flow:

```bash
sqlrite-security init-policy --path .sqlrite/rbac-policy.json
sqlrite-security add-key --registry .sqlrite/tenant_keys.json --tenant demo --key-id k1 --key-material demo-secret --active
```

See [`docs/security.md`](docs/security.md).

## Distribution

### Release archive

```bash
bash scripts/create-release-archive.sh --version 1.0.2
```

### Docker

```bash
docker build -t sqlrite:local .
docker run --rm -p 8099:8099 -v "$PWD/docker-data:/data" sqlrite:local
```

### Seeded Docker Compose demo

```bash
docker compose -f deploy/docker-compose.seeded-demo.yml up --build
```

## Documentation

| Topic | Path |
|---|---|
| Detailed project guide | [`PROJECT_README.md`](PROJECT_README.md) |
| Docs home | [`docs/README.md`](docs/README.md) |
| Getting started | [`docs/getting-started.md`](docs/getting-started.md) |
| Embedded usage | [`docs/embedded.md`](docs/embedded.md) |
| Query patterns | [`docs/querying.md`](docs/querying.md) |
| SQL retrieval | [`docs/sql.md`](docs/sql.md) |
| Ingestion and reindexing | [`docs/ingestion.md`](docs/ingestion.md) |
| Server, gRPC, MCP | [`docs/server-api.md`](docs/server-api.md) |
| Security | [`docs/security.md`](docs/security.md) |
| Migrations | [`docs/migrations.md`](docs/migrations.md) |
| Operations | [`docs/operations.md`](docs/operations.md) |
| Performance | [`docs/performance.md`](docs/performance.md) |
| Examples | [`docs/examples.md`](docs/examples.md) |
| Distribution | [`docs/distribution.md`](docs/distribution.md) |
| Release policy | [`docs/release_policy.md`](docs/release_policy.md) |
| API docs | [`docs.rs/sqlrite`](https://docs.rs/sqlrite) |

## Examples

| Example | Run it |
|---|---|
| Minimal embedded search | `cargo run --example basic_search` |
| Query patterns | `cargo run --example query_use_cases` |
| Ingestion worker | `cargo run --example ingestion_worker` |
| Secure tenant flow | `cargo run --example secure_tenant` |
| Rotation workflow fixture | `cargo run --example security_rotation_workflow` |
| Tool adapter | `cargo run --example tool_adapter` |

## Repository Layout

| Path | Purpose |
|---|---|
| `src` | core engine and CLI |
| `src/bin` | companion binaries |
| `examples` | runnable examples |
| `sdk/python` | Python SDK |
| `sdk/typescript` | TypeScript SDK |
| `docs` | public documentation |
| `deploy` | Docker and deployment assets |
