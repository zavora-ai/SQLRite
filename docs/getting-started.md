# Getting Started

This guide takes you from install to a working local database.

## Best Install Path

For most developers, install from crates.io:

```bash
cargo install sqlrite
```

If you want the full companion CLI toolchain, install from a source checkout:

```bash
git clone https://github.com/zavora-ai/SQLRite.git
cd SQLRite
cargo install --path .
```

Verify:

```bash
command -v sqlrite
sqlrite --help
sqlrite init --db sqlrite_verify.db --seed-demo
sqlrite query --db sqlrite_verify.db --text "local memory" --top-k 1
```

## Install Options

| Option | Command | Notes |
|---|---|---|
| crates.io | `cargo install sqlrite` | installs the main `sqlrite` binary |
| Cargo source install | `cargo install --path .` | installs `sqlrite` plus companion binaries |
| Repo helper | `bash scripts/sqlrite-global-install.sh` | local checkout convenience |
| Release installer | `bash scripts/sqlrite-install.sh --version 1.0.2` | installs `sqlrite` only |

## First Working Flow

### 1. Create a demo database

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

### 2. Run a query

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

### 3. Run a quick smoke report

```bash
sqlrite quickstart --db sqlrite_quickstart.db --runs 5 --json --output quickstart.json
```

## Source Checkout Equivalents

If you have not installed the binaries yet:

| Installed command | Source checkout form |
|---|---|
| `sqlrite` | `cargo run --` |
| `sqlrite-security` | `cargo run --bin sqlrite-security --` |
| `sqlrite-reindex` | `cargo run --bin sqlrite-reindex --` |
| `sqlrite-grpc-client` | `cargo run --bin sqlrite-grpc-client --` |
| `sqlrite-serve` | `cargo run --bin sqlrite-serve --` |

## What to Read Next

1. `/Users/jameskaranja/Developer/projects/SQLRight/docs/embedded.md`
2. `/Users/jameskaranja/Developer/projects/SQLRight/docs/querying.md`
3. `/Users/jameskaranja/Developer/projects/SQLRight/docs/sql.md`
