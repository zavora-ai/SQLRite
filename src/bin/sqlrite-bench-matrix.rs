use sqlrite::{
    BenchmarkConfig, BenchmarkReport, DurabilityProfile, FusionStrategy, RuntimeConfig,
    VectorIndexMode, run_benchmark,
};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
struct MatrixScenario {
    name: String,
    runtime: RuntimeConfig,
    config: BenchmarkConfig,
}

#[derive(Debug, Clone, serde::Serialize)]
struct MatrixRun {
    name: String,
    report: BenchmarkReport,
}

#[derive(Debug, Clone, serde::Serialize)]
struct MatrixReport {
    profile: String,
    runs: Vec<MatrixRun>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args =
        parse_args(std::env::args().skip(1).collect::<Vec<_>>()).map_err(std::io::Error::other)?;
    let mut base_config = profile_to_config(&args.profile).map_err(std::io::Error::other)?;
    if let Some(concurrency) = args.concurrency {
        base_config.concurrency = concurrency;
    }

    let scenarios = vec![
        MatrixScenario {
            name: "weighted + brute_force".to_string(),
            runtime: runtime_with_mode(args.durability_profile, VectorIndexMode::BruteForce),
            config: BenchmarkConfig {
                fusion_strategy: FusionStrategy::Weighted,
                ..base_config.clone()
            },
        },
        MatrixScenario {
            name: format!("rrf(k={}) + brute_force", args.rrf_rank_constant),
            runtime: runtime_with_mode(args.durability_profile, VectorIndexMode::BruteForce),
            config: BenchmarkConfig {
                fusion_strategy: FusionStrategy::ReciprocalRankFusion {
                    rank_constant: args.rrf_rank_constant,
                },
                ..base_config.clone()
            },
        },
        MatrixScenario {
            name: "weighted + lsh_ann".to_string(),
            runtime: runtime_with_mode(args.durability_profile, VectorIndexMode::LshAnn),
            config: BenchmarkConfig {
                fusion_strategy: FusionStrategy::Weighted,
                ..base_config.clone()
            },
        },
        MatrixScenario {
            name: "weighted + hnsw_baseline".to_string(),
            runtime: runtime_with_mode(args.durability_profile, VectorIndexMode::HnswBaseline),
            config: BenchmarkConfig {
                fusion_strategy: FusionStrategy::Weighted,
                ..base_config.clone()
            },
        },
        MatrixScenario {
            name: "weighted + disabled_index".to_string(),
            runtime: runtime_with_mode(args.durability_profile, VectorIndexMode::Disabled),
            config: BenchmarkConfig {
                fusion_strategy: FusionStrategy::Weighted,
                ..base_config
            },
        },
    ];

    let mut runs = Vec::with_capacity(scenarios.len());
    for scenario in scenarios {
        eprintln!("running scenario: {}", scenario.name);
        let report = run_benchmark(scenario.config, scenario.runtime)?;
        runs.push(MatrixRun {
            name: scenario.name,
            report,
        });
    }

