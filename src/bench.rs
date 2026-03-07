use crate::{
    ChunkInput, FusionStrategy, QueryProfile, Result, RuntimeConfig, SearchRequest, SqlRite,
    SqlRiteError,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const TOPIC_KEYWORDS: &[&[&str]] = &[
    &["rust", "memory", "safety", "agents"],
    &["sqlite", "local", "storage", "embedded"],
    &["retrieval", "vector", "hybrid", "ranking"],
    &["metadata", "tenant", "filter", "policy"],
    &["pipeline", "chunking", "embedding", "batch"],
    &["latency", "throughput", "benchmark", "qps"],
    &["observability", "trace", "metrics", "alerts"],
    &["security", "audit", "compliance", "governance"],
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub corpus_size: usize,
    pub query_count: usize,
    pub warmup_queries: usize,
    pub concurrency: usize,
    pub embedding_dim: usize,
    pub top_k: usize,
    pub candidate_limit: usize,
    pub query_profile: QueryProfile,
    pub alpha: f32,
    pub fusion_strategy: FusionStrategy,
    pub batch_size: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            corpus_size: 10_000,
            query_count: 500,
            warmup_queries: 100,
            concurrency: 1,
            embedding_dim: 128,
            top_k: 10,
            candidate_limit: 500,
            query_profile: QueryProfile::Balanced,
            alpha: 0.65,
            fusion_strategy: FusionStrategy::Weighted,
            batch_size: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkLatency {
    pub avg_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub corpus_size: usize,
    pub query_count: usize,
    pub warmup_queries: usize,
    pub concurrency: usize,
    pub embedding_dim: usize,
    pub top_k: usize,
    pub candidate_limit: usize,
    pub effective_candidate_limit: usize,
    pub query_profile: String,
    pub alpha: f32,
    pub fusion_strategy: String,
    pub vector_index_mode: String,
    pub vector_storage_kind: String,
    pub sqlite_mmap_size_bytes: i64,
    pub sqlite_cache_size_kib: i64,
    pub ingest_duration_ms: f64,
    pub query_duration_ms: f64,
    pub total_duration_ms: f64,
    pub qps: f64,
    pub top1_hit_rate: f64,
    pub latency: BenchmarkLatency,
    #[serde(default)]
    pub ingest_chunks_per_sec: f64,
    #[serde(default)]
    pub embedding_bytes: usize,
    #[serde(default)]
    pub content_bytes: usize,
    #[serde(default)]
    pub dataset_payload_bytes: usize,
    #[serde(default)]
    pub vector_index_entries: usize,
    #[serde(default)]
    pub vector_index_dimension: Option<usize>,
    #[serde(default)]
    pub vector_index_estimated_memory_bytes: usize,
    #[serde(default)]
    pub approx_working_set_bytes: usize,
}

pub fn run_benchmark(
    config: BenchmarkConfig,
    runtime_config: RuntimeConfig,
) -> Result<BenchmarkReport> {
    validate_config(&config)?;

    let total_start = Instant::now();
    let benchmark_path = if config.concurrency > 1 {
        Some(unique_benchmark_db_path())
    } else {
        None
    };
    let db = if let Some(path) = benchmark_path.as_ref() {
        SqlRite::open_with_config(path, runtime_config.clone())?
    } else {
        SqlRite::open_in_memory_with_config(runtime_config.clone())?
    };

    let outcome = (|| -> Result<BenchmarkReport> {
        let ingest_start = Instant::now();
        let ingest_stats = ingest_corpus(&db, &config)?;
        let ingest_duration = ingest_start.elapsed();
        let vector_stats = db.vector_index_stats();

        for i in 0..config.warmup_queries {
            let (request, _) = synthetic_query(&config, i + 1_000_000);
            let _ = db.search(request)?;
        }

        let query_start = Instant::now();
        let (latencies_ms, top1_hits) = if config.concurrency > 1 {
            let path = benchmark_path.as_ref().ok_or_else(|| {
                SqlRiteError::InvalidBenchmarkConfig("missing benchmark path".to_string())
            })?;
            run_parallel_query_phase(path, &runtime_config, &config)?
        } else {
            run_single_query_phase(&db, &config)?
        };
        let query_duration = query_start.elapsed();
        let total_duration = total_start.elapsed();

        let latency = summarize_latencies(&latencies_ms);
        let effective_candidate_limit = synthetic_query(
            &config,
            config
                .query_count
                .saturating_add(config.warmup_queries)
                .saturating_add(1),
        )
        .0
        .resolve_query_profile()
        .candidate_limit;
        let qps = if query_duration.as_secs_f64() > 0.0 {
            config.query_count as f64 / query_duration.as_secs_f64()
        } else {
            0.0
        };
        let top1_hit_rate = if config.query_count > 0 {
            top1_hits as f64 / config.query_count as f64
        } else {
            0.0
        };
        let ingest_chunks_per_sec = if ingest_duration.as_secs_f64() > 0.0 {
            config.corpus_size as f64 / ingest_duration.as_secs_f64()
        } else {
            0.0
        };
        let vector_index_entries = vector_stats.as_ref().map_or(0, |stats| stats.entries);
        let vector_index_dimension = vector_stats.as_ref().and_then(|stats| stats.dimension);
        let vector_storage_kind = vector_stats
            .as_ref()
            .map(|stats| stats.storage_kind.clone())
            .unwrap_or_else(|| runtime_config.vector_storage_kind.as_str().to_string());
        let vector_index_estimated_memory_bytes = vector_stats
            .as_ref()
            .map_or(0, |stats| stats.estimated_memory_bytes);
        let dataset_payload_bytes = ingest_stats.embedding_bytes + ingest_stats.content_bytes;
        let approx_working_set_bytes = dataset_payload_bytes + vector_index_estimated_memory_bytes;

        Ok(BenchmarkReport {
            corpus_size: config.corpus_size,
            query_count: config.query_count,
            warmup_queries: config.warmup_queries,
            concurrency: config.concurrency,
            embedding_dim: config.embedding_dim,
            top_k: config.top_k,
            candidate_limit: config.candidate_limit,
            effective_candidate_limit,
            query_profile: match config.query_profile {
                QueryProfile::Balanced => "balanced".to_string(),
                QueryProfile::Latency => "latency".to_string(),
                QueryProfile::Recall => "recall".to_string(),
            },
            alpha: config.alpha,
            fusion_strategy: fusion_label(config.fusion_strategy),
            vector_index_mode: vector_index_mode_label(runtime_config.vector_index_mode),
            vector_storage_kind,
            sqlite_mmap_size_bytes: runtime_config.sqlite_mmap_size_bytes,
            sqlite_cache_size_kib: runtime_config.sqlite_cache_size_kib,
            ingest_duration_ms: ingest_duration.as_secs_f64() * 1000.0,
            query_duration_ms: query_duration.as_secs_f64() * 1000.0,
            total_duration_ms: total_duration.as_secs_f64() * 1000.0,
            qps,
            top1_hit_rate,
            latency,
            ingest_chunks_per_sec,
            embedding_bytes: ingest_stats.embedding_bytes,
            content_bytes: ingest_stats.content_bytes,
            dataset_payload_bytes,
            vector_index_entries,
            vector_index_dimension,
            vector_index_estimated_memory_bytes,
            approx_working_set_bytes,
        })
    })();

    if let Some(path) = benchmark_path.as_ref() {
        cleanup_benchmark_db(path);
    }

    outcome
}

fn run_single_query_phase(db: &SqlRite, config: &BenchmarkConfig) -> Result<(Vec<f64>, usize)> {
    let mut latencies_ms = Vec::with_capacity(config.query_count);
    let mut top1_hits = 0usize;
    for i in 0..config.query_count {
        let (request, expected_top1) = synthetic_query(config, i);
        let started = Instant::now();
        let results = db.search(request)?;
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        latencies_ms.push(elapsed);
        if results.first().map(|item| item.chunk_id.as_str()) == Some(expected_top1.as_str()) {
            top1_hits += 1;
        }
    }
    Ok((latencies_ms, top1_hits))
}

#[derive(Debug)]
struct QueryWorkerStats {
    latencies_ms: Vec<f64>,
    top1_hits: usize,
}

fn run_parallel_query_phase(
    db_path: &Path,
    runtime_config: &RuntimeConfig,
    config: &BenchmarkConfig,
) -> Result<(Vec<f64>, usize)> {
    let worker_outputs = (0..config.concurrency)
        .into_par_iter()
        .map(|worker_idx| -> Result<QueryWorkerStats> {
            let db = SqlRite::open_with_config(db_path, runtime_config.clone())?;
            let mut latencies_ms =
                Vec::with_capacity(config.query_count.div_ceil(config.concurrency).max(1));
            let mut top1_hits = 0usize;

            let mut query_idx = worker_idx;
            while query_idx < config.query_count {
                let (request, expected_top1) = synthetic_query(config, query_idx);
                let started = Instant::now();
                let results = db.search(request)?;
                let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                latencies_ms.push(elapsed);
                if results.first().map(|item| item.chunk_id.as_str())
                    == Some(expected_top1.as_str())
                {
                    top1_hits += 1;
                }
                query_idx += config.concurrency;
            }

            Ok(QueryWorkerStats {
                latencies_ms,
                top1_hits,
            })
        })
        .collect::<Vec<_>>();

    let mut merged_latencies = Vec::with_capacity(config.query_count);
    let mut merged_top1_hits = 0usize;
    for worker in worker_outputs {
        let worker = worker?;
        merged_top1_hits += worker.top1_hits;
        merged_latencies.extend(worker.latencies_ms);
    }
    Ok((merged_latencies, merged_top1_hits))
}

fn unique_benchmark_db_path() -> PathBuf {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "sqlrite_bench_{}_{}.db",
        std::process::id(),
        now_ms
    ))
}

