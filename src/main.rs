use serde_json::json;
use sqlrite::{ChunkInput, RuntimeConfig, SearchRequest, SqlRite};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = SqlRite::open_with_config("sqlrite_demo.db", RuntimeConfig::default())?;

    if db.chunk_count()? == 0 {
        db.ingest_chunks(&[
            ChunkInput {
                id: "demo-1".to_string(),
                doc_id: "doc-a".to_string(),
                content: "Rust and SQLite are ideal for local-first AI agents.".to_string(),
                embedding: vec![0.92, 0.08, 0.0],
                metadata: json!({"tenant": "demo", "topic": "agent-memory"}),
                source: Some("seed/demo-1.md".to_string()),
            },
            ChunkInput {
                id: "demo-2".to_string(),
                doc_id: "doc-b".to_string(),
                content: "Hybrid retrieval mixes vector search with keyword signals.".to_string(),
                embedding: vec![0.65, 0.35, 0.0],
                metadata: json!({"tenant": "demo", "topic": "retrieval"}),
                source: Some("seed/demo-2.md".to_string()),
            },
            ChunkInput {
                id: "demo-3".to_string(),
                doc_id: "doc-c".to_string(),
                content: "Batching and metadata filters keep RAG pipelines deterministic."
                    .to_string(),
                embedding: vec![0.3, 0.7, 0.0],
                metadata: json!({"tenant": "demo", "topic": "ops"}),
                source: Some("seed/demo-3.md".to_string()),
            },
        ])?;
    }

    let request = SearchRequest::builder()
        .query_text("local-first agent memory")
        .query_embedding(vec![0.9, 0.1, 0.0])
        .alpha(0.6)
        .top_k(3)
        .build()?;
    let results = db.search(request)?;

    println!("Top matches:");
    for item in &results {
        println!(
            "- {} (doc: {}, score: {:.3})\n  {}",
            item.chunk_id, item.doc_id, item.hybrid_score, item.content
        );
    }

    Ok(())
}
