// Run: cargo run --example query_use_cases
// Demonstrates: text/vector/hybrid, metadata filters, doc scope, RRF, and candidate tuning.

use serde_json::json;
use sqlrite::{ChunkInput, FusionStrategy, Result, SearchRequest, SearchResult, SqlRite};

fn main() -> Result<()> {
    let db = SqlRite::open_in_memory()?;
    seed(&db)?;

    println!("== 1) Text-only query ==");
    print_results(&db.search(SearchRequest::text("local sqlite rag", 3))?, 3);

    println!("\n== 2) Vector-only query ==");
    print_results(
        &db.search(SearchRequest::vector(vec![0.95, 0.05, 0.0], 3))?,
        3,
    );

    println!("\n== 3) Hybrid query with alpha tuning ==");
    let mut hybrid = SearchRequest::hybrid("audit logging", vec![0.55, 0.45, 0.0], 3);
    hybrid.alpha = 0.35;
    print_results(&db.search(hybrid)?, 3);

    println!("\n== 4) Metadata-filtered query (tenant/topic) ==");
    let filtered = SearchRequest::builder()
        .query_text("policy governance")
        .metadata_filter("tenant", "acme")
        .metadata_filter("topic", "security")
        .top_k(5)
        .build()?;
    print_results(&db.search(filtered)?, 5);

    println!("\n== 5) Doc-scoped query ==");
    let doc_scoped = SearchRequest::builder()
        .query_text("ingestion checkpoint")
        .doc_id("doc-ingest")
        .top_k(5)
        .build()?;
    print_results(&db.search(doc_scoped)?, 5);

    println!("\n== 6) RRF fusion query ==");
    let rrf = SearchRequest::builder()
        .query_text("latency throughput")
        .query_embedding(vec![0.15, 0.85, 0.0])
        .fusion_strategy(FusionStrategy::ReciprocalRankFusion {
            rank_constant: 60.0,
        })
        .top_k(3)
        .build()?;
    print_results(&db.search(rrf)?, 3);

    println!("\n== 7) Candidate-limit tuning (precision/latency control) ==");
    let tuned = SearchRequest::builder()
        .query_text("retrieval memory")
        .query_embedding(vec![0.7, 0.3, 0.0])
        .candidate_limit(25)
        .top_k(3)
        .build()?;
    print_results(&db.search(tuned)?, 3);

    Ok(())
}

fn seed(db: &SqlRite) -> Result<()> {
    db.ingest_chunks(&[
        ChunkInput::new(
            "c1",
            "doc-rag",
            "SQLite powers local-first RAG memory for agents.",
            vec![0.98, 0.02, 0.0],
        )
        .with_metadata(json!({"tenant": "acme", "topic": "retrieval"})),
        ChunkInput::new(
            "c2",
            "doc-security",
            "Policy enforcement and audit logging are critical for tenant governance.",
            vec![0.50, 0.50, 0.0],
        )
        .with_metadata(json!({"tenant": "acme", "topic": "security"})),
        ChunkInput::new(
            "c3",
            "doc-ingest",
            "Ingestion checkpoints support restart-safe pipelines.",
            vec![0.75, 0.25, 0.0],
        )
        .with_metadata(json!({"tenant": "acme", "topic": "ingestion"})),
        ChunkInput::new(
            "c4",
            "doc-perf",
            "Benchmark latency and throughput to tune retrieval performance.",
            vec![0.10, 0.90, 0.0],
        )
        .with_metadata(json!({"tenant": "acme", "topic": "performance"})),
        ChunkInput::new(
            "c5",
            "doc-rag",
            "Hybrid retrieval blends embeddings with lexical ranking.",
            vec![0.85, 0.15, 0.0],
        )
        .with_metadata(json!({"tenant": "acme", "topic": "retrieval"})),
        ChunkInput::new(
            "c6",
            "doc-security",
            "Key rotation and encrypted metadata protect sensitive fields.",
            vec![0.35, 0.65, 0.0],
        )
        .with_metadata(json!({"tenant": "beta", "topic": "security"})),
    ])?;
    Ok(())
}

fn print_results(results: &[SearchResult], max_rows: usize) {
    if results.is_empty() {
        println!("(no results)");
        return;
    }

    for result in results.iter().take(max_rows) {
        let tenant = result
            .metadata
            .get("tenant")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("n/a");
        let topic = result
            .metadata
            .get("topic")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("n/a");
        println!(
            "- {} | doc={} | tenant={} | topic={} | hybrid={:.3} | vector={:.3} | text={:.3}",
            result.chunk_id,
            result.doc_id,
            tenant,
            topic,
            result.hybrid_score,
            result.vector_score,
            result.text_score
        );
    }
}
