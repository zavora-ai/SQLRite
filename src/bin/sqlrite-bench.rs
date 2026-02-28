use sqlrite::{
    BenchmarkConfig, DurabilityProfile, FusionStrategy, RuntimeConfig, VectorIndexMode,
    run_benchmark,
};
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args =
        parse_args(std::env::args().skip(1).collect::<Vec<_>>()).map_err(std::io::Error::other)?;

    let fusion_strategy = match args.fusion_mode.as_str() {
        "weighted" => FusionStrategy::Weighted,
        "rrf" => FusionStrategy::ReciprocalRankFusion {
            rank_constant: args.rrf_rank_constant,
        },
        other => {
            return Err(std::io::Error::other(format!(
                "invalid fusion mode `{other}`; expected weighted or rrf"
            ))
            .into());
        }
    };

    let config = BenchmarkConfig {
        corpus_size: args.corpus_size,
        query_count: args.query_count,
        warmup_queries: args.warmup_queries,
        embedding_dim: args.embedding_dim,
        top_k: args.top_k,
        candidate_limit: args.candidate_limit,
        alpha: args.alpha,
        fusion_strategy,
        batch_size: args.batch_size,
    };

    let mut runtime = RuntimeConfig::default().with_vector_index_mode(args.index_mode);
    runtime.durability_profile = args.durability_profile;

    let report = run_benchmark(config, runtime)?;
    print_summary(&report);

    let payload = serde_json::to_string_pretty(&report)?;
    if let Some(path) = args.output_path {
        fs::write(path, payload)?;
    } else {
        println!("{payload}");
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct BenchCliArgs {
    corpus_size: usize,
    query_count: usize,
    warmup_queries: usize,
    embedding_dim: usize,
    top_k: usize,
    candidate_limit: usize,
    alpha: f32,
    batch_size: usize,
    fusion_mode: String,
    rrf_rank_constant: f32,
    output_path: Option<PathBuf>,
    index_mode: VectorIndexMode,
    durability_profile: DurabilityProfile,
}

impl Default for BenchCliArgs {
    fn default() -> Self {
        Self {
            corpus_size: 10_000,
            query_count: 500,
            warmup_queries: 100,
            embedding_dim: 128,
            top_k: 10,
            candidate_limit: 500,
            alpha: 0.65,
            batch_size: 500,
            fusion_mode: "weighted".to_string(),
            rrf_rank_constant: 60.0,
            output_path: None,
            index_mode: VectorIndexMode::BruteForce,
            durability_profile: DurabilityProfile::Balanced,
        }
    }
}

fn parse_args(args: Vec<String>) -> Result<BenchCliArgs, String> {
    let mut cfg = BenchCliArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--corpus" => {
                i += 1;
                cfg.corpus_size = parse_usize(&args, i, "--corpus")?;
            }
            "--queries" => {
                i += 1;
                cfg.query_count = parse_usize(&args, i, "--queries")?;
            }
            "--warmup" => {
                i += 1;
                cfg.warmup_queries = parse_usize(&args, i, "--warmup")?;
            }
            "--embedding-dim" => {
                i += 1;
                cfg.embedding_dim = parse_usize(&args, i, "--embedding-dim")?;
            }
            "--top-k" => {
                i += 1;
                cfg.top_k = parse_usize(&args, i, "--top-k")?;
            }
            "--candidate-limit" => {
                i += 1;
                cfg.candidate_limit = parse_usize(&args, i, "--candidate-limit")?;
            }
            "--batch-size" => {
                i += 1;
                cfg.batch_size = parse_usize(&args, i, "--batch-size")?;
            }
            "--alpha" => {
                i += 1;
                cfg.alpha = parse_f32(&args, i, "--alpha")?;
            }
            "--fusion" => {
                i += 1;
                cfg.fusion_mode = parse_string(&args, i, "--fusion")?;
            }
            "--rrf-k" => {
                i += 1;
                cfg.rrf_rank_constant = parse_f32(&args, i, "--rrf-k")?;
            }
            "--output" => {
                i += 1;
                cfg.output_path = Some(PathBuf::from(parse_string(&args, i, "--output")?));
            }
            "--index-mode" => {
                i += 1;
                let value = parse_string(&args, i, "--index-mode")?;
                cfg.index_mode = match value.as_str() {
                    "brute_force" => VectorIndexMode::BruteForce,
                    "disabled" => VectorIndexMode::Disabled,
                    "lsh_ann" => VectorIndexMode::LshAnn,
                    "hnsw_baseline" | "hnsw" => VectorIndexMode::HnswBaseline,
                    other => {
                        return Err(format!(
                            "invalid --index-mode `{other}`; expected brute_force, lsh_ann, hnsw_baseline, or disabled"
                        ));
                    }
                };
            }
            "--durability" => {
                i += 1;
                let value = parse_string(&args, i, "--durability")?;
                cfg.durability_profile = match value.as_str() {
                    "balanced" => DurabilityProfile::Balanced,
                    "durable" => DurabilityProfile::Durable,
                    "fast_unsafe" => DurabilityProfile::FastUnsafe,
                    other => {
                        return Err(format!(
                            "invalid --durability `{other}`; expected balanced, durable, or fast_unsafe"
                        ));
                    }
                };
            }
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown argument `{other}`\n{}", usage())),
        }
        i += 1;
    }

    Ok(cfg)
}

fn parse_string(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}\n{}", usage()))
}

fn parse_usize(args: &[String], index: usize, flag: &str) -> Result<usize, String> {
    let raw = parse_string(args, index, flag)?;
    raw.parse::<usize>()
        .map_err(|_| format!("invalid integer for {flag}: `{raw}`\n{}", usage()))
}

fn parse_f32(args: &[String], index: usize, flag: &str) -> Result<f32, String> {
    let raw = parse_string(args, index, flag)?;
    raw.parse::<f32>()
        .map_err(|_| format!("invalid number for {flag}: `{raw}`\n{}", usage()))
}

fn usage() -> String {
    "usage: cargo run --bin sqlrite-bench -- [--corpus N] [--queries N] [--warmup N] [--embedding-dim N] [--top-k N] [--candidate-limit N] [--batch-size N] [--alpha F] [--fusion weighted|rrf] [--rrf-k F] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--durability balanced|durable|fast_unsafe] [--output PATH]".to_string()
}

fn print_summary(report: &sqlrite::BenchmarkReport) {
    println!(
        "SQLRite benchmark: corpus={}, queries={}, index={}, fusion={}",
        report.corpus_size, report.query_count, report.vector_index_mode, report.fusion_strategy
    );
    println!(
        "ingest_ms={:.2}, query_ms={:.2}, qps={:.2}, top1_hit_rate={:.4}",
        report.ingest_duration_ms, report.query_duration_ms, report.qps, report.top1_hit_rate
    );
    println!(
        "ingest_chunks_per_sec={:.2}, dataset_payload_bytes={}, index_estimated_bytes={}, approx_working_set_bytes={}",
        report.ingest_chunks_per_sec,
        report.dataset_payload_bytes,
        report.vector_index_estimated_memory_bytes,
        report.approx_working_set_bytes
    );
    println!(
        "latency_ms: avg={:.4}, p50={:.4}, p95={:.4}, p99={:.4}, min={:.4}, max={:.4}",
        report.latency.avg_ms,
        report.latency.p50_ms,
        report.latency.p95_ms,
        report.latency.p99_ms,
        report.latency.min_ms,
        report.latency.max_ms
    );
}
