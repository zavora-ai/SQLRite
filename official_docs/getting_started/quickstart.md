# Quickstart Guide

This guide gets you from a working install to a seeded database, a first query, and a basic health report.

## What You Will Do

| Step | Command | Result |
|---|---|---|
| Inspect the CLI | `sqlrite --help` | confirms the CLI is reachable |
| Create a demo database | `sqlrite init --db sqlrite_demo.db --seed-demo` | creates a local database with sample content |
| Run a first query | `sqlrite query ...` | returns ranked retrieval results |
| Run a smoke-test report | `sqlrite quickstart ...` | writes a JSON readiness report |
| Check database health | `sqlrite doctor ...` | prints schema and integrity details |

## Running from a Source Checkout

If you have not installed the binaries yet, replace:

| Installed command | Source-checkout equivalent |
|---|---|
| `sqlrite` | `cargo run --` |
| `sqlrite-security` | `cargo run --bin sqlrite-security --` |
| `sqlrite-reindex` | `cargo run --bin sqlrite-reindex --` |
| `sqlrite-grpc-client` | `cargo run --bin sqlrite-grpc-client --` |
| `sqlrite-ingest` | `cargo run --bin sqlrite-ingest --` |

## Step 1: Inspect the CLI

```bash
sqlrite --help
```

What to look for:

- top-level commands such as `init`, `query`, `sql`, `serve`, `doctor`, and `backup`

## Step 2: Create a Demo Database

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
```

Expected result:

```text
initialized SQLRite database
- path=sqlrite_demo.db
- schema_version=3
- chunk_count=3
- profile=balanced
- index_mode=brute_force
```

What this means:

| Field | Meaning |
|---|---|
| `path` | database file created locally |
| `schema_version` | active SQLRite schema version |
| `chunk_count` | number of demo chunks inserted |
| `profile` | default retrieval profile |
| `index_mode` | default retrieval index mode |

## Step 3: Run a First Query

```bash
sqlrite query --db sqlrite_demo.db --text "agents local memory" --top-k 3
```

Expected output shape:

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

How to read it:

| Field | Meaning |
|---|---|
| `query_profile` | retrieval profile that was applied |
| `resolved_candidate_limit` | effective candidate set size |
| `hybrid` | final fused ranking score |
| `vector` | vector contribution |
| `text` | text contribution |

## Step 4: Generate a Quickstart Report

```bash
sqlrite quickstart \
  --db sqlrite_quickstart.db \
  --runs 5 \
  --json \
  --output quickstart.json
```

What to look for:

| Field | Healthy value |
|---|---|
| `successful_runs` | equal to `runs` |
| `success_rate` | `1.0` |
| `median_total_ms` | finite positive number |
| `p95_total_ms` | finite positive number |

Example output:

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

## Step 5: Check Database Health

```bash
sqlrite doctor --db sqlrite_demo.db
```

Use this when you want a quick integrity and configuration check.

Typical fields include:

- schema version
- chunk count
- index mode
- vector storage mode
- integrity status

## Next Steps

1. Learn the common retrieval flows in `official_docs/querying/query_patterns.md`.
2. Move into SQL-native retrieval with `official_docs/sql/sql_retrieval_guide.md`.
3. If you want HTTP or gRPC access, continue with `official_docs/integrations/server_and_api_guide.md`.
