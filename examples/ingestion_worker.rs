// Run: cargo run --example ingestion_worker
// Demonstrates: checkpointed ingestion + follow-up query.

use serde_json::json;
use sqlrite::{
    ChunkingStrategy, DeterministicEmbeddingProvider, IngestionRequest, IngestionWorker, Result,
    SearchRequest, SqlRite,
};

fn main() -> Result<()> {
    let db = SqlRite::open_in_memory()?;
    let provider = DeterministicEmbeddingProvider::new(64, "det-v1")?;

    let checkpoint = std::env::temp_dir().join("sqlrite-example-ingest.checkpoint.json");

    let worker = IngestionWorker::new(&db, provider).with_checkpoint_path(checkpoint);

    let request = IngestionRequest::from_direct(
        "job-1",
        "doc-1",
        "source-1",
        "acme",
        "# Intro\nSQLRite can ingest text with checkpoints.\n\n# Notes\nThe ingestion worker also writes tenant and offset metadata.",
    )
    .with_chunking(ChunkingStrategy::HeadingAware {
        max_chars: 80,
        overlap_chars: 10,
    })
    .with_batch_size(4)
    .with_metadata(json!({"source_kind": "demo"}));

    let report = worker.ingest(request)?;
    println!("== ingestion_worker report ==");
    println!(
        "ingested chunks: total={}, processed={}",
        report.total_chunks, report.processed_chunks
    );

    let results = db.search(SearchRequest::text("checkpoints tenant metadata", 3))?;
    println!("search results: {}", results.len());
    Ok(())
}
