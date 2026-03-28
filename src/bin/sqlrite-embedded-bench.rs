use serde::Deserialize;
use serde_json::json;
use sqlrite::{
    ChunkInput, QueryProfile, Result, RuntimeConfig, SearchRequest, SqlRite, VectorIndexMode,
};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::File;
use std::io::{BufReader, Write};
use std::time::Instant;

#[derive(Debug, Deserialize)]
struct WorkloadRecord {
    id: u64,
    chunk_id: String,
    doc_id: String,
    tenant: String,
    embedding: Vec<f32>,
    content: String,
}

#[derive(Debug, Deserialize)]
struct WorkloadQuery {
    vector: Vec<f32>,
    tenant: String,
    ground_truth_ids: Vec<u64>,
}

#[derive(Debug, Deserialize)]
struct WorkloadFile {
    records: Vec<WorkloadRecord>,
    queries: Vec<WorkloadQuery>,
    top_k: usize,
    warmup: usize,
}

fn percentile_ms(latencies_s: &[f64], percentile: f64) -> f64 {
    if latencies_s.is_empty() {
        return 0.0;
    }
    let mut sorted = latencies_s.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let rank = percentile.clamp(0.0, 100.0) / 100.0 * (sorted.len().saturating_sub(1)) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    let weight = rank - lower as f64;
    let value = sorted[lower] * (1.0 - weight) + sorted[upper] * weight;
    value * 1000.0
}

fn compute_metrics(results: &[Vec<u64>], queries: &[WorkloadQuery], top_k: usize) -> (f64, f64) {
    if results.is_empty() || queries.is_empty() {
        return (0.0, 0.0);
    }
    let mut top1_hits = 0usize;
    let mut recall_total = 0.0f64;
    for (returned, query) in results.iter().zip(queries.iter()) {
        let truth = &query.ground_truth_ids[..query.ground_truth_ids.len().min(top_k)];
        if let (Some(first_returned), Some(first_truth)) = (returned.first(), truth.first())
            && first_returned == first_truth
        {
            top1_hits += 1;
        }
        let truth_set: HashSet<u64> = truth.iter().copied().collect();
        let overlap = returned
            .iter()
            .take(top_k)
            .filter(|candidate| truth_set.contains(candidate))
            .count();
        recall_total += overlap as f64 / truth.len().max(1) as f64;
    }
    (
        top1_hits as f64 / queries.len() as f64,
        recall_total / queries.len() as f64,
    )
}

fn parse_index_mode(value: &str) -> Result<VectorIndexMode> {
    match value {
        "brute_force" => Ok(VectorIndexMode::BruteForce),
        "hnsw_baseline" => Ok(VectorIndexMode::HnswBaseline),
        "lsh_ann" => Ok(VectorIndexMode::LshAnn),
        "disabled" => Ok(VectorIndexMode::Disabled),
        _ => Err(std::io::Error::other(format!("unsupported --index-mode `{value}`")).into()),
    }
}

fn parse_args() -> Result<(String, VectorIndexMode)> {
    let mut workload = None;
    let mut index_mode = VectorIndexMode::BruteForce;
    let args: Vec<String> = env::args().collect();
    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--workload" => {
                i += 1;
                workload = args.get(i).cloned();
            }
            "--index-mode" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| std::io::Error::other("missing value for --index-mode"))?;
                index_mode = parse_index_mode(value)?;
            }
            flag => {
                return Err(std::io::Error::other(format!("unknown argument `{flag}`")).into());
            }
        }
        i += 1;
    }

    let workload =
        workload.ok_or_else(|| std::io::Error::other("missing required --workload PATH"))?;
    Ok((workload, index_mode))
}

fn main() -> Result<()> {
    let (workload_path, index_mode) = parse_args()?;
    let reader = BufReader::new(File::open(&workload_path)?);
    let workload: WorkloadFile = serde_json::from_reader(reader)?;

    let runtime_config = RuntimeConfig::default()
        .with_vector_index_mode(index_mode)
        .with_ann_persistence(false);
    let db = SqlRite::open_in_memory_with_config(runtime_config)?;

    let setup_started = Instant::now();
    let ingest_batch: Vec<ChunkInput> = workload
        .records
        .iter()
        .map(|record| ChunkInput {
            id: record.chunk_id.clone(),
            doc_id: record.doc_id.clone(),
            content: record.content.clone(),
            embedding: record.embedding.clone(),
            metadata: json!({ "tenant": record.tenant }),
            source: None,
        })
        .collect();
    db.ingest_chunks(&ingest_batch)?;
    let setup_seconds = setup_started.elapsed().as_secs_f64();

    let id_lookup: HashMap<&str, u64> = workload
        .records
        .iter()
        .map(|record| (record.chunk_id.as_str(), record.id))
        .collect();

    for query in workload.queries.iter().take(workload.warmup) {
        let _ = db.search(
            SearchRequest::builder()
                .query_embedding(query.vector.clone())
                .metadata_filter("tenant", &query.tenant)
                .top_k(workload.top_k)
                .candidate_limit(workload.top_k)
                .query_profile(QueryProfile::Latency)
                .include_payloads(false)
                .build()?,
        )?;
    }

    let mut latencies = Vec::with_capacity(workload.queries.len().saturating_sub(workload.warmup));
    let mut results = Vec::with_capacity(workload.queries.len().saturating_sub(workload.warmup));
    let started = Instant::now();
    for query in workload.queries.iter().skip(workload.warmup) {
        let query_started = Instant::now();
        let search_results = db.search(
            SearchRequest::builder()
                .query_embedding(query.vector.clone())
                .metadata_filter("tenant", &query.tenant)
                .top_k(workload.top_k)
                .candidate_limit(workload.top_k)
                .query_profile(QueryProfile::Latency)
                .include_payloads(false)
                .build()?,
        )?;
        latencies.push(query_started.elapsed().as_secs_f64());
        let ids = search_results
            .into_iter()
            .filter_map(|row| id_lookup.get(row.chunk_id.as_str()).copied())
            .collect::<Vec<_>>();
        results.push(ids);
    }
    let elapsed = started.elapsed().as_secs_f64();
    let (top1_hit_rate, recall_at_k) = compute_metrics(
        &results,
        &workload.queries[workload.warmup..],
        workload.top_k,
    );
    let report = json!({
        "qps": if elapsed > 0.0 { results.len() as f64 / elapsed } else { 0.0 },
        "p50_ms": percentile_ms(&latencies, 50.0),
        "p95_ms": percentile_ms(&latencies, 95.0),
        "top1_hit_rate": top1_hit_rate,
        "recall_at_k": recall_at_k,
        "setup_seconds": setup_seconds,
    });

    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &report)?;
    stdout.write_all(b"\n")?;
    Ok(())
}
