use serde::Serialize;
use sqlrite::{
    BenchmarkConfig, BenchmarkReport, DurabilityProfile, EvalDataset, EvalReport, FusionStrategy,
    RuntimeConfig, VectorIndexMode, evaluate_dataset, run_benchmark,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SUITE_VERSION: &str = "s12-v1";

#[derive(Debug, Clone)]
struct BenchSuiteArgs {
    profiles: Vec<String>,
    scenarios: Vec<String>,
    concurrency_profile: String,
    concurrency_levels: Vec<usize>,
    dataset_path: PathBuf,
    dataset_id: String,
    embedding_model: String,
    hardware_class: String,
    durability_profile: DurabilityProfile,
    rrf_rank_constant: f32,
    eval_index_modes: Vec<VectorIndexMode>,
    skip_eval: bool,
    output_path: Option<PathBuf>,
}

impl Default for BenchSuiteArgs {
    fn default() -> Self {
        Self {
            profiles: vec!["10k".to_string()],
            scenarios: Vec::new(),
            concurrency_profile: "10k".to_string(),
            concurrency_levels: vec![1, 2, 4],
            dataset_path: PathBuf::from("examples/eval_dataset.json"),
            dataset_id: "examples/eval_dataset.json".to_string(),
            embedding_model: "deterministic-local-v1".to_string(),
            hardware_class: "unspecified".to_string(),
            durability_profile: DurabilityProfile::Balanced,
            rrf_rank_constant: 60.0,
            eval_index_modes: vec![
                VectorIndexMode::BruteForce,
                VectorIndexMode::LshAnn,
                VectorIndexMode::HnswBaseline,
            ],
            skip_eval: false,
            output_path: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct BenchSuiteMetadata {
    suite_version: String,
    generated_at_unix_seconds: u64,
    embedding_model: String,
    dataset_id: String,
    dataset_path: String,
    hardware_class: String,
    host_os: String,
    host_arch: String,
    host_cpu_threads: usize,
}

#[derive(Debug, Serialize)]
struct MatrixRun {
    name: String,
    report: BenchmarkReport,
}

#[derive(Debug, Serialize)]
struct ProfileMatrixReport {
    profile: String,
    runs: Vec<MatrixRun>,
}

#[derive(Debug, Serialize)]
struct ConcurrencyRun {
    concurrency: usize,
    report: BenchmarkReport,
}

#[derive(Debug, Serialize)]
struct ConcurrencySweepReport {
    profile: String,
    scenario: String,
    runs: Vec<ConcurrencyRun>,
}

#[derive(Debug, Serialize)]
struct EvalRun {
    index_mode: String,
    report: EvalReport,
}

#[derive(Debug, Serialize)]
struct BenchSuiteReport {
    metadata: BenchSuiteMetadata,
    benchmark_profiles: Vec<ProfileMatrixReport>,
    concurrency_sweep: ConcurrencySweepReport,
    eval_runs: Vec<EvalRun>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args =
        parse_args(std::env::args().skip(1).collect::<Vec<_>>()).map_err(std::io::Error::other)?;
    let report = run_suite(&args)?;
    print_human_summary(&report);

    let payload = serde_json::to_string_pretty(&report)?;
    if let Some(path) = args.output_path {
        fs::write(path, payload)?;
    } else {
        println!("{payload}");
    }

    Ok(())
}

fn run_suite(args: &BenchSuiteArgs) -> Result<BenchSuiteReport, Box<dyn std::error::Error>> {
    let benchmark_profiles = run_profile_matrices(args)?;
    let concurrency_sweep = run_concurrency_sweep(args)?;
    let eval_runs = if args.skip_eval {
        Vec::new()
    } else {
        run_eval_modes(args)?
    };

    let dataset_path = args
        .dataset_path
        .canonicalize()
        .unwrap_or_else(|_| args.dataset_path.clone())
        .display()
        .to_string();
    let metadata = BenchSuiteMetadata {
        suite_version: SUITE_VERSION.to_string(),
        generated_at_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        embedding_model: args.embedding_model.clone(),
        dataset_id: args.dataset_id.clone(),
        dataset_path,
        hardware_class: args.hardware_class.clone(),
        host_os: std::env::consts::OS.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        host_cpu_threads: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
    };

    Ok(BenchSuiteReport {
        metadata,
        benchmark_profiles,
        concurrency_sweep,
        eval_runs,
    })
}

fn run_profile_matrices(
    args: &BenchSuiteArgs,
) -> Result<Vec<ProfileMatrixReport>, Box<dyn std::error::Error>> {
    let mut out = Vec::with_capacity(args.profiles.len());
    for profile in &args.profiles {
        eprintln!("running benchmark matrix profile={profile}");
        let base = profile_to_config(profile).map_err(std::io::Error::other)?;
        let mut scenarios = vec![
            (
                "weighted + brute_force".to_string(),
                runtime_with_mode(args.durability_profile, VectorIndexMode::BruteForce),
                BenchmarkConfig {
                    fusion_strategy: FusionStrategy::Weighted,
                    ..base.clone()
                },
            ),
            (
                format!("rrf(k={}) + brute_force", args.rrf_rank_constant),
                runtime_with_mode(args.durability_profile, VectorIndexMode::BruteForce),
                BenchmarkConfig {
                    fusion_strategy: FusionStrategy::ReciprocalRankFusion {
                        rank_constant: args.rrf_rank_constant,
                    },
                    ..base.clone()
                },
            ),
            (
                "weighted + lsh_ann".to_string(),
                runtime_with_mode(args.durability_profile, VectorIndexMode::LshAnn),
                BenchmarkConfig {
                    fusion_strategy: FusionStrategy::Weighted,
                    ..base.clone()
                },
            ),
            (
                "weighted + hnsw_baseline".to_string(),
                runtime_with_mode(args.durability_profile, VectorIndexMode::HnswBaseline),
                BenchmarkConfig {
                    fusion_strategy: FusionStrategy::Weighted,
                    ..base.clone()
                },
            ),
            (
                "weighted + disabled_index".to_string(),
                runtime_with_mode(args.durability_profile, VectorIndexMode::Disabled),
                BenchmarkConfig {
                    fusion_strategy: FusionStrategy::Weighted,
                    ..base.clone()
                },
            ),
        ];
        if !args.scenarios.is_empty() {
            scenarios.retain(|(name, _, _)| args.scenarios.contains(name));
        }
        if scenarios.is_empty() {
            return Err(std::io::Error::other(format!(
                "no benchmark scenarios selected for profile `{profile}`"
            ))
            .into());
        }

        let mut runs = Vec::with_capacity(scenarios.len());
        for (name, runtime, config) in scenarios {
            eprintln!("  scenario={name}");
            let report = run_benchmark(config, runtime)?;
            runs.push(MatrixRun { name, report });
        }
        out.push(ProfileMatrixReport {
            profile: profile.clone(),
            runs,
        });
    }
    Ok(out)
}

fn run_concurrency_sweep(
    args: &BenchSuiteArgs,
) -> Result<ConcurrencySweepReport, Box<dyn std::error::Error>> {
    let base = profile_to_config(&args.concurrency_profile).map_err(std::io::Error::other)?;
    let runtime = runtime_with_mode(args.durability_profile, VectorIndexMode::BruteForce);
    let mut runs = Vec::with_capacity(args.concurrency_levels.len());

    for &concurrency in &args.concurrency_levels {
        eprintln!(
            "running concurrency sweep profile={} concurrency={}",
            args.concurrency_profile, concurrency
        );
        let mut config = base.clone();
        config.concurrency = concurrency;
        config.fusion_strategy = FusionStrategy::Weighted;
        let report = run_benchmark(config, runtime.clone())?;
        runs.push(ConcurrencyRun {
            concurrency,
            report,
        });
    }

    Ok(ConcurrencySweepReport {
        profile: args.concurrency_profile.clone(),
        scenario: "weighted + brute_force".to_string(),
        runs,
    })
}

fn run_eval_modes(args: &BenchSuiteArgs) -> Result<Vec<EvalRun>, Box<dyn std::error::Error>> {
    let data = fs::read_to_string(&args.dataset_path)?;
    let dataset: EvalDataset = serde_json::from_str(&data)?;
    let mut runs = Vec::with_capacity(args.eval_index_modes.len());

    for &mode in &args.eval_index_modes {
        eprintln!("running eval index_mode={}", index_mode_name(mode));
        let runtime = runtime_with_mode(args.durability_profile, mode);
        let report = evaluate_dataset(dataset.clone(), runtime)?;
        runs.push(EvalRun {
            index_mode: index_mode_name(mode).to_string(),
            report,
        });
    }

    Ok(runs)
}

fn parse_args(args: Vec<String>) -> Result<BenchSuiteArgs, String> {
    let mut cfg = BenchSuiteArgs::default();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--profiles" => {
                i += 1;
                cfg.profiles = parse_csv_strings(&args, i, "--profiles")?;
            }
            "--scenarios" => {
                i += 1;
                cfg.scenarios = parse_csv_strings(&args, i, "--scenarios")?;
            }
            "--concurrency-profile" => {
                i += 1;
                cfg.concurrency_profile = parse_string(&args, i, "--concurrency-profile")?;
            }
            "--concurrency-levels" => {
                i += 1;
                cfg.concurrency_levels = parse_csv_usize(&args, i, "--concurrency-levels")?;
            }
            "--dataset" => {
                i += 1;
                cfg.dataset_path = PathBuf::from(parse_string(&args, i, "--dataset")?);
            }
            "--dataset-id" => {
                i += 1;
                cfg.dataset_id = parse_string(&args, i, "--dataset-id")?;
            }
            "--embedding-model" => {
                i += 1;
                cfg.embedding_model = parse_string(&args, i, "--embedding-model")?;
            }
            "--hardware-class" => {
                i += 1;
                cfg.hardware_class = parse_string(&args, i, "--hardware-class")?;
            }
            "--durability" => {
                i += 1;
                cfg.durability_profile =
                    parse_durability(&parse_string(&args, i, "--durability")?)?;
            }
            "--rrf-k" => {
                i += 1;
                cfg.rrf_rank_constant = parse_f32(&args, i, "--rrf-k")?;
            }
            "--eval-index-modes" => {
                i += 1;
                cfg.eval_index_modes = parse_csv_index_modes(&args, i, "--eval-index-modes")?;
            }
            "--skip-eval" => {
                cfg.skip_eval = true;
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

    if cfg.profiles.is_empty() {
        return Err(format!(
            "--profiles must contain at least one profile\n{}",
            usage()
        ));
    }
    for profile in &cfg.profiles {
        let _ = profile_to_config(profile)?;
    }
    let _ = profile_to_config(&cfg.concurrency_profile)?;
    if cfg.concurrency_levels.is_empty() {
        return Err(format!(
            "--concurrency-levels must contain at least one positive integer\n{}",
            usage()
        ));
    }
    if cfg.rrf_rank_constant <= 0.0 {
        return Err(format!("--rrf-k must be > 0.0\n{}", usage()));
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

fn parse_csv_strings(args: &[String], index: usize, flag: &str) -> Result<Vec<String>, String> {
    let raw = parse_string(args, index, flag)?;
    let values: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect();
    if values.is_empty() {
        return Err(format!("invalid csv list for {flag}: `{raw}`\n{}", usage()));
    }
    Ok(values)
}

fn parse_csv_usize(args: &[String], index: usize, flag: &str) -> Result<Vec<usize>, String> {
    let raw = parse_string(args, index, flag)?;
    let mut values = Vec::new();
    for part in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let parsed = part
            .parse::<usize>()
            .map_err(|_| format!("invalid integer in {flag}: `{part}`\n{}", usage()))?;
        if parsed == 0 {
            return Err(format!(
                "invalid value in {flag}: `{part}` (must be >= 1)\n{}",
                usage()
            ));
        }
        values.push(parsed);
    }
    if values.is_empty() {
        return Err(format!("invalid csv list for {flag}: `{raw}`\n{}", usage()));
    }
    Ok(values)
}

fn parse_csv_index_modes(
    args: &[String],
    index: usize,
    flag: &str,
) -> Result<Vec<VectorIndexMode>, String> {
    let raw = parse_string(args, index, flag)?;
    let mut values = Vec::new();
    for part in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        values.push(parse_index_mode(part)?);
    }
    if values.is_empty() {
        return Err(format!("invalid csv list for {flag}: `{raw}`\n{}", usage()));
    }
    Ok(values)
}

fn parse_durability(value: &str) -> Result<DurabilityProfile, String> {
    match value {
        "balanced" => Ok(DurabilityProfile::Balanced),
        "durable" => Ok(DurabilityProfile::Durable),
        "fast_unsafe" => Ok(DurabilityProfile::FastUnsafe),
        other => Err(format!(
            "invalid --durability `{other}`; expected balanced, durable, or fast_unsafe"
        )),
    }
}

fn parse_index_mode(value: &str) -> Result<VectorIndexMode, String> {
    match value {
        "brute_force" => Ok(VectorIndexMode::BruteForce),
        "lsh_ann" => Ok(VectorIndexMode::LshAnn),
        "hnsw_baseline" | "hnsw" => Ok(VectorIndexMode::HnswBaseline),
        "disabled" => Ok(VectorIndexMode::Disabled),
        other => Err(format!(
            "invalid index mode `{other}`; expected brute_force, lsh_ann, hnsw_baseline, or disabled"
        )),
    }
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
            candidate_limit: 1800,
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
            candidate_limit: 2400,
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

fn index_mode_name(mode: VectorIndexMode) -> &'static str {
    match mode {
        VectorIndexMode::BruteForce => "brute_force",
        VectorIndexMode::LshAnn => "lsh_ann",
        VectorIndexMode::HnswBaseline => "hnsw_baseline",
        VectorIndexMode::Disabled => "disabled",
    }
}

fn usage() -> String {
    "usage: cargo run --bin sqlrite-bench-suite -- [--profiles quick|10k|100k|1m|10m,...] [--scenarios \"weighted + lsh_ann,weighted + hnsw_baseline,...\"] [--concurrency-profile quick|10k|100k|1m|10m] [--concurrency-levels N,N,...] [--dataset PATH] [--dataset-id ID] [--embedding-model NAME] [--hardware-class NAME] [--durability balanced|durable|fast_unsafe] [--rrf-k F] [--eval-index-modes brute_force|lsh_ann|hnsw_baseline|disabled,...] [--skip-eval] [--output PATH]".to_string()
}

fn print_human_summary(report: &BenchSuiteReport) {
    println!(
        "SQLRite benchmark suite: version={}, host={} {}, cpu_threads={}",
        report.metadata.suite_version,
        report.metadata.host_os,
        report.metadata.host_arch,
        report.metadata.host_cpu_threads
    );
    println!(
        "metadata: dataset_id={}, embedding_model={}, hardware_class={}",
        report.metadata.dataset_id, report.metadata.embedding_model, report.metadata.hardware_class
    );
    for matrix in &report.benchmark_profiles {
        println!("profile={}", matrix.profile);
        for run in &matrix.runs {
            println!(
                "  {:<28} qps={:>8.2} p95_ms={:>8.3} top1={:>6.4} conc={}",
                run.name,
                run.report.qps,
                run.report.latency.p95_ms,
                run.report.top1_hit_rate,
                run.report.concurrency
            );
        }
    }
    println!(
        "concurrency_sweep profile={} scenario={}",
        report.concurrency_sweep.profile, report.concurrency_sweep.scenario
    );
    for run in &report.concurrency_sweep.runs {
        println!(
            "  concurrency={} qps={:.2} p95_ms={:.3}",
            run.concurrency, run.report.qps, run.report.latency.p95_ms
        );
    }
    if report.eval_runs.is_empty() {
        println!("eval: skipped");
    } else {
        for eval in &report.eval_runs {
            if let Some(k1) = eval.report.aggregate_metrics_at_k.get(&1usize) {
                println!(
                    "eval mode={} k=1 recall={:.4} mrr={:.4} ndcg={:.4}",
                    eval.index_mode, k1.recall, k1.mrr, k1.ndcg
                );
            } else {
                println!("eval mode={} (no k=1 metrics)", eval.index_mode);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_profile_and_concurrency_lists() -> Result<(), Box<dyn std::error::Error>>
    {
        let args = vec![
            "--profiles".to_string(),
            "quick,10k".to_string(),
            "--concurrency-profile".to_string(),
            "quick".to_string(),
            "--concurrency-levels".to_string(),
            "1,2,8".to_string(),
            "--eval-index-modes".to_string(),
            "brute_force,lsh_ann".to_string(),
            "--dataset".to_string(),
            "examples/eval_dataset.json".to_string(),
        ];
        let parsed = parse_args(args).map_err(std::io::Error::other)?;
        assert_eq!(parsed.profiles, vec!["quick", "10k"]);
        assert_eq!(parsed.concurrency_profile, "quick");
        assert_eq!(parsed.concurrency_levels, vec![1, 2, 8]);
        assert_eq!(parsed.eval_index_modes.len(), 2);
        Ok(())
    }

    #[test]
    fn parse_args_rejects_zero_concurrency() {
        let args = vec![
            "--concurrency-levels".to_string(),
            "0,2".to_string(),
            "--dataset".to_string(),
            "examples/eval_dataset.json".to_string(),
        ];
        assert!(parse_args(args).is_err());
    }
}