fn cleanup_benchmark_db(path: &Path) {
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(path.with_extension("db-wal"));
    let _ = fs::remove_file(path.with_extension("db-shm"));
}

fn validate_config(config: &BenchmarkConfig) -> Result<()> {
    if config.corpus_size == 0 {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "corpus_size must be at least 1".to_string(),
        ));
    }
    if config.query_count == 0 {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "query_count must be at least 1".to_string(),
        ));
    }
    if config.concurrency == 0 {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "concurrency must be at least 1".to_string(),
        ));
    }
    if config.embedding_dim == 0 {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "embedding_dim must be at least 1".to_string(),
        ));
    }
    if config.top_k == 0 {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "top_k must be at least 1".to_string(),
        ));
    }
    if config.candidate_limit == 0 {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "candidate_limit must be at least 1".to_string(),
        ));
    }
    if config.candidate_limit < config.top_k {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "candidate_limit must be >= top_k".to_string(),
        ));
    }
    if config.batch_size == 0 {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "batch_size must be at least 1".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&config.alpha) {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "alpha must be between 0.0 and 1.0".to_string(),
        ));
    }
    if let FusionStrategy::ReciprocalRankFusion { rank_constant } = config.fusion_strategy
        && rank_constant <= 0.0
    {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "rrf rank_constant must be > 0.0".to_string(),
        ));
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
struct IngestStats {
    embedding_bytes: usize,
    content_bytes: usize,
}

