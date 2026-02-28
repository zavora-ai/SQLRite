use sqlrite::{DurabilityProfile, EvalDataset, RuntimeConfig, VectorIndexMode, evaluate_dataset};
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args(std::env::args().skip(1).collect::<Vec<_>>())?;
    let data = fs::read_to_string(&args.dataset_path)?;
    let dataset: EvalDataset = serde_json::from_str(&data)?;

    let mut runtime = RuntimeConfig::default().with_vector_index_mode(args.index_mode);
    runtime.durability_profile = args.durability_profile;

    let report = evaluate_dataset(dataset, runtime)?;
    print_human_summary(&report);

    let serialized = serde_json::to_string_pretty(&report)?;
    if let Some(path) = args.output_path {
        fs::write(path, serialized)?;
    } else {
        println!("{serialized}");
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct EvalCliArgs {
    dataset_path: PathBuf,
    output_path: Option<PathBuf>,
    index_mode: VectorIndexMode,
    durability_profile: DurabilityProfile,
}

fn parse_args(args: Vec<String>) -> Result<EvalCliArgs, String> {
    let mut dataset_path = None;
    let mut output_path = None;
    let mut index_mode = VectorIndexMode::BruteForce;
    let mut durability_profile = DurabilityProfile::Balanced;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dataset" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| {
                    "missing value for --dataset\nusage: cargo run --bin sqlrite-eval -- --dataset <path> [--output <path>] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--durability balanced|durable|fast_unsafe]".to_string()
                })?;
                dataset_path = Some(PathBuf::from(value));
            }
            "--output" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| {
                    "missing value for --output\nusage: cargo run --bin sqlrite-eval -- --dataset <path> [--output <path>] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--durability balanced|durable|fast_unsafe]".to_string()
                })?;
                output_path = Some(PathBuf::from(value));
            }
            "--index-mode" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| {
                    "missing value for --index-mode\nusage: cargo run --bin sqlrite-eval -- --dataset <path> [--output <path>] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--durability balanced|durable|fast_unsafe]".to_string()
                })?;
                index_mode = match value.as_str() {
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
                let value = args.get(i).ok_or_else(|| {
                    "missing value for --durability\nusage: cargo run --bin sqlrite-eval -- --dataset <path> [--output <path>] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--durability balanced|durable|fast_unsafe]".to_string()
                })?;
                durability_profile = match value.as_str() {
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
            "--help" | "-h" => {
                return Err("usage: cargo run --bin sqlrite-eval -- --dataset <path> [--output <path>] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--durability balanced|durable|fast_unsafe]".to_string());
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`\nusage: cargo run --bin sqlrite-eval -- --dataset <path> [--output <path>] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--durability balanced|durable|fast_unsafe]"
                ));
            }
        }
        i += 1;
    }

    let dataset_path = dataset_path.ok_or_else(|| {
        "missing required --dataset <path>\nusage: cargo run --bin sqlrite-eval -- --dataset <path> [--output <path>] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--durability balanced|durable|fast_unsafe]".to_string()
    })?;

    Ok(EvalCliArgs {
        dataset_path,
        output_path,
        index_mode,
        durability_profile,
    })
}

fn print_human_summary(report: &sqlrite::EvalReport) {
    println!(
        "SQLRite eval summary: corpus={}, queries={}, ks={:?}",
        report.summary.corpus_size, report.summary.query_count, report.summary.k_values
    );
    for k in &report.summary.k_values {
        if let Some(metrics) = report.aggregate_metrics_at_k.get(k) {
            println!(
                "k={k}: recall={:.4}, precision={:.4}, mrr={:.4}, ndcg={:.4}, hit_rate={:.4}",
                metrics.recall, metrics.precision, metrics.mrr, metrics.ndcg, metrics.hit_rate
            );
        }
    }
}
