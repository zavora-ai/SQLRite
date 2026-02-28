use sqlrite::{FusionStrategy, SearchRequest, SqlRite};
use std::collections::HashMap;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args =
        parse_args(std::env::args().skip(1).collect::<Vec<_>>()).map_err(std::io::Error::other)?;

    let db = SqlRite::open(&args.db_path)?;

    let fusion_strategy = match args.fusion_mode.as_str() {
        "weighted" => FusionStrategy::Weighted,
        "rrf" => FusionStrategy::ReciprocalRankFusion {
            rank_constant: args.rrf_rank_constant,
        },
        other => {
            return Err(std::io::Error::other(format!(
                "invalid --fusion `{other}`; expected weighted or rrf"
            ))
            .into());
        }
    };

    let request = SearchRequest {
        query_text: args.query_text,
        query_embedding: args.query_embedding,
        top_k: args.top_k,
        alpha: args.alpha,
        candidate_limit: args.candidate_limit,
        metadata_filters: args.metadata_filters,
        doc_id: args.doc_id,
        fusion_strategy,
    };

    let results = db.search(request)?;
    println!("results={}", results.len());
    for (idx, item) in results.iter().enumerate() {
        println!(
            "{}. {} | doc={} | hybrid={:.3} | vector={:.3} | text={:.3}",
            idx + 1,
            item.chunk_id,
            item.doc_id,
            item.hybrid_score,
            item.vector_score,
            item.text_score
        );
        println!("   {}", item.content);
    }

    Ok(())
}

#[derive(Debug)]
struct QueryCliArgs {
    db_path: PathBuf,
    query_text: Option<String>,
    query_embedding: Option<Vec<f32>>,
    top_k: usize,
    alpha: f32,
    candidate_limit: usize,
    doc_id: Option<String>,
    fusion_mode: String,
    rrf_rank_constant: f32,
    metadata_filters: HashMap<String, String>,
}

impl Default for QueryCliArgs {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("sqlrite_demo.db"),
            query_text: None,
            query_embedding: None,
            top_k: 5,
            alpha: 0.65,
            candidate_limit: 500,
            doc_id: None,
            fusion_mode: "weighted".to_string(),
            rrf_rank_constant: 60.0,
            metadata_filters: HashMap::new(),
        }
    }
}

fn parse_args(args: Vec<String>) -> Result<QueryCliArgs, String> {
    let mut cfg = QueryCliArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                cfg.db_path = PathBuf::from(parse_string(&args, i, "--db")?);
            }
            "--text" => {
                i += 1;
                cfg.query_text = Some(parse_string(&args, i, "--text")?);
            }
            "--vector" => {
                i += 1;
                cfg.query_embedding =
                    Some(parse_embedding_csv(&parse_string(&args, i, "--vector")?)?);
            }
            "--top-k" => {
                i += 1;
                cfg.top_k = parse_usize(&args, i, "--top-k")?;
            }
            "--alpha" => {
                i += 1;
                cfg.alpha = parse_f32(&args, i, "--alpha")?;
            }
            "--candidate-limit" => {
                i += 1;
                cfg.candidate_limit = parse_usize(&args, i, "--candidate-limit")?;
            }
            "--doc-id" => {
                i += 1;
                cfg.doc_id = Some(parse_string(&args, i, "--doc-id")?);
            }
            "--fusion" => {
                i += 1;
                cfg.fusion_mode = parse_string(&args, i, "--fusion")?;
            }
            "--rrf-k" => {
                i += 1;
                cfg.rrf_rank_constant = parse_f32(&args, i, "--rrf-k")?;
            }
            "--filter" => {
                i += 1;
                let raw = parse_string(&args, i, "--filter")?;
                let Some((key, value)) = raw.split_once('=') else {
                    return Err(format!(
                        "invalid --filter `{raw}`; expected key=value\n{}",
                        usage()
                    ));
                };
                cfg.metadata_filters
                    .insert(key.trim().to_string(), value.trim().to_string());
            }
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown argument `{other}`\n{}", usage())),
        }
        i += 1;
    }

    if cfg.query_text.is_none() && cfg.query_embedding.is_none() {
        return Err(format!(
            "at least one of --text or --vector is required\n{}",
            usage()
        ));
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

fn parse_embedding_csv(raw: &str) -> Result<Vec<f32>, String> {
    let values = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<f32>()
                .map_err(|_| format!("invalid vector value `{s}`\n{}", usage()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if values.is_empty() {
        return Err(format!("--vector cannot be empty\n{}", usage()));
    }
    Ok(values)
}

fn usage() -> String {
    "usage: cargo run --bin sqlrite-query -- [--db PATH] [--text QUERY] [--vector v1,v2,...] [--top-k N] [--alpha F] [--candidate-limit N] [--doc-id ID] [--filter key=value]... [--fusion weighted|rrf] [--rrf-k F]".to_string()
}