fn ingest_corpus(db: &SqlRite, config: &BenchmarkConfig) -> Result<IngestStats> {
    let mut batch = Vec::with_capacity(config.batch_size);
    let mut stats = IngestStats::default();
    for i in 0..config.corpus_size {
        let chunk = synthetic_chunk(i, config.embedding_dim);
        stats.embedding_bytes += chunk.embedding.len() * std::mem::size_of::<f32>();
        stats.content_bytes += chunk.content.len();
        batch.push(chunk);
        if batch.len() >= config.batch_size {
            db.ingest_chunks(&batch)?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        db.ingest_chunks(&batch)?;
    }
    Ok(stats)
}

fn synthetic_chunk(index: usize, embedding_dim: usize) -> ChunkInput {
    let topic = topic_for(index);
    let words = TOPIC_KEYWORDS[topic];
    ChunkInput {
        id: chunk_id(index),
        doc_id: format!("doc-{:06}", index / 5),
        content: format!(
            "{} {} {} {} synthetic chunk {}",
            words[0], words[1], words[2], words[3], index
        ),
        embedding: topic_embedding(topic, index, embedding_dim),
        metadata: json!({
            "tenant": "bench",
            "topic": format!("topic_{topic}"),
        }),
        source: Some(format!("synthetic/{index}.md")),
    }
}

fn synthetic_query(config: &BenchmarkConfig, query_index: usize) -> (SearchRequest, String) {
    let target_index = (query_index * 9973) % config.corpus_size;
    let topic = topic_for(target_index);
    let words = TOPIC_KEYWORDS[topic];
    let expected_top1 = chunk_id(target_index);
    let query_embedding = perturb_embedding(
        &topic_embedding(topic, target_index, config.embedding_dim),
        query_index as u64 + 1337,
    );

    (
        SearchRequest {
            query_text: Some(format!("{} {} {}", words[0], words[1], words[2])),
            query_embedding: Some(query_embedding),
            top_k: config.top_k,
            alpha: config.alpha,
            candidate_limit: config.candidate_limit.min(config.corpus_size),
            query_profile: config.query_profile,
            metadata_filters: HashMap::new(),
            doc_id: None,
            fusion_strategy: config.fusion_strategy,
        },
        expected_top1,
    )
}

fn topic_for(index: usize) -> usize {
    index % TOPIC_KEYWORDS.len()
}

fn chunk_id(index: usize) -> String {
    format!("chunk-{index:08}")
}

fn topic_embedding(topic: usize, index: usize, dim: usize) -> Vec<f32> {
    let mut vec = vec![0.0; dim];
    for (j, value) in vec.iter_mut().enumerate() {
        let seed = ((index as u64 + 1) << 32) ^ (j as u64 + 1);
        *value = (pseudo_uniform(seed) - 0.5) * 0.08;
    }
    vec[topic % dim] += 1.0;
    normalize_vector(&mut vec);
    vec
}

fn perturb_embedding(base: &[f32], seed_base: u64) -> Vec<f32> {
    let mut out = base.to_vec();
    for (j, value) in out.iter_mut().enumerate() {
        let noise = (pseudo_uniform(seed_base ^ (j as u64 + 1)) - 0.5) * 0.02;
        *value += noise;
    }
    normalize_vector(&mut out);
    out
}

fn normalize_vector(vector: &mut [f32]) {
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in vector {
            *value /= norm;
        }
    }
}

