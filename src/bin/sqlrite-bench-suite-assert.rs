use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
struct BenchSuiteAssertArgs {
    suite_path: PathBuf,
    rules: Vec<BenchmarkRule>,
    eval_rules: Vec<EvalRule>,
}

#[derive(Debug, Clone)]
struct BenchmarkRule {
    profile: String,
    scenario: String,
    min_qps: Option<f64>,
    max_p95_ms: Option<f64>,
    min_top1: Option<f64>,
    min_ingest_cpm: Option<f64>,
}

#[derive(Debug, Clone)]
struct EvalRule {
    index_mode: String,
    min_recall_k1: Option<f64>,
    min_mrr_k1: Option<f64>,
    min_ndcg_k1: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct BenchSuiteReport {
    benchmark_profiles: Vec<ProfileMatrixReport>,
    #[allow(dead_code)]
    concurrency_sweep: serde_json::Value,
    eval_runs: Vec<EvalRun>,
}

#[derive(Debug, Deserialize)]
struct ProfileMatrixReport {
    profile: String,
    runs: Vec<MatrixRun>,
}

#[derive(Debug, Deserialize)]
struct MatrixRun {
    name: String,
    report: BenchmarkReport,
}

#[derive(Debug, Deserialize)]
struct BenchmarkReport {
    qps: f64,
    top1_hit_rate: f64,
    ingest_chunks_per_sec: f64,
    latency: BenchmarkLatency,
}

#[derive(Debug, Deserialize)]
struct BenchmarkLatency {
    p95_ms: f64,
}

#[derive(Debug, Deserialize)]
struct EvalRun {
    index_mode: String,
    report: EvalReport,
}

#[derive(Debug, Deserialize)]
struct EvalReport {
    aggregate_metrics_at_k: HashMap<String, EvalMetricsAtK>,
}

#[derive(Debug, Deserialize)]
struct EvalMetricsAtK {
    recall: f64,
    mrr: f64,
    ndcg: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args =
        parse_args(std::env::args().skip(1).collect::<Vec<_>>()).map_err(std::io::Error::other)?;
    let payload = fs::read_to_string(&args.suite_path)?;
    let suite: BenchSuiteReport = serde_json::from_str(&payload)?;

    let mut failures = Vec::new();
    validate_benchmark_rules(&suite, &args.rules, &mut failures);
    validate_eval_rules(&suite, &args.eval_rules, &mut failures);

    if failures.is_empty() {
        println!(
            "suite assertions passed: bench_rules={}, eval_rules={}",
            args.rules.len(),
            args.eval_rules.len()
        );
        return Ok(());
    }

