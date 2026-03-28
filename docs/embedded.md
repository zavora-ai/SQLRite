# Embedded Usage

Embedded use is the primary SQLRite deployment model.

You open a database directly from your process, ingest content, and query it without a separate service.

## Why Embedded First

| Benefit | Why it matters |
|---|---|
| single local database file | easy deployment and testing |
| no network hop | lower latency and simpler failure modes |
| SQL + retrieval in one engine | fewer moving parts |
| deterministic ranking | easier evaluation and debugging |

## Minimal Example

Source: `/Users/jameskaranja/Developer/projects/SQLRight/examples/basic_search.rs`

```rust
use serde_json::json;
use sqlrite::{ChunkInput, Result, SearchRequest, SqlRite};

fn main() -> Result<()> {
    let db = SqlRite::open_in_memory()?;

    db.ingest_chunks(&[
        ChunkInput::new(
            "c1",
            "doc-rust",
            "Rust is a systems language for fast and safe services.",
            vec![0.95, 0.05, 0.0],
        )
        .with_metadata(json!({"tenant": "acme", "topic": "rust"})),
    ])?;

    let request = SearchRequest::hybrid("local-first sqlite", vec![0.45, 0.55, 0.0], 2);
    let results = db.search(request)?;

    println!("results={}", results.len());
    Ok(())
}
```

## Open Modes

| API | Use when |
|---|---|
| `SqlRite::open_in_memory()` | tests, demos, transient agent memory |
| `SqlRite::open_with_config(path, config)` | file-backed embedded apps |
| `SqlRite::open_in_memory_with_config(config)` | in-memory apps with custom runtime settings |

## Common Embedded Flow

1. Open the database.
2. Ingest chunks with ids, doc ids, content, embeddings, and metadata.
3. Query with text, vectors, or both.
4. Use `doctor`, backups, and compaction as the dataset grows.

## Embedded Examples

| Example | Command | Focus |
|---|---|---|
| basic search | `cargo run --example basic_search` | smallest embedded retrieval flow |
| query use cases | `cargo run --example query_use_cases` | text, vector, hybrid, filters, RRF |
| ingestion worker | `cargo run --example ingestion_worker` | chunking and checkpointed ingest |
| secure tenant | `cargo run --example secure_tenant` | encrypted metadata and tenant access |
| tool adapter | `cargo run --example tool_adapter` | tool-call style integration |

## File-Backed Embedded Setup

```bash
sqlrite init --db app_memory.db --seed-demo
sqlrite query --db app_memory.db --text "local memory" --top-k 3
```

Use file-backed mode when you want local persistence without running a server.

## Runtime Knobs

Useful environment variables for embedded workloads:

| Variable | Purpose |
|---|---|
| `SQLRITE_VECTOR_STORAGE` | choose `f32`, `f16`, or `int8` |
| `SQLRITE_SQLITE_MMAP_SIZE` | raise SQLite mmap size |
| `SQLRITE_SQLITE_CACHE_SIZE_KIB` | raise SQLite cache size |
| `SQLRITE_ENABLE_ANN_PERSISTENCE` | persist ANN sidecar state |

## When to Add a Server Boundary

Stay embedded if:

- the same process owns retrieval
- local latency matters most
- deployment simplicity matters more than multi-client access

Move to HTTP or gRPC if:

- multiple processes need shared access
- you need a network API
- you want SDK-first or agent-tool access