fn pseudo_uniform(seed: u64) -> f32 {
    let mut x = seed.wrapping_add(0x9E3779B97F4A7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^= x >> 31;
    (x as f64 / u64::MAX as f64) as f32
}

fn summarize_latencies(latencies_ms: &[f64]) -> BenchmarkLatency {
    if latencies_ms.is_empty() {
        return BenchmarkLatency {
            avg_ms: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
        };
    }

    let mut sorted = latencies_ms.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let avg_ms = sorted.iter().sum::<f64>() / sorted.len() as f64;
    BenchmarkLatency {
        avg_ms,
        p50_ms: percentile(&sorted, 0.50),
        p95_ms: percentile(&sorted, 0.95),
        p99_ms: percentile(&sorted, 0.99),
        min_ms: *sorted.first().expect("not empty"),
        max_ms: *sorted.last().expect("not empty"),
    }
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = ((sorted.len() - 1) as f64 * pct).round() as usize;
    sorted[rank]
}

fn fusion_label(strategy: FusionStrategy) -> String {
    match strategy {
        FusionStrategy::Weighted => "weighted".to_string(),
        FusionStrategy::ReciprocalRankFusion { rank_constant } => {
            format!("rrf(rank_constant={rank_constant})")
        }
    }
}

fn vector_index_mode_label(mode: crate::VectorIndexMode) -> String {
    match mode {
        crate::VectorIndexMode::Disabled => "disabled".to_string(),
        crate::VectorIndexMode::BruteForce => "brute_force".to_string(),
        crate::VectorIndexMode::LshAnn => "lsh_ann".to_string(),
        crate::VectorIndexMode::HnswBaseline => "hnsw_baseline".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_smoke_test() -> Result<()> {
        let report = run_benchmark(
            BenchmarkConfig {
                corpus_size: 300,
                query_count: 50,
                warmup_queries: 10,
                concurrency: 1,
                embedding_dim: 32,
                top_k: 5,
                candidate_limit: 50,
                query_profile: QueryProfile::Balanced,
                alpha: 0.6,
                fusion_strategy: FusionStrategy::Weighted,
                batch_size: 64,
            },
            RuntimeConfig::default(),
        )?;
        assert_eq!(report.corpus_size, 300);
        assert_eq!(report.query_count, 50);
        assert!(report.qps >= 0.0);
        Ok(())
    }

    #[test]
    fn percentile_computes_expected_value() {
        let sorted = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((percentile(&sorted, 0.50) - 3.0).abs() < 1e-6);
        assert!((percentile(&sorted, 0.95) - 5.0).abs() < 1e-6);
    }
}