    eprintln!("suite assertion failures:");
    for failure in &failures {
        eprintln!("- {failure}");
    }
    Err(std::io::Error::other(format!("{} assertion(s) failed", failures.len())).into())
}

fn validate_benchmark_rules(
    suite: &BenchSuiteReport,
    rules: &[BenchmarkRule],
    failures: &mut Vec<String>,
) {
    for rule in rules {
        let run = suite
            .benchmark_profiles
            .iter()
            .find(|matrix| matrix.profile == rule.profile)
            .and_then(|matrix| matrix.runs.iter().find(|run| run.name == rule.scenario));

        let Some(run) = run else {
            failures.push(format!(
                "missing benchmark run profile=`{}` scenario=`{}`",
                rule.profile, rule.scenario
            ));
            continue;
        };

        if let Some(min_qps) = rule.min_qps
            && run.report.qps < min_qps
        {
            failures.push(format!(
                "profile=`{}` scenario=`{}` qps {:.2} < min_qps {:.2}",
                rule.profile, rule.scenario, run.report.qps, min_qps
            ));
        }
        if let Some(max_p95_ms) = rule.max_p95_ms
            && run.report.latency.p95_ms > max_p95_ms
        {
            failures.push(format!(
                "profile=`{}` scenario=`{}` p95 {:.3}ms > max_p95 {:.3}ms",
                rule.profile, rule.scenario, run.report.latency.p95_ms, max_p95_ms
            ));
        }
        if let Some(min_top1) = rule.min_top1
            && run.report.top1_hit_rate < min_top1
        {
            failures.push(format!(
                "profile=`{}` scenario=`{}` top1 {:.4} < min_top1 {:.4}",
                rule.profile, rule.scenario, run.report.top1_hit_rate, min_top1
            ));
        }
        if let Some(min_ingest_cpm) = rule.min_ingest_cpm {
            let ingest_cpm = run.report.ingest_chunks_per_sec * 60.0;
            if ingest_cpm < min_ingest_cpm {
                failures.push(format!(
                    "profile=`{}` scenario=`{}` ingest_cpm {:.2} < min_ingest_cpm {:.2}",
                    rule.profile, rule.scenario, ingest_cpm, min_ingest_cpm
                ));
            }
        }
    }
}

fn validate_eval_rules(suite: &BenchSuiteReport, rules: &[EvalRule], failures: &mut Vec<String>) {
    for rule in rules {
        let run = suite
            .eval_runs
            .iter()
            .find(|run| run.index_mode == rule.index_mode);
        let Some(run) = run else {
            failures.push(format!("missing eval run index_mode=`{}`", rule.index_mode));
            continue;
        };

        let k1 = run.report.aggregate_metrics_at_k.get("1");
        let Some(k1) = k1 else {
            failures.push(format!(
                "missing eval k=1 aggregate metrics index_mode=`{}`",
                rule.index_mode
            ));
            continue;
        };

        if let Some(min_recall_k1) = rule.min_recall_k1
            && k1.recall < min_recall_k1
        {
            failures.push(format!(
                "eval index_mode=`{}` recall@1 {:.4} < min_recall_k1 {:.4}",
                rule.index_mode, k1.recall, min_recall_k1
            ));
        }
        if let Some(min_mrr_k1) = rule.min_mrr_k1
            && k1.mrr < min_mrr_k1
        {
            failures.push(format!(
                "eval index_mode=`{}` mrr@1 {:.4} < min_mrr_k1 {:.4}",
                rule.index_mode, k1.mrr, min_mrr_k1
            ));
        }
        if let Some(min_ndcg_k1) = rule.min_ndcg_k1
            && k1.ndcg < min_ndcg_k1
        {
            failures.push(format!(
                "eval index_mode=`{}` ndcg@1 {:.4} < min_ndcg_k1 {:.4}",
                rule.index_mode, k1.ndcg, min_ndcg_k1
            ));
        }
    }
}

fn parse_args(args: Vec<String>) -> Result<BenchSuiteAssertArgs, String> {
    let mut suite_path = None;
    let mut rules = Vec::new();
    let mut eval_rules = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--suite" => {
                i += 1;
                suite_path = Some(PathBuf::from(parse_string(&args, i, "--suite")?));
            }
            "--rule" => {
                i += 1;
                rules.push(parse_benchmark_rule(&parse_string(&args, i, "--rule")?)?);
            }
            "--eval-rule" => {
                i += 1;
                eval_rules.push(parse_eval_rule(&parse_string(&args, i, "--eval-rule")?)?);
            }
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown argument `{other}`\n{}", usage())),
        }
        i += 1;
    }

    let suite_path =
        suite_path.ok_or_else(|| format!("missing required --suite <path>\n{}", usage()))?;
    if rules.is_empty() && eval_rules.is_empty() {
        return Err(format!(
            "provide at least one --rule or --eval-rule\n{}",
            usage()
        ));
    }

    Ok(BenchSuiteAssertArgs {
        suite_path,
        rules,
        eval_rules,
    })
}

fn parse_string(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}\n{}", usage()))
}