    print_matrix_summary(&args.profile, &runs);
    let payload = serde_json::to_string_pretty(&MatrixReport {
        profile: args.profile,
        runs,
    })?;
    if let Some(path) = args.output_path {
        fs::write(path, payload)?;
    } else {
        println!("{payload}");
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct MatrixArgs {
    profile: String,
    durability_profile: DurabilityProfile,
    rrf_rank_constant: f32,
    concurrency: Option<usize>,
    output_path: Option<PathBuf>,
}

impl Default for MatrixArgs {
    fn default() -> Self {
        Self {
            profile: "quick".to_string(),
            durability_profile: DurabilityProfile::Balanced,
            rrf_rank_constant: 60.0,
            concurrency: None,
            output_path: None,
        }
    }
}

fn parse_args(args: Vec<String>) -> Result<MatrixArgs, String> {
    let mut cfg = MatrixArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--profile" => {
                i += 1;
                cfg.profile = parse_string(&args, i, "--profile")?;
            }
            "--rrf-k" => {
                i += 1;
                cfg.rrf_rank_constant = parse_f32(&args, i, "--rrf-k")?;
            }
            "--concurrency" => {
                i += 1;
                cfg.concurrency = Some(parse_usize(&args, i, "--concurrency")?);
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
            "--output" => {
                i += 1;
                cfg.output_path = Some(PathBuf::from(parse_string(&args, i, "--output")?));
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

fn parse_f32(args: &[String], index: usize, flag: &str) -> Result<f32, String> {
    let raw = parse_string(args, index, flag)?;
    raw.parse::<f32>()
        .map_err(|_| format!("invalid number for {flag}: `{raw}`\n{}", usage()))
}

fn parse_usize(args: &[String], index: usize, flag: &str) -> Result<usize, String> {
    let raw = parse_string(args, index, flag)?;
    raw.parse::<usize>()
        .map_err(|_| format!("invalid integer for {flag}: `{raw}`\n{}", usage()))
}

fn usage() -> String {
    "usage: cargo run --bin sqlrite-bench-matrix -- [--profile quick|10k|100k|1m|10m] [--concurrency N] [--durability balanced|durable|fast_unsafe] [--rrf-k F] [--output PATH]".to_string()
}

fn profile_to_config(profile: &str) -> Result<BenchmarkConfig, String> {
    let cfg = match profile {
        "quick" => BenchmarkConfig {
            corpus_size: 3_000,
            query_count: 200,
            warmup_queries: 50,
            concurrency: 1,
            embedding_dim: 64,
            top_k: 10,
            candidate_limit: 300,
            alpha: 0.65,
            fusion_strategy: FusionStrategy::Weighted,
            batch_size: 256,
        },
        "10k" => BenchmarkConfig {
            corpus_size: 10_000,
            query_count: 500,
            warmup_queries: 100,
            concurrency: 1,
            embedding_dim: 128,
            top_k: 10,
            candidate_limit: 500,
            alpha: 0.65,
            fusion_strategy: FusionStrategy::Weighted,
            batch_size: 500,
        },
        "100k" => BenchmarkConfig {
            corpus_size: 100_000,
            query_count: 1000,
            warmup_queries: 200,
            concurrency: 1,
            embedding_dim: 256,
            top_k: 10,
            candidate_limit: 1000,
            alpha: 0.65,
            fusion_strategy: FusionStrategy::Weighted,
            batch_size: 1000,
        },
        "1m" => BenchmarkConfig {
            corpus_size: 1_000_000,
            query_count: 2000,
            warmup_queries: 500,
            concurrency: 1,
            embedding_dim: 384,
            top_k: 10,
            candidate_limit: 2000,
            alpha: 0.65,
            fusion_strategy: FusionStrategy::Weighted,
            batch_size: 2000,
        },
        "10m" => BenchmarkConfig {
            corpus_size: 10_000_000,
            query_count: 5000,
            warmup_queries: 1000,
            concurrency: 1,
            embedding_dim: 384,
            top_k: 10,
            candidate_limit: 4000,
            alpha: 0.65,
            fusion_strategy: FusionStrategy::Weighted,
            batch_size: 4000,
        },
        other => {
            return Err(format!(
                "invalid profile `{other}`; expected quick, 10k, 100k, 1m, or 10m"
            ));
        }
    };
    Ok(cfg)
}

fn runtime_with_mode(durability: DurabilityProfile, mode: VectorIndexMode) -> RuntimeConfig {
    let mut runtime = RuntimeConfig::default().with_vector_index_mode(mode);
    runtime.durability_profile = durability;
    runtime
}

fn print_matrix_summary(profile: &str, runs: &[MatrixRun]) {
    println!("SQLRite benchmark matrix profile={profile}");
    println!(
        "{:<28} {:>6} {:>10} {:>10} {:>10} {:>10} {:>10} {:>12} {:>10}",
        "scenario",
        "conc",
        "qps",
        "p50(ms)",
        "p95(ms)",
        "top1",
        "query_ms",
        "ingest_cps",
        "work_mb"
    );
    for run in runs {
        println!(
            "{:<28} {:>6} {:>10.2} {:>10.3} {:>10.3} {:>10.4} {:>10.1} {:>12.1} {:>10.2}",
            run.name,
            run.report.concurrency,
            run.report.qps,
            run.report.latency.p50_ms,
            run.report.latency.p95_ms,
            run.report.top1_hit_rate,
            run.report.query_duration_ms,
            run.report.ingest_chunks_per_sec,
            run.report.approx_working_set_bytes as f64 / (1024.0 * 1024.0)
        );
    }
}
