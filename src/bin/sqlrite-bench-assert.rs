use serde::Deserialize;
use sqlrite::BenchmarkReport;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args(std::env::args().skip(1).collect::<Vec<_>>())?;
    let payload = fs::read_to_string(&args.matrix_path)?;
    let matrix: MatrixReport = serde_json::from_str(&payload)?;

    let scenario_filter: HashSet<String> = args.scenarios.into_iter().collect();
    let selected: Vec<&MatrixRun> = matrix
        .runs
        .iter()
        .filter(|run| scenario_filter.is_empty() || scenario_filter.contains(run.name.as_str()))
        .collect();

    if selected.is_empty() {
        return Err(std::io::Error::other(format!(
            "no matching scenarios found in {}",
            args.matrix_path.display()
        ))
        .into());
    }

    let mut failures = Vec::new();
    for run in &selected {
        if let Some(min_qps) = args.min_qps
            && run.report.qps < min_qps
        {
            failures.push(format!(
                "{} qps {:.2} < min_qps {:.2}",
                run.name, run.report.qps, min_qps
            ));
        }
        if let Some(max_p95_ms) = args.max_p95_ms
            && run.report.latency.p95_ms > max_p95_ms
        {
            failures.push(format!(
                "{} p95 {:.3}ms > max_p95 {:.3}ms",
                run.name, run.report.latency.p95_ms, max_p95_ms
            ));
        }
        if let Some(min_top1) = args.min_top1
            && run.report.top1_hit_rate < min_top1
        {
            failures.push(format!(
                "{} top1 {:.4} < min_top1 {:.4}",
                run.name, run.report.top1_hit_rate, min_top1
            ));
        }
        if let Some(max_query_ms) = args.max_query_ms
            && run.report.query_duration_ms > max_query_ms
        {
            failures.push(format!(
                "{} query_ms {:.1} > max_query_ms {:.1}",
                run.name, run.report.query_duration_ms, max_query_ms
            ));
        }
        if let Some(min_ingest_chunks_per_sec) = args.min_ingest_chunks_per_sec
            && run.report.ingest_chunks_per_sec < min_ingest_chunks_per_sec
        {
            failures.push(format!(
                "{} ingest_chunks_per_sec {:.2} < min_ingest_cps {:.2}",
                run.name, run.report.ingest_chunks_per_sec, min_ingest_chunks_per_sec
            ));
        }
        if let Some(max_working_set_bytes) = args.max_working_set_bytes
            && run.report.approx_working_set_bytes as u64 > max_working_set_bytes
        {
            failures.push(format!(
                "{} approx_working_set_bytes {} > max_working_set_bytes {}",
                run.name, run.report.approx_working_set_bytes, max_working_set_bytes
            ));
        }
    }

    if failures.is_empty() {
        println!(
            "benchmark assertions passed: profile={}, checked={} scenario(s)",
            matrix.profile,
            selected.len()
        );
        return Ok(());
    }

    eprintln!("benchmark assertion failures:");
    for failure in &failures {
        eprintln!("- {failure}");
    }
    Err(std::io::Error::other(format!("{} assertion(s) failed", failures.len())).into())
}

#[derive(Debug, Deserialize)]
struct MatrixReport {
    profile: String,
    runs: Vec<MatrixRun>,
}

#[derive(Debug, Deserialize)]
struct MatrixRun {
    name: String,
    report: BenchmarkReport,
}

#[derive(Debug, Clone)]
struct BenchAssertArgs {
    matrix_path: PathBuf,
    scenarios: Vec<String>,
    min_qps: Option<f64>,
    max_p95_ms: Option<f64>,
    min_top1: Option<f64>,
    max_query_ms: Option<f64>,
    min_ingest_chunks_per_sec: Option<f64>,
    max_working_set_bytes: Option<u64>,
}

fn parse_args(args: Vec<String>) -> Result<BenchAssertArgs, String> {
    let mut matrix_path = None;
    let mut scenarios = Vec::new();
    let mut min_qps = None;
    let mut max_p95_ms = None;
    let mut min_top1 = None;
    let mut max_query_ms = None;
    let mut min_ingest_chunks_per_sec = None;
    let mut max_working_set_bytes = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--matrix" => {
                i += 1;
                matrix_path = Some(PathBuf::from(parse_string(&args, i, "--matrix")?));
            }
            "--scenario" => {
                i += 1;
                scenarios.push(parse_string(&args, i, "--scenario")?);
            }
            "--min-qps" => {
                i += 1;
                min_qps = Some(parse_f64(&args, i, "--min-qps")?);
            }
            "--max-p95-ms" => {
                i += 1;
                max_p95_ms = Some(parse_f64(&args, i, "--max-p95-ms")?);
            }
            "--min-top1" => {
                i += 1;
                min_top1 = Some(parse_f64(&args, i, "--min-top1")?);
            }
            "--max-query-ms" => {
                i += 1;
                max_query_ms = Some(parse_f64(&args, i, "--max-query-ms")?);
            }
            "--min-ingest-cps" => {
                i += 1;
                min_ingest_chunks_per_sec = Some(parse_f64(&args, i, "--min-ingest-cps")?);
            }
            "--max-working-set-bytes" => {
                i += 1;
                max_working_set_bytes = Some(parse_u64(&args, i, "--max-working-set-bytes")?);
            }
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown argument `{other}`\n{}", usage())),
        }
        i += 1;
    }

    let matrix_path =
        matrix_path.ok_or_else(|| format!("missing required --matrix <path>\n{}", usage()))?;

    Ok(BenchAssertArgs {
        matrix_path,
        scenarios,
        min_qps,
        max_p95_ms,
        min_top1,
        max_query_ms,
        min_ingest_chunks_per_sec,
        max_working_set_bytes,
    })
}

fn parse_string(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}\n{}", usage()))
}

fn parse_f64(args: &[String], index: usize, flag: &str) -> Result<f64, String> {
    let raw = parse_string(args, index, flag)?;
    raw.parse::<f64>()
        .map_err(|_| format!("invalid number for {flag}: `{raw}`\n{}", usage()))
}

fn parse_u64(args: &[String], index: usize, flag: &str) -> Result<u64, String> {
    let raw = parse_string(args, index, flag)?;
    raw.parse::<u64>()
        .map_err(|_| format!("invalid integer for {flag}: `{raw}`\n{}", usage()))
}

fn usage() -> String {
    "usage: cargo run --bin sqlrite-bench-assert -- --matrix <path> [--scenario <name>] [--min-qps F] [--max-p95-ms F] [--min-top1 F] [--max-query-ms F] [--min-ingest-cps F] [--max-working-set-bytes N]".to_string()
}