fn parse_benchmark_rule(value: &str) -> Result<BenchmarkRule, String> {
    let fields = parse_key_values(value)?;
    let profile = required_field(&fields, "profile", value)?;
    let scenario = required_field(&fields, "scenario", value)?;
    Ok(BenchmarkRule {
        profile,
        scenario,
        min_qps: parse_optional_f64(&fields, "min_qps", value)?,
        max_p95_ms: parse_optional_f64(&fields, "max_p95_ms", value)?,
        min_top1: parse_optional_f64(&fields, "min_top1", value)?,
        min_ingest_cpm: parse_optional_f64(&fields, "min_ingest_cpm", value)?,
    })
}

fn parse_eval_rule(value: &str) -> Result<EvalRule, String> {
    let fields = parse_key_values(value)?;
    let index_mode = required_field(&fields, "index_mode", value)?;
    Ok(EvalRule {
        index_mode,
        min_recall_k1: parse_optional_f64(&fields, "min_recall_k1", value)?,
        min_mrr_k1: parse_optional_f64(&fields, "min_mrr_k1", value)?,
        min_ndcg_k1: parse_optional_f64(&fields, "min_ndcg_k1", value)?,
    })
}

fn parse_key_values(value: &str) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    for part in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let (key, val) = part.split_once('=').ok_or_else(|| {
            format!(
                "invalid rule fragment `{part}` in `{value}`; expected key=value\n{}",
                usage()
            )
        })?;
        out.insert(key.trim().to_string(), val.trim().to_string());
    }
    if out.is_empty() {
        return Err(format!("invalid empty rule `{value}`\n{}", usage()));
    }
    Ok(out)
}

fn required_field(
    fields: &HashMap<String, String>,
    key: &str,
    raw: &str,
) -> Result<String, String> {
    fields.get(key).cloned().ok_or_else(|| {
        format!(
            "missing `{key}` in rule `{raw}`\nexpected key/value pairs like profile=100k,scenario=weighted + lsh_ann,max_p95_ms=40\n{}",
            usage()
        )
    })
}

fn parse_optional_f64(
    fields: &HashMap<String, String>,
    key: &str,
    raw: &str,
) -> Result<Option<f64>, String> {
    let Some(value) = fields.get(key) else {
        return Ok(None);
    };
    let parsed = value.parse::<f64>().map_err(|_| {
        format!(
            "invalid numeric value `{value}` for `{key}` in rule `{raw}`\n{}",
            usage()
        )
    })?;
    Ok(Some(parsed))
}

fn usage() -> String {
    "usage: cargo run --bin sqlrite-bench-suite-assert -- --suite <path> [--rule \"profile=<name>,scenario=<scenario>,min_qps=<f>,max_p95_ms=<f>,min_top1=<f>,min_ingest_cpm=<f>\"]... [--eval-rule \"index_mode=<mode>,min_recall_k1=<f>,min_mrr_k1=<f>,min_ndcg_k1=<f>\"]...".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_benchmark_rule() -> Result<(), Box<dyn std::error::Error>> {
        let rule = parse_benchmark_rule(
            "profile=100k,scenario=weighted + lsh_ann,max_p95_ms=40,min_top1=0.99,min_qps=8",
        )
        .map_err(std::io::Error::other)?;
        assert_eq!(rule.profile, "100k");
        assert_eq!(rule.scenario, "weighted + lsh_ann");
        assert_eq!(rule.max_p95_ms, Some(40.0));
        assert_eq!(rule.min_top1, Some(0.99));
        assert_eq!(rule.min_qps, Some(8.0));
        Ok(())
    }

    #[test]
    fn parses_eval_rule() -> Result<(), Box<dyn std::error::Error>> {
        let rule = parse_eval_rule("index_mode=lsh_ann,min_recall_k1=0.8,min_mrr_k1=1.0")
            .map_err(std::io::Error::other)?;
        assert_eq!(rule.index_mode, "lsh_ann");
        assert_eq!(rule.min_recall_k1, Some(0.8));
        assert_eq!(rule.min_mrr_k1, Some(1.0));
        Ok(())
    }

    #[test]
    fn parse_args_requires_suite_and_rules() {
        assert!(parse_args(Vec::new()).is_err());
        assert!(parse_args(vec!["--suite".to_string(), "x.json".to_string()]).is_err());
    }
}
