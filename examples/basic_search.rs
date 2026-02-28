// Run: cargo run --example basic_search
// Demonstrates: minimal hybrid search with ergonomic constructors.

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
        ChunkInput::new(
            "c2",
            "doc-rag",
            "RAG combines search with generation to ground responses.",
            vec![0.70, 0.30, 0.0],
        )
        .with_metadata(json!({"tenant": "acme", "topic": "rag"})),
        ChunkInput::new(
            "c3",
            "doc-sqlite",
            "SQLite is ideal for embedded and local-first applications.",
            vec![0.40, 0.60, 0.0],
        )
        .with_metadata(json!({"tenant": "acme", "topic": "sqlite"})),
    ])?;

    let request = SearchRequest::hybrid("local-first sqlite", vec![0.45, 0.55, 0.0], 2);
    let results = db.search(request)?;

    println!("== basic_search results ==");
    for result in results {
        println!(
            "{} | doc={} | score={:.3}",
            result.chunk_id, result.doc_id, result.hybrid_score
        );
    }

    Ok(())
}
