use rusqlite::Connection;
use rusqlite::Error as SqlError;
use rusqlite::functions::FunctionFlags;
use rusqlite::types::ValueRef;
use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlrite::{
    BenchmarkConfig, ChunkInput, CompactionOptions, DurabilityProfile, FusionStrategy,
    RuntimeConfig, SearchRequest, ServerConfig, SqlRite, VectorIndexMode, VectorStorageKind,
    backup_file, build_health_report, run_benchmark, serve_health_endpoints, verify_backup_file,
};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return run_default_demo();
    }

    dispatch_command(args)
}

fn dispatch_command(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    match args[0].as_str() {
        "init" => cmd_init(&args[1..]),
        "sql" => cmd_sql(&args[1..]),
        "ingest" => cmd_ingest(&args[1..]),
        "query" => cmd_query(&args[1..]),
        "quickstart" => cmd_quickstart(&args[1..]),
        "serve" => cmd_serve(&args[1..]),
        "backup" => cmd_backup(&args[1..]),
        "compact" => cmd_compact(&args[1..]),
        "benchmark" => cmd_benchmark(&args[1..]),
        "doctor" => cmd_doctor(&args[1..]),
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        other => {
            Err(std::io::Error::other(format!("unknown command `{other}`\n{}", usage())).into())
        }
    }
}

fn run_default_demo() -> Result<(), Box<dyn std::error::Error>> {
    let db = SqlRite::open_with_config("sqlrite_demo.db", RuntimeConfig::default())?;
    seed_demo_chunks_if_empty(&db)?;

    let request = SearchRequest::builder()
        .query_text("local-first agent memory")
        .query_embedding(vec![0.9, 0.1, 0.0])
        .alpha(0.6)
        .top_k(3)
        .build()?;
    let results = db.search(request)?;

    println!("Top matches:");
    for item in &results {
        println!(
            "- {} (doc: {}, score: {:.3})\n  {}",
            item.chunk_id, item.doc_id, item.hybrid_score, item.content
        );
    }
    println!("\nTry `cargo run -- --help` for the unified SQLRite CLI.");

    Ok(())
}

fn seed_demo_chunks_if_empty(db: &SqlRite) -> Result<(), Box<dyn std::error::Error>> {
    if db.chunk_count()? > 0 {
        return Ok(());
    }

    db.ingest_chunks(&[
        ChunkInput {
            id: "demo-1".to_string(),
            doc_id: "doc-a".to_string(),
            content: "Rust and SQLite are ideal for local-first AI agents.".to_string(),
            embedding: vec![0.92, 0.08, 0.0],
            metadata: json!({"tenant": "demo", "topic": "agent-memory"}),
            source: Some("seed/demo-1.md".to_string()),
        },
        ChunkInput {
            id: "demo-2".to_string(),
            doc_id: "doc-b".to_string(),
            content: "Hybrid retrieval mixes vector search with keyword signals.".to_string(),
            embedding: vec![0.65, 0.35, 0.0],
            metadata: json!({"tenant": "demo", "topic": "retrieval"}),
            source: Some("seed/demo-2.md".to_string()),
        },
        ChunkInput {
            id: "demo-3".to_string(),
            doc_id: "doc-c".to_string(),
            content: "Batching and metadata filters keep RAG pipelines deterministic.".to_string(),
            embedding: vec![0.3, 0.7, 0.0],
            metadata: json!({"tenant": "demo", "topic": "ops"}),
            source: Some("seed/demo-3.md".to_string()),
        },
    ])?;
    Ok(())
}

#[derive(Debug)]
struct InitArgs {
    db_path: PathBuf,
    profile: DurabilityProfile,
    index_mode: VectorIndexMode,
    seed_demo: bool,
}

impl Default for InitArgs {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("sqlrite.db"),
            profile: DurabilityProfile::Balanced,
            index_mode: VectorIndexMode::BruteForce,
            seed_demo: false,
        }
    }
}

fn cmd_init(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = parse_init_args(args).map_err(std::io::Error::other)?;
    let runtime = runtime_config(parsed.profile, parsed.index_mode);
    let db = SqlRite::open_with_config(&parsed.db_path, runtime)?;

    if parsed.seed_demo {
        seed_demo_chunks_if_empty(&db)?;
    }

    println!("initialized SQLRite database");
    println!("- path={}", parsed.db_path.display());
    println!("- schema_version={}", db.schema_version());
    println!("- chunk_count={}", db.chunk_count()?);
    println!("- profile={}", profile_name(parsed.profile));
    println!("- index_mode={}", index_mode_name(parsed.index_mode));
    Ok(())
}

fn parse_init_args(args: &[String]) -> Result<InitArgs, String> {
    let mut out = InitArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                out.db_path = PathBuf::from(parse_string(args, i, "--db")?);
            }
            "--profile" => {
                i += 1;
                out.profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--index-mode" => {
                i += 1;
                out.index_mode = parse_index_mode(&parse_string(args, i, "--index-mode")?)?;
            }
            "--seed-demo" => {
                out.seed_demo = true;
            }
            "--help" | "-h" => {
                return Err(
                    "usage: sqlrite init [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--seed-demo]".to_string(),
                )
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`\nusage: sqlrite init [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--seed-demo]"
                ))
            }
        }
        i += 1;
    }

    Ok(out)
}

#[derive(Debug)]
struct SqlArgs {
    db_path: PathBuf,
    profile: DurabilityProfile,
    statement: Option<String>,
}

fn cmd_sql(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = parse_sql_args(args).map_err(std::io::Error::other)?;
    // Ensure SQL CLI sessions always run against the latest schema/catalog migrations.
    let mut bootstrap = RuntimeConfig {
        durability_profile: parsed.profile,
        ..RuntimeConfig::default()
    };
    apply_runtime_env_overrides(&mut bootstrap);
    let _ = SqlRite::open_with_config(&parsed.db_path, bootstrap)?;

    let conn = Connection::open(&parsed.db_path)?;
    apply_sql_runtime_profile(&conn, parsed.profile)?;
    register_retrieval_sql_functions(&conn)?;

    if let Some(statement) = parsed.statement {
        execute_sql_statement(&conn, &statement)?;
    } else {
        run_sql_repl(&conn, &parsed.db_path)?;
    }

    Ok(())
}

fn parse_sql_args(args: &[String]) -> Result<SqlArgs, String> {
    let mut db_path = PathBuf::from("sqlrite.db");
    let mut profile = DurabilityProfile::Balanced;
    let mut statement = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                db_path = PathBuf::from(parse_string(args, i, "--db")?);
            }
            "--profile" => {
                i += 1;
                profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--execute" => {
                i += 1;
                statement = Some(parse_string(args, i, "--execute")?);
            }
            "--help" | "-h" => {
                return Err(
                    "usage: sqlrite sql [--db PATH] [--profile balanced|durable|fast_unsafe] [--execute \"SQL\"]".to_string(),
                )
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`\nusage: sqlrite sql [--db PATH] [--profile balanced|durable|fast_unsafe] [--execute \"SQL\"]"
                ))
            }
        }
        i += 1;
    }

    Ok(SqlArgs {
        db_path,
        profile,
        statement,
    })
}

#[derive(Debug)]
struct IngestArgs {
    db_path: PathBuf,
    profile: DurabilityProfile,
    index_mode: VectorIndexMode,
    id: String,
    doc_id: String,
    content: String,
    embedding: Vec<f32>,
    metadata: Value,
    source: Option<String>,
}

fn cmd_ingest(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = parse_ingest_args(args).map_err(std::io::Error::other)?;
    let runtime = runtime_config(parsed.profile, parsed.index_mode);
    let db = SqlRite::open_with_config(&parsed.db_path, runtime)?;

    let mut chunk = ChunkInput::new(parsed.id, parsed.doc_id, parsed.content, parsed.embedding)
        .with_metadata(parsed.metadata);
    if let Some(source) = parsed.source {
        chunk = chunk.with_source(source);
    }

    db.ingest_chunk(&chunk)?;
    println!("ingested 1 chunk");
    println!("- db={}", parsed.db_path.display());
    println!("- chunk_count={}", db.chunk_count()?);
    Ok(())
}

fn parse_ingest_args(args: &[String]) -> Result<IngestArgs, String> {
    let mut db_path = PathBuf::from("sqlrite.db");
    let mut profile = DurabilityProfile::Balanced;
    let mut index_mode = VectorIndexMode::BruteForce;
    let mut id = None;
    let mut doc_id = None;
    let mut content = None;
    let mut embedding = None;
    let mut metadata = Value::Object(Map::new());
    let mut source = None;
    let mut tenant = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                db_path = PathBuf::from(parse_string(args, i, "--db")?);
            }
            "--profile" => {
                i += 1;
                profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--index-mode" => {
                i += 1;
                index_mode = parse_index_mode(&parse_string(args, i, "--index-mode")?)?;
            }
            "--id" => {
                i += 1;
                id = Some(parse_string(args, i, "--id")?);
            }
            "--doc-id" => {
                i += 1;
                doc_id = Some(parse_string(args, i, "--doc-id")?);
            }
            "--content" => {
                i += 1;
                content = Some(parse_string(args, i, "--content")?);
            }
            "--embedding" => {
                i += 1;
                embedding = Some(parse_embedding_csv(&parse_string(args, i, "--embedding")?)?);
            }
            "--metadata" => {
                i += 1;
                let raw = parse_string(args, i, "--metadata")?;
                metadata = serde_json::from_str::<Value>(&raw)
                    .map_err(|error| format!("invalid --metadata JSON: {error}"))?;
            }
            "--tenant" => {
                i += 1;
                tenant = Some(parse_string(args, i, "--tenant")?);
            }
            "--source" => {
                i += 1;
                source = Some(parse_string(args, i, "--source")?);
            }
            "--help" | "-h" => {
                return Err("usage: sqlrite ingest [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] --id ID --doc-id ID --content TEXT --embedding v1,v2,... [--metadata JSON] [--tenant TENANT] [--source SRC]".to_string())
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`\nusage: sqlrite ingest [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] --id ID --doc-id ID --content TEXT --embedding v1,v2,... [--metadata JSON] [--tenant TENANT] [--source SRC]"
                ))
            }
        }
        i += 1;
    }

    let id = id.ok_or_else(|| "missing --id".to_string())?;
    let doc_id = doc_id.ok_or_else(|| "missing --doc-id".to_string())?;
    let content = content.ok_or_else(|| "missing --content".to_string())?;
    let embedding = embedding.ok_or_else(|| "missing --embedding".to_string())?;

    if let Some(tenant_id) = tenant {
        if let Value::Object(map) = &mut metadata {
            map.insert("tenant".to_string(), Value::String(tenant_id));
        } else {
            metadata = json!({
                "tenant": tenant_id,
                "raw": metadata,
            });
        }
    }

    Ok(IngestArgs {
        db_path,
        profile,
        index_mode,
        id,
        doc_id,
        content,
        embedding,
        metadata,
        source,
    })
}

#[derive(Debug)]
struct QueryArgs {
    db_path: PathBuf,
    profile: DurabilityProfile,
    index_mode: VectorIndexMode,
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

impl Default for QueryArgs {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("sqlrite.db"),
            profile: DurabilityProfile::Balanced,
            index_mode: VectorIndexMode::BruteForce,
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

fn cmd_query(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_query_args(args).map_err(std::io::Error::other)?;
    let runtime = runtime_config(args.profile, args.index_mode);
    let db = SqlRite::open_with_config(&args.db_path, runtime)?;

    let fusion_strategy = parse_fusion_strategy(&args.fusion_mode, args.rrf_rank_constant)
        .map_err(std::io::Error::other)?;

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

fn parse_query_args(args: &[String]) -> Result<QueryArgs, String> {
    let mut cfg = QueryArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                cfg.db_path = PathBuf::from(parse_string(args, i, "--db")?);
            }
            "--profile" => {
                i += 1;
                cfg.profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--index-mode" => {
                i += 1;
                cfg.index_mode = parse_index_mode(&parse_string(args, i, "--index-mode")?)?;
            }
            "--text" => {
                i += 1;
                cfg.query_text = Some(parse_string(args, i, "--text")?);
            }
            "--vector" => {
                i += 1;
                cfg.query_embedding = Some(parse_embedding_csv(&parse_string(args, i, "--vector")?)?);
            }
            "--top-k" => {
                i += 1;
                cfg.top_k = parse_usize(args, i, "--top-k")?;
            }
            "--alpha" => {
                i += 1;
                cfg.alpha = parse_f32(args, i, "--alpha")?;
            }
            "--candidate-limit" => {
                i += 1;
                cfg.candidate_limit = parse_usize(args, i, "--candidate-limit")?;
            }
            "--doc-id" => {
                i += 1;
                cfg.doc_id = Some(parse_string(args, i, "--doc-id")?);
            }
            "--fusion" => {
                i += 1;
                cfg.fusion_mode = parse_string(args, i, "--fusion")?;
            }
            "--rrf-k" => {
                i += 1;
                cfg.rrf_rank_constant = parse_f32(args, i, "--rrf-k")?;
            }
            "--filter" => {
                i += 1;
                let raw = parse_string(args, i, "--filter")?;
                let Some((key, value)) = raw.split_once('=') else {
                    return Err("invalid --filter format; expected key=value".to_string());
                };
                cfg.metadata_filters
                    .insert(key.trim().to_string(), value.trim().to_string());
            }
            "--help" | "-h" => {
                return Err("usage: sqlrite query [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--text QUERY] [--vector v1,v2,...] [--top-k N] [--alpha F] [--candidate-limit N] [--doc-id ID] [--filter key=value]... [--fusion weighted|rrf] [--rrf-k F]".to_string())
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`\nusage: sqlrite query [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--text QUERY] [--vector v1,v2,...] [--top-k N] [--alpha F] [--candidate-limit N] [--doc-id ID] [--filter key=value]... [--fusion weighted|rrf] [--rrf-k F]"
                ))
            }
        }
        i += 1;
    }

    if cfg.query_text.is_none() && cfg.query_embedding.is_none() {
        return Err("at least one of --text or --vector is required".to_string());
    }

    Ok(cfg)
}

#[derive(Debug)]
struct QuickstartArgs {
    db_path: PathBuf,
    profile: DurabilityProfile,
    index_mode: VectorIndexMode,
    seed_demo: bool,
    reset_db: bool,
    query_text: Option<String>,
    query_embedding: Option<Vec<f32>>,
    top_k: usize,
    alpha: f32,
    candidate_limit: usize,
    fusion_mode: String,
    rrf_rank_constant: f32,
    runs: usize,
    min_success_rate: Option<f64>,
    max_median_ms: Option<f64>,
    json_output: bool,
    output_path: Option<PathBuf>,
}

impl Default for QuickstartArgs {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("sqlrite_quickstart.db"),
            profile: DurabilityProfile::Balanced,
            index_mode: VectorIndexMode::BruteForce,
            seed_demo: true,
            reset_db: true,
            query_text: Some("agents local memory".to_string()),
            query_embedding: None,
            top_k: 3,
            alpha: 0.65,
            candidate_limit: 200,
            fusion_mode: "weighted".to_string(),
            rrf_rank_constant: 60.0,
            runs: 1,
            min_success_rate: None,
            max_median_ms: None,
            json_output: false,
            output_path: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct QuickstartRunReport {
    run: usize,
    db_path: String,
    init_ms: f64,
    query_ms: f64,
    total_ms: f64,
    chunk_count: usize,
    result_count: usize,
    success: bool,
    first_chunk_id: Option<String>,
    first_doc_id: Option<String>,
    first_hybrid_score: Option<f32>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct QuickstartReport {
    version: String,
    generated_at_unix_ms: u128,
    os: String,
    arch: String,
    db_path: String,
    profile: String,
    index_mode: String,
    runs: usize,
    successful_runs: usize,
    success_rate: f64,
    median_total_ms: f64,
    median_query_ms: f64,
    p95_total_ms: f64,
    max_total_ms: f64,
    gate_max_median_ms: Option<f64>,
    gate_max_median_ms_passed: Option<bool>,
    gate_min_success_rate: Option<f64>,
    gate_min_success_rate_passed: Option<bool>,
    runs_report: Vec<QuickstartRunReport>,
}

fn cmd_quickstart(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_quickstart_args(args).map_err(std::io::Error::other)?;
    let fusion_strategy = parse_fusion_strategy(&args.fusion_mode, args.rrf_rank_constant)
        .map_err(std::io::Error::other)?;

    let mut runs_report = Vec::with_capacity(args.runs);
    for run in 1..=args.runs {
        let run_report = match run_quickstart_once(&args, fusion_strategy, run) {
            Ok(run) => run,
            Err(error) => QuickstartRunReport {
                run,
                db_path: args.db_path.display().to_string(),
                init_ms: 0.0,
                query_ms: 0.0,
                total_ms: 0.0,
                chunk_count: 0,
                result_count: 0,
                success: false,
                first_chunk_id: None,
                first_doc_id: None,
                first_hybrid_score: None,
                error: Some(error),
            },
        };
        runs_report.push(run_report);
    }

    let report = summarize_quickstart_report(&args, runs_report);
    let payload = serde_json::to_string_pretty(&report)?;

    if let Some(path) = &args.output_path {
        fs::write(path, &payload)?;
    }

    if args.json_output {
        println!("{payload}");
    } else {
        print_quickstart_report(&report);
    }

    let mut failures = Vec::new();
    if args.min_success_rate.is_none() && report.successful_runs < report.runs {
        failures.push(format!(
            "run failures detected ({}/{})",
            report.runs - report.successful_runs,
            report.runs
        ));
    }
    if let Some(false) = report.gate_max_median_ms_passed {
        failures.push(format!(
            "median total {:.2}ms exceeded max {:.2}ms",
            report.median_total_ms,
            report.gate_max_median_ms.unwrap_or_default()
        ));
    }
    if let Some(false) = report.gate_min_success_rate_passed {
        failures.push(format!(
            "success rate {:.2}% below minimum {:.2}%",
            report.success_rate * 100.0,
            report.gate_min_success_rate.unwrap_or_default() * 100.0
        ));
    }

    if failures.is_empty() {
        return Ok(());
    }

    Err(std::io::Error::other(format!("quickstart gate failed: {}", failures.join("; "))).into())
}

fn parse_quickstart_args(args: &[String]) -> Result<QuickstartArgs, String> {
    let mut cfg = QuickstartArgs::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                cfg.db_path = PathBuf::from(parse_string(args, i, "--db")?);
            }
            "--profile" => {
                i += 1;
                cfg.profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--index-mode" => {
                i += 1;
                cfg.index_mode = parse_index_mode(&parse_string(args, i, "--index-mode")?)?;
            }
            "--seed-demo" => {
                cfg.seed_demo = true;
            }
            "--no-seed-demo" => {
                cfg.seed_demo = false;
            }
            "--reset" => {
                cfg.reset_db = true;
            }
            "--no-reset" => {
                cfg.reset_db = false;
            }
            "--text" => {
                i += 1;
                cfg.query_text = Some(parse_string(args, i, "--text")?);
            }
            "--vector" => {
                i += 1;
                cfg.query_embedding =
                    Some(parse_embedding_csv(&parse_string(args, i, "--vector")?)?);
            }
            "--top-k" => {
                i += 1;
                cfg.top_k = parse_usize(args, i, "--top-k")?;
            }
            "--alpha" => {
                i += 1;
                cfg.alpha = parse_f32(args, i, "--alpha")?;
            }
            "--candidate-limit" => {
                i += 1;
                cfg.candidate_limit = parse_usize(args, i, "--candidate-limit")?;
            }
            "--fusion" => {
                i += 1;
                cfg.fusion_mode = parse_string(args, i, "--fusion")?;
            }
            "--rrf-k" => {
                i += 1;
                cfg.rrf_rank_constant = parse_f32(args, i, "--rrf-k")?;
            }
            "--runs" => {
                i += 1;
                cfg.runs = parse_usize(args, i, "--runs")?;
            }
            "--min-success-rate" => {
                i += 1;
                cfg.min_success_rate = Some(parse_f64(args, i, "--min-success-rate")?);
            }
            "--max-median-ms" => {
                i += 1;
                cfg.max_median_ms = Some(parse_f64(args, i, "--max-median-ms")?);
            }
            "--output" => {
                i += 1;
                cfg.output_path = Some(PathBuf::from(parse_string(args, i, "--output")?));
            }
            "--json" => {
                cfg.json_output = true;
            }
            "--help" | "-h" => return Err(quickstart_usage().to_string()),
            other => {
                return Err(format!(
                    "unknown argument `{other}`\n{}",
                    quickstart_usage()
                ));
            }
        }
        i += 1;
    }

    if cfg.runs == 0 {
        return Err("invalid --runs value 0; expected >= 1".to_string());
    }
    if let Some(text) = &cfg.query_text
        && text.trim().is_empty()
    {
        cfg.query_text = None;
    }
    if cfg.query_text.is_none() && cfg.query_embedding.is_none() {
        return Err("at least one of --text or --vector is required".to_string());
    }
    if let Some(value) = cfg.min_success_rate
        && !(0.0..=1.0).contains(&value)
    {
        return Err("invalid --min-success-rate; expected value between 0 and 1".to_string());
    }
    if let Some(value) = cfg.max_median_ms
        && value <= 0.0
    {
        return Err("invalid --max-median-ms; expected positive milliseconds".to_string());
    }
    parse_fusion_strategy(&cfg.fusion_mode, cfg.rrf_rank_constant)?;

    Ok(cfg)
}

fn run_quickstart_once(
    args: &QuickstartArgs,
    fusion_strategy: FusionStrategy,
    run: usize,
) -> Result<QuickstartRunReport, String> {
    if args.reset_db {
        remove_sqlite_sidecars(&args.db_path).map_err(|error| {
            format!(
                "failed to reset db '{}' before run {}: {}",
                args.db_path.display(),
                run,
                error
            )
        })?;
    }

    let total_start = Instant::now();
    let init_start = Instant::now();
    let db =
        SqlRite::open_with_config(&args.db_path, runtime_config(args.profile, args.index_mode))
            .map_err(|error| format!("open db failed: {error}"))?;
    if args.seed_demo {
        seed_demo_chunks_if_empty(&db).map_err(|error| format!("seed demo failed: {error}"))?;
    }
    let chunk_count = db
        .chunk_count()
        .map_err(|error| format!("chunk_count failed: {error}"))?;
    let init_ms = init_start.elapsed().as_secs_f64() * 1_000.0;

    let query_start = Instant::now();
    let mut request_builder = SearchRequest::builder()
        .top_k(args.top_k)
        .alpha(args.alpha)
        .candidate_limit(args.candidate_limit)
        .fusion_strategy(fusion_strategy);
    if let Some(text) = &args.query_text {
        request_builder = request_builder.query_text(text.clone());
    }
    if let Some(vector) = &args.query_embedding {
        request_builder = request_builder.query_embedding(vector.clone());
    }
    let request = request_builder
        .build()
        .map_err(|error| format!("build request failed: {error}"))?;
    let results = db
        .search(request)
        .map_err(|error| format!("query execution failed: {error}"))?;
    let query_ms = query_start.elapsed().as_secs_f64() * 1_000.0;
    let total_ms = total_start.elapsed().as_secs_f64() * 1_000.0;

    let first = results.first();
    Ok(QuickstartRunReport {
        run,
        db_path: args.db_path.display().to_string(),
        init_ms,
        query_ms,
        total_ms,
        chunk_count,
        result_count: results.len(),
        success: !results.is_empty(),
        first_chunk_id: first.map(|item| item.chunk_id.clone()),
        first_doc_id: first.map(|item| item.doc_id.clone()),
        first_hybrid_score: first.map(|item| item.hybrid_score),
        error: None,
    })
}

fn summarize_quickstart_report(
    args: &QuickstartArgs,
    runs_report: Vec<QuickstartRunReport>,
) -> QuickstartReport {
    let successful_runs = runs_report.iter().filter(|run| run.success).count();
    let success_rate = successful_runs as f64 / runs_report.len() as f64;

    let successful_totals: Vec<f64> = runs_report
        .iter()
        .filter(|run| run.success)
        .map(|run| run.total_ms)
        .collect();
    let successful_queries: Vec<f64> = runs_report
        .iter()
        .filter(|run| run.success)
        .map(|run| run.query_ms)
        .collect();
    let median_total_ms = median(&successful_totals);
    let median_query_ms = median(&successful_queries);
    let p95_total_ms = percentile(&successful_totals, 0.95);
    let max_total_ms = successful_totals.iter().copied().fold(0.0, f64::max);

    let gate_max_median_ms_passed = args
        .max_median_ms
        .map(|limit| successful_runs > 0 && median_total_ms <= limit);
    let gate_min_success_rate_passed = args.min_success_rate.map(|minimum| success_rate >= minimum);

    QuickstartReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        generated_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
        db_path: args.db_path.display().to_string(),
        profile: profile_name(args.profile).to_string(),
        index_mode: index_mode_name(args.index_mode).to_string(),
        runs: runs_report.len(),
        successful_runs,
        success_rate,
        median_total_ms,
        median_query_ms,
        p95_total_ms,
        max_total_ms,
        gate_max_median_ms: args.max_median_ms,
        gate_max_median_ms_passed,
        gate_min_success_rate: args.min_success_rate,
        gate_min_success_rate_passed,
        runs_report,
    }
}

fn print_quickstart_report(report: &QuickstartReport) {
    println!("sqlrite quickstart");
    println!("- version={}", report.version);
    println!("- os={} arch={}", report.os, report.arch);
    println!("- db={}", report.db_path);
    println!("- profile={}", report.profile);
    println!("- index_mode={}", report.index_mode);
    println!(
        "- runs={} successful_runs={} success_rate={:.2}%",
        report.runs,
        report.successful_runs,
        report.success_rate * 100.0
    );
    println!("- median_total_ms={:.2}", report.median_total_ms);
    println!("- median_query_ms={:.2}", report.median_query_ms);
    println!("- p95_total_ms={:.2}", report.p95_total_ms);
    println!("- max_total_ms={:.2}", report.max_total_ms);

    if let Some(limit) = report.gate_max_median_ms {
        println!(
            "- gate_max_median_ms={:.2} passed={}",
            limit,
            report.gate_max_median_ms_passed.unwrap_or(false)
        );
    }
    if let Some(limit) = report.gate_min_success_rate {
        println!(
            "- gate_min_success_rate={:.2}% passed={}",
            limit * 100.0,
            report.gate_min_success_rate_passed.unwrap_or(false)
        );
    }

    for run in &report.runs_report {
        if run.success {
            println!(
                "- run={} success=true total_ms={:.2} init_ms={:.2} query_ms={:.2} results={} first_chunk={}",
                run.run,
                run.total_ms,
                run.init_ms,
                run.query_ms,
                run.result_count,
                run.first_chunk_id.as_deref().unwrap_or("<none>")
            );
        } else {
            println!(
                "- run={} success=false error={}",
                run.run,
                run.error.as_deref().unwrap_or("unknown failure")
            );
        }
    }
}

fn remove_sqlite_sidecars(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut targets = Vec::new();
    targets.push(path.to_path_buf());
    for suffix in ["-wal", "-shm", "-journal"] {
        let raw = format!("{}{}", path.display(), suffix);
        targets.push(PathBuf::from(raw));
    }

    for target in targets {
        if target.exists() {
            fs::remove_file(&target)?;
        }
    }
    Ok(())
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((sorted.len() - 1) as f64 * p.clamp(0.0, 1.0)).round() as usize;
    sorted[rank]
}

fn quickstart_usage() -> &'static str {
    "usage: sqlrite quickstart [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--seed-demo|--no-seed-demo] [--reset|--no-reset] [--text QUERY] [--vector v1,v2,...] [--top-k N] [--alpha F] [--candidate-limit N] [--fusion weighted|rrf] [--rrf-k F] [--runs N] [--max-median-ms F] [--min-success-rate F] [--json] [--output PATH]"
}

#[derive(Debug)]
struct ServeArgs {
    db_path: PathBuf,
    bind_addr: String,
    profile: DurabilityProfile,
    index_mode: VectorIndexMode,
}

fn cmd_serve(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_serve_args(args).map_err(std::io::Error::other)?;
    println!("starting SQLRite server on {}", args.bind_addr);
    serve_health_endpoints(
        args.db_path,
        runtime_config(args.profile, args.index_mode),
        ServerConfig {
            bind_addr: args.bind_addr,
        },
    )
    .map_err(|error| error.into())
}

fn parse_serve_args(args: &[String]) -> Result<ServeArgs, String> {
    let mut out = ServeArgs {
        db_path: PathBuf::from("sqlrite.db"),
        bind_addr: "127.0.0.1:8099".to_string(),
        profile: DurabilityProfile::Balanced,
        index_mode: VectorIndexMode::BruteForce,
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                out.db_path = PathBuf::from(parse_string(args, i, "--db")?);
            }
            "--bind" => {
                i += 1;
                out.bind_addr = parse_string(args, i, "--bind")?;
            }
            "--profile" => {
                i += 1;
                out.profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--index-mode" => {
                i += 1;
                out.index_mode = parse_index_mode(&parse_string(args, i, "--index-mode")?)?;
            }
            "--help" | "-h" => {
                return Err("usage: sqlrite serve [--db PATH] [--bind HOST:PORT] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled]".to_string())
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`\nusage: sqlrite serve [--db PATH] [--bind HOST:PORT] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled]"
                ))
            }
        }
        i += 1;
    }

    Ok(out)
}

fn cmd_backup(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(args.first().map(String::as_str), Some("verify")) {
        return cmd_backup_verify(&args[1..]);
    }

    let mut source = None;
    let mut destination = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--source" => {
                i += 1;
                source = Some(parse_string(args, i, "--source")?);
            }
            "--dest" => {
                i += 1;
                destination = Some(parse_string(args, i, "--dest")?);
            }
            "--help" | "-h" => {
                return Err(std::io::Error::other(
                    "usage:\n  sqlrite backup --source <db_path> --dest <backup_path>\n  sqlrite backup verify --path <backup_path>",
                )
                .into())
            }
            other => {
                return Err(std::io::Error::other(format!(
                    "unknown argument `{other}`\nusage:\n  sqlrite backup --source <db_path> --dest <backup_path>\n  sqlrite backup verify --path <backup_path>"
                ))
                .into())
            }
        }
        i += 1;
    }

    let source = source.ok_or_else(|| {
        std::io::Error::other(
            "missing --source\nusage:\n  sqlrite backup --source <db_path> --dest <backup_path>\n  sqlrite backup verify --path <backup_path>",
        )
    })?;
    let destination = destination.ok_or_else(|| {
        std::io::Error::other(
            "missing --dest\nusage:\n  sqlrite backup --source <db_path> --dest <backup_path>\n  sqlrite backup verify --path <backup_path>",
        )
    })?;

    backup_file(source, destination)?;
    println!("backup complete");
    Ok(())
}

fn cmd_backup_verify(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut path = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--path" => {
                i += 1;
                path = Some(parse_string(args, i, "--path")?);
            }
            "--help" | "-h" => {
                return Err(std::io::Error::other(
                    "usage: sqlrite backup verify --path <backup_path>",
                )
                .into());
            }
            other => {
                return Err(std::io::Error::other(format!(
                    "unknown argument `{other}`\nusage: sqlrite backup verify --path <backup_path>"
                ))
                .into());
            }
        }
        i += 1;
    }

    let path = path.ok_or_else(|| {
        std::io::Error::other("missing --path\nusage: sqlrite backup verify --path <backup_path>")
    })?;
    let report = verify_backup_file(path)?;
    println!("backup verification:");
    println!("- integrity_ok={}", report.integrity_check_ok);
    println!("- chunk_count={}", report.chunk_count);
    println!("- schema_version={}", report.schema_version);
    println!("- index_mode={}", report.vector_index_mode);
    Ok(())
}

#[derive(Debug)]
struct CompactArgs {
    db_path: PathBuf,
    profile: DurabilityProfile,
    index_mode: VectorIndexMode,
    dedupe_by_content_hash: bool,
    prune_orphan_documents: bool,
    wal_checkpoint_truncate: bool,
    analyze: bool,
    vacuum: bool,
    json_output: bool,
}

impl Default for CompactArgs {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("sqlrite.db"),
            profile: DurabilityProfile::Balanced,
            index_mode: VectorIndexMode::BruteForce,
            dedupe_by_content_hash: true,
            prune_orphan_documents: true,
            wal_checkpoint_truncate: true,
            analyze: true,
            vacuum: true,
            json_output: false,
        }
    }
}

fn cmd_compact(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_compact_args(args).map_err(std::io::Error::other)?;
    let runtime = runtime_config(args.profile, args.index_mode);
    let db = SqlRite::open_with_config(&args.db_path, runtime)?;
    let report = db.compact(CompactionOptions {
        dedupe_by_content_hash: args.dedupe_by_content_hash,
        prune_orphan_documents: args.prune_orphan_documents,
        wal_checkpoint_truncate: args.wal_checkpoint_truncate,
        analyze: args.analyze,
        vacuum: args.vacuum,
    })?;

    if args.json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("sqlrite compact");
    println!("- db_path={}", args.db_path.display());
    println!(
        "- options=dedupe_by_content_hash:{}, prune_orphan_documents:{}, wal_checkpoint_truncate:{}, analyze:{}, vacuum:{}",
        args.dedupe_by_content_hash,
        args.prune_orphan_documents,
        args.wal_checkpoint_truncate,
        args.analyze,
        args.vacuum
    );
    println!(
        "- chunks(before={}, after={}, removed={}, deduplicated={})",
        report.before_chunks,
        report.after_chunks,
        report.removed_chunks,
        report.deduplicated_chunks
    );
    println!(
        "- documents(before={}, after={}, orphan_removed={})",
        report.before_documents, report.after_documents, report.orphan_documents_removed
    );
    println!(
        "- maintenance(wal_checkpoint_applied={}, analyze_applied={}, vacuum_applied={}, vector_index_rebuilt={})",
        report.wal_checkpoint_applied,
        report.analyze_applied,
        report.vacuum_applied,
        report.vector_index_rebuilt
    );
    println!(
        "- storage(size_before_bytes={:?}, size_after_bytes={:?}, reclaimed_bytes={:?})",
        report.database_size_before_bytes, report.database_size_after_bytes, report.reclaimed_bytes
    );
    println!("- duration_ms={:.2}", report.duration_ms);
    Ok(())
}

fn parse_compact_args(args: &[String]) -> Result<CompactArgs, String> {
    let mut out = CompactArgs::default();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                out.db_path = PathBuf::from(parse_string(args, i, "--db")?);
            }
            "--profile" => {
                i += 1;
                out.profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--index-mode" => {
                i += 1;
                out.index_mode = parse_index_mode(&parse_string(args, i, "--index-mode")?)?;
            }
            "--no-dedupe-by-content-hash" => {
                out.dedupe_by_content_hash = false;
            }
            "--no-prune-orphan-documents" => {
                out.prune_orphan_documents = false;
            }
            "--no-wal-checkpoint" => {
                out.wal_checkpoint_truncate = false;
            }
            "--no-analyze" => {
                out.analyze = false;
            }
            "--no-vacuum" => {
                out.vacuum = false;
            }
            "--json" => {
                out.json_output = true;
            }
            "--help" | "-h" => {
                return Err("usage: sqlrite compact [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--no-dedupe-by-content-hash] [--no-prune-orphan-documents] [--no-wal-checkpoint] [--no-analyze] [--no-vacuum] [--json]".to_string());
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`\nusage: sqlrite compact [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--no-dedupe-by-content-hash] [--no-prune-orphan-documents] [--no-wal-checkpoint] [--no-analyze] [--no-vacuum] [--json]"
                ));
            }
        }
        i += 1;
    }

    Ok(out)
}

#[derive(Debug)]
struct BenchmarkArgs {
    config: BenchmarkConfig,
    profile: DurabilityProfile,
    index_mode: VectorIndexMode,
    output_path: Option<PathBuf>,
}

fn cmd_benchmark(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_benchmark_args(args).map_err(std::io::Error::other)?;
    let runtime = runtime_config(args.profile, args.index_mode);
    let report = run_benchmark(args.config, runtime)?;

    println!(
        "SQLRite benchmark: corpus={}, queries={}, concurrency={}, index={}, fusion={}",
        report.corpus_size,
        report.query_count,
        report.concurrency,
        report.vector_index_mode,
        report.fusion_strategy
    );
    println!(
        "runtime: storage={}, mmap_size_bytes={}, cache_size_kib={}",
        report.vector_storage_kind, report.sqlite_mmap_size_bytes, report.sqlite_cache_size_kib
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
        "latency_ms: avg={:.4}, p50={:.4}, p95={:.4}, p99={:.4}",
        report.latency.avg_ms, report.latency.p50_ms, report.latency.p95_ms, report.latency.p99_ms
    );
    if let Some(path) = args.output_path {
        fs::write(path, serde_json::to_string_pretty(&report)?)?;
    }
    Ok(())
}

fn parse_benchmark_args(args: &[String]) -> Result<BenchmarkArgs, String> {
    let mut config = BenchmarkConfig::default();
    let mut profile = DurabilityProfile::Balanced;
    let mut index_mode = VectorIndexMode::BruteForce;
    let mut fusion_mode = "weighted".to_string();
    let mut rrf_rank_constant = 60.0f32;
    let mut output_path = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--corpus" => {
                i += 1;
                config.corpus_size = parse_usize(args, i, "--corpus")?;
            }
            "--queries" => {
                i += 1;
                config.query_count = parse_usize(args, i, "--queries")?;
            }
            "--warmup" => {
                i += 1;
                config.warmup_queries = parse_usize(args, i, "--warmup")?;
            }
            "--concurrency" => {
                i += 1;
                config.concurrency = parse_usize(args, i, "--concurrency")?;
            }
            "--embedding-dim" => {
                i += 1;
                config.embedding_dim = parse_usize(args, i, "--embedding-dim")?;
            }
            "--top-k" => {
                i += 1;
                config.top_k = parse_usize(args, i, "--top-k")?;
            }
            "--candidate-limit" => {
                i += 1;
                config.candidate_limit = parse_usize(args, i, "--candidate-limit")?;
            }
            "--batch-size" => {
                i += 1;
                config.batch_size = parse_usize(args, i, "--batch-size")?;
            }
            "--alpha" => {
                i += 1;
                config.alpha = parse_f32(args, i, "--alpha")?;
            }
            "--fusion" => {
                i += 1;
                fusion_mode = parse_string(args, i, "--fusion")?;
            }
            "--rrf-k" => {
                i += 1;
                rrf_rank_constant = parse_f32(args, i, "--rrf-k")?;
            }
            "--profile" => {
                i += 1;
                profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--durability" => {
                i += 1;
                profile = parse_profile(&parse_string(args, i, "--durability")?)?;
            }
            "--index-mode" => {
                i += 1;
                index_mode = parse_index_mode(&parse_string(args, i, "--index-mode")?)?;
            }
            "--output" => {
                i += 1;
                output_path = Some(PathBuf::from(parse_string(args, i, "--output")?));
            }
            "--help" | "-h" => {
                return Err("usage: sqlrite benchmark [--corpus N] [--queries N] [--warmup N] [--concurrency N] [--embedding-dim N] [--top-k N] [--candidate-limit N] [--batch-size N] [--alpha F] [--fusion weighted|rrf] [--rrf-k F] [--profile balanced|durable|fast_unsafe] [--durability balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--output PATH]".to_string())
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`\nusage: sqlrite benchmark [--corpus N] [--queries N] [--warmup N] [--concurrency N] [--embedding-dim N] [--top-k N] [--candidate-limit N] [--batch-size N] [--alpha F] [--fusion weighted|rrf] [--rrf-k F] [--profile balanced|durable|fast_unsafe] [--durability balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--output PATH]"
                ))
            }
        }
        i += 1;
    }

    config.fusion_strategy = parse_fusion_strategy(&fusion_mode, rrf_rank_constant)?;

    Ok(BenchmarkArgs {
        config,
        profile,
        index_mode,
        output_path,
    })
}

fn cmd_doctor(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut db_path = PathBuf::from("sqlrite.db");
    let mut profile = DurabilityProfile::Balanced;
    let mut index_mode = VectorIndexMode::BruteForce;
    let mut json_output = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                db_path = PathBuf::from(parse_string(args, i, "--db")?);
            }
            "--profile" => {
                i += 1;
                profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--index-mode" => {
                i += 1;
                index_mode = parse_index_mode(&parse_string(args, i, "--index-mode")?)?;
            }
            "--json" => {
                json_output = true;
            }
            "--help" | "-h" => {
                return Err(std::io::Error::other(
                    "usage: sqlrite doctor [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--json]",
                )
                .into())
            }
            other => {
                return Err(std::io::Error::other(format!(
                    "unknown argument `{other}`\nusage: sqlrite doctor [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--json]"
                ))
                .into())
            }
        }
        i += 1;
    }

    let memory_db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
    let in_memory_integrity_ok = memory_db.integrity_check_ok()?;

    let db_exists_before = db_path.exists();
    let runtime = runtime_config(profile, index_mode);
    let db = SqlRite::open_with_config(&db_path, runtime.clone())?;
    let health = build_health_report(&db)?;
    let db_parent_writable = path_parent_writable(&db_path);

    let local_bin = default_local_bin();
    let path_contains_local_bin = local_bin
        .as_ref()
        .is_some_and(|path| path_is_in_env_path(path));

    let rustc_version = tool_version_line("rustc");
    let cargo_version = tool_version_line("cargo");

    let mut recommendations = Vec::new();
    if let Some(local_bin_path) = &local_bin
        && !path_contains_local_bin
    {
        recommendations.push(format!(
            "add '{}' to PATH to run sqlrite globally",
            local_bin_path.display()
        ));
    }
    if !db_exists_before {
        recommendations.push(format!(
            "database '{}' did not exist before diagnostics; run 'sqlrite init --db {} --seed-demo' if this is a new environment",
            db_path.display(),
            db_path.display()
        ));
    }
    if !health.integrity_check_ok {
        recommendations.push(
            "database integrity_check is not ok; run backup/restore verification".to_string(),
        );
    }
    if rustc_version.is_none() || cargo_version.is_none() {
        recommendations.push(
            "rust toolchain not fully available in PATH; installs from source may fail".to_string(),
        );
    }

    let report = DoctorReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
        cwd: env::current_dir()?.display().to_string(),
        binary_path: env::current_exe()?.display().to_string(),
        path_contains_local_bin,
        local_bin: local_bin.map(|p| p.display().to_string()),
        rustc_version,
        cargo_version,
        supported_profiles: vec![
            "balanced".to_string(),
            "durable".to_string(),
            "fast_unsafe".to_string(),
        ],
        supported_index_modes: vec![
            "brute_force".to_string(),
            "lsh_ann".to_string(),
            "hnsw_baseline".to_string(),
            "disabled".to_string(),
        ],
        supported_vector_storage: vec!["f32".to_string(), "f16".to_string(), "int8".to_string()],
        in_memory_integrity_ok,
        db: DoctorDbReport {
            path: db_path.display().to_string(),
            existed_before: db_exists_before,
            parent_writable: db_parent_writable,
            profile: profile_name(profile).to_string(),
            requested_index_mode: index_mode_name(index_mode).to_string(),
            requested_vector_storage: runtime.vector_storage_kind.as_str().to_string(),
            integrity_ok: health.integrity_check_ok,
            schema_version: health.schema_version,
            chunk_count: health.chunk_count,
            active_index_mode: health.vector_index_mode,
            active_storage_kind: health.vector_index_storage_kind,
            index_entries: health.vector_index_entries,
            index_estimated_memory_bytes: health.vector_index_estimated_memory_bytes,
            sqlite_mmap_size_bytes: runtime.sqlite_mmap_size_bytes,
            sqlite_cache_size_kib: runtime.sqlite_cache_size_kib,
        },
        recommendations,
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("sqlrite doctor");
    println!("- version={}", report.version);
    println!("- os={} arch={}", report.os, report.arch);
    println!("- cwd={}", report.cwd);
    println!("- binary_path={}", report.binary_path);
    if let Some(local_bin_path) = &report.local_bin {
        println!("- local_bin={}", local_bin_path);
    }
    println!(
        "- path_contains_local_bin={}",
        report.path_contains_local_bin
    );
    if let Some(value) = &report.rustc_version {
        println!("- rustc={value}");
    }
    if let Some(value) = &report.cargo_version {
        println!("- cargo={value}");
    }
    println!(
        "- supported_profiles={}",
        report.supported_profiles.join(",")
    );
    println!(
        "- supported_index_modes={}",
        report.supported_index_modes.join(",")
    );
    println!(
        "- supported_vector_storage={}",
        report.supported_vector_storage.join(",")
    );
    println!("- in_memory_integrity_ok={}", report.in_memory_integrity_ok);
    println!("- db_path={}", report.db.path);
    println!("- db_exists_before={}", report.db.existed_before);
    println!("- db_parent_writable={}", report.db.parent_writable);
    println!("- profile={}", report.db.profile);
    println!("- requested_index_mode={}", report.db.requested_index_mode);
    println!(
        "- requested_vector_storage={}",
        report.db.requested_vector_storage
    );
    println!("- integrity_ok={}", report.db.integrity_ok);
    println!("- schema_version={}", report.db.schema_version);
    println!("- chunk_count={}", report.db.chunk_count);
    println!("- index_mode={}", report.db.active_index_mode);
    println!("- vector_storage={}", report.db.active_storage_kind);
    println!("- index_entries={}", report.db.index_entries);
    println!(
        "- index_estimated_memory_bytes={}",
        report.db.index_estimated_memory_bytes
    );
    println!(
        "- sqlite_mmap_size_bytes={}",
        report.db.sqlite_mmap_size_bytes
    );
    println!(
        "- sqlite_cache_size_kib={}",
        report.db.sqlite_cache_size_kib
    );
    if !report.recommendations.is_empty() {
        println!("recommendations:");
        for rec in &report.recommendations {
            println!("- {rec}");
        }
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    version: String,
    os: String,
    arch: String,
    cwd: String,
    binary_path: String,
    path_contains_local_bin: bool,
    local_bin: Option<String>,
    rustc_version: Option<String>,
    cargo_version: Option<String>,
    supported_profiles: Vec<String>,
    supported_index_modes: Vec<String>,
    supported_vector_storage: Vec<String>,
    in_memory_integrity_ok: bool,
    db: DoctorDbReport,
    recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DoctorDbReport {
    path: String,
    existed_before: bool,
    parent_writable: bool,
    profile: String,
    requested_index_mode: String,
    requested_vector_storage: String,
    integrity_ok: bool,
    schema_version: i64,
    chunk_count: usize,
    active_index_mode: String,
    active_storage_kind: String,
    index_entries: usize,
    index_estimated_memory_bytes: usize,
    sqlite_mmap_size_bytes: i64,
    sqlite_cache_size_kib: i64,
}

fn runtime_config(profile: DurabilityProfile, index_mode: VectorIndexMode) -> RuntimeConfig {
    let mut cfg = RuntimeConfig::default().with_vector_index_mode(index_mode);
    cfg.durability_profile = profile;
    apply_runtime_env_overrides(&mut cfg);
    cfg
}

fn apply_runtime_env_overrides(cfg: &mut RuntimeConfig) {
    if let Ok(raw) = env::var("SQLRITE_VECTOR_STORAGE")
        && let Ok(storage) = parse_vector_storage_kind(&raw)
    {
        cfg.vector_storage_kind = storage;
    }

    let mut tuning = cfg.ann_tuning;
    if let Ok(raw) = env::var("SQLRITE_ANN_MIN_CANDIDATES")
        && let Ok(value) = raw.parse::<usize>()
    {
        tuning.min_candidates = value.max(1);
    }
    if let Ok(raw) = env::var("SQLRITE_ANN_MAX_HAMMING_RADIUS")
        && let Ok(value) = raw.parse::<usize>()
    {
        tuning.max_hamming_radius = value;
    }
    if let Ok(raw) = env::var("SQLRITE_ANN_MAX_CANDIDATE_MULTIPLIER")
        && let Ok(value) = raw.parse::<usize>()
    {
        tuning.max_candidate_multiplier = value.max(1);
    }
    cfg.ann_tuning = tuning;

    if let Ok(raw) = env::var("SQLRITE_ENABLE_ANN_PERSISTENCE")
        && let Some(value) = parse_boolish(&raw)
    {
        cfg.enable_ann_persistence = value;
    }
    if let Ok(raw) = env::var("SQLRITE_SQLITE_MMAP_SIZE")
        && let Ok(value) = raw.parse::<i64>()
    {
        cfg.sqlite_mmap_size_bytes = value.max(0);
    }
    if let Ok(raw) = env::var("SQLRITE_SQLITE_CACHE_SIZE_KIB")
        && let Ok(value) = raw.parse::<i64>()
    {
        cfg.sqlite_cache_size_kib = value.max(0);
    }
}

fn parse_profile(value: &str) -> Result<DurabilityProfile, String> {
    match value {
        "balanced" => Ok(DurabilityProfile::Balanced),
        "durable" => Ok(DurabilityProfile::Durable),
        "fast_unsafe" | "fast-unsafe" => Ok(DurabilityProfile::FastUnsafe),
        other => Err(format!(
            "invalid --profile `{other}`; expected balanced, durable, or fast_unsafe"
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
            "invalid --index-mode `{other}`; expected brute_force, lsh_ann, hnsw_baseline, or disabled"
        )),
    }
}

fn parse_vector_storage_kind(value: &str) -> Result<VectorStorageKind, String> {
    match value {
        "f32" => Ok(VectorStorageKind::F32),
        "f16" => Ok(VectorStorageKind::F16),
        "int8" => Ok(VectorStorageKind::Int8),
        other => Err(format!(
            "invalid vector storage `{other}`; expected f32, f16, or int8"
        )),
    }
}

fn parse_boolish(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_string(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_usize(args: &[String], index: usize, flag: &str) -> Result<usize, String> {
    let raw = parse_string(args, index, flag)?;
    raw.parse::<usize>()
        .map_err(|_| format!("invalid integer for {flag}: `{raw}`"))
}

fn parse_f32(args: &[String], index: usize, flag: &str) -> Result<f32, String> {
    let raw = parse_string(args, index, flag)?;
    raw.parse::<f32>()
        .map_err(|_| format!("invalid number for {flag}: `{raw}`"))
}

fn parse_f64(args: &[String], index: usize, flag: &str) -> Result<f64, String> {
    let raw = parse_string(args, index, flag)?;
    raw.parse::<f64>()
        .map_err(|_| format!("invalid number for {flag}: `{raw}`"))
}

fn parse_embedding_csv(raw: &str) -> Result<Vec<f32>, String> {
    let values = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<f32>()
                .map_err(|_| format!("invalid vector value `{s}`"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if values.is_empty() {
        return Err("embedding vector cannot be empty".to_string());
    }

    Ok(values)
}

fn parse_fusion_strategy(mode: &str, rank_constant: f32) -> Result<FusionStrategy, String> {
    match mode {
        "weighted" => Ok(FusionStrategy::Weighted),
        "rrf" => {
            if rank_constant <= 0.0 {
                return Err("invalid --rrf-k; expected > 0".to_string());
            }
            Ok(FusionStrategy::ReciprocalRankFusion { rank_constant })
        }
        other => Err(format!(
            "invalid --fusion `{other}`; expected weighted or rrf"
        )),
    }
}

fn default_local_bin() -> Option<PathBuf> {
    let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".local").join("bin"))
}

fn path_is_in_env_path(target: &Path) -> bool {
    env::var_os("PATH")
        .map(|raw| env::split_paths(&raw).any(|entry| entry == target))
        .unwrap_or(false)
}

fn path_parent_writable(path: &Path) -> bool {
    let parent = path
        .parent()
        .filter(|candidate| !candidate.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        return false;
    }

    let probe = parent.join(format!(
        ".sqlrite-write-probe-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    match fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn tool_version_line(tool: &str) -> Option<String> {
    Command::new(tool)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn profile_name(profile: DurabilityProfile) -> &'static str {
    match profile {
        DurabilityProfile::Balanced => "balanced",
        DurabilityProfile::Durable => "durable",
        DurabilityProfile::FastUnsafe => "fast_unsafe",
    }
}

fn index_mode_name(mode: VectorIndexMode) -> &'static str {
    match mode {
        VectorIndexMode::Disabled => "disabled",
        VectorIndexMode::BruteForce => "brute_force",
        VectorIndexMode::LshAnn => "lsh_ann",
        VectorIndexMode::HnswBaseline => "hnsw_baseline",
    }
}

fn apply_sql_runtime_profile(
    conn: &Connection,
    profile: DurabilityProfile,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "synchronous", synchronous_sql(profile))?;
    let _: String = conn.query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

fn synchronous_sql(profile: DurabilityProfile) -> &'static str {
    match profile {
        DurabilityProfile::Balanced => "NORMAL",
        DurabilityProfile::Durable => "FULL",
        DurabilityProfile::FastUnsafe => "OFF",
    }
}

fn register_retrieval_sql_functions(conn: &Connection) -> Result<(), SqlError> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;

    conn.create_scalar_function("vector", 1, flags, |ctx| {
        let vector = vector_from_value(ctx.get_raw(0))?;
        Ok(encode_vector_blob(&vector))
    })?;

    conn.create_scalar_function("vec_dims", 1, flags, |ctx| {
        let vector = vector_from_value(ctx.get_raw(0))?;
        Ok(vector.len() as i64)
    })?;

    conn.create_scalar_function("vec_to_json", 1, flags, |ctx| {
        let vector = vector_from_value(ctx.get_raw(0))?;
        serde_json::to_string(&vector)
            .map_err(|error| user_fn_error(format!("failed to serialize vector: {error}")))
    })?;

    conn.create_scalar_function("l2_distance", 2, flags, |ctx| {
        let left = vector_from_value(ctx.get_raw(0))?;
        let right = vector_from_value(ctx.get_raw(1))?;
        ensure_same_dimension(&left, &right)?;
        Ok(l2_distance(&left, &right) as f64)
    })?;

    conn.create_scalar_function("cosine_distance", 2, flags, |ctx| {
        let left = vector_from_value(ctx.get_raw(0))?;
        let right = vector_from_value(ctx.get_raw(1))?;
        ensure_same_dimension(&left, &right)?;
        Ok(cosine_distance(&left, &right) as f64)
    })?;

    conn.create_scalar_function("neg_inner_product", 2, flags, |ctx| {
        let left = vector_from_value(ctx.get_raw(0))?;
        let right = vector_from_value(ctx.get_raw(1))?;
        ensure_same_dimension(&left, &right)?;
        Ok(neg_inner_product(&left, &right) as f64)
    })?;

    conn.create_scalar_function("embed", 1, flags, |ctx| {
        let text = text_from_value(ctx.get_raw(0))?;
        let vector = embed_text(&text, 16);
        Ok(encode_vector_blob(&vector))
    })?;

    conn.create_scalar_function("bm25_score", 2, flags, |ctx| {
        let query = text_from_value(ctx.get_raw(0))?;
        let document = text_from_value(ctx.get_raw(1))?;
        Ok(bm25_score(&query, &document) as f64)
    })?;

    conn.create_scalar_function("hybrid_score", 3, flags, |ctx| {
        let vector_score = ctx.get::<f64>(0)?;
        let text_score = ctx.get::<f64>(1)?;
        let alpha = ctx.get::<f64>(2)?;
        if !(0.0..=1.0).contains(&alpha) {
            return Err(user_fn_error(
                "hybrid_score alpha must be between 0.0 and 1.0",
            ));
        }
        Ok((alpha * vector_score) + ((1.0 - alpha) * text_score))
    })?;

    Ok(())
}

fn user_fn_error(message: impl Into<String>) -> SqlError {
    SqlError::UserFunctionError(Box::new(std::io::Error::other(message.into())))
}

fn ensure_same_dimension(left: &[f32], right: &[f32]) -> Result<(), SqlError> {
    if left.len() == right.len() {
        return Ok(());
    }
    Err(user_fn_error(format!(
        "vector dimension mismatch: left={} right={}",
        left.len(),
        right.len()
    )))
}

fn vector_from_value(value: ValueRef<'_>) -> Result<Vec<f32>, SqlError> {
    match value {
        ValueRef::Blob(bytes) => decode_vector_blob(bytes),
        ValueRef::Text(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            parse_vector_literal(&text).map_err(user_fn_error)
        }
        ValueRef::Integer(v) => Ok(vec![v as f32]),
        ValueRef::Real(v) => Ok(vec![v as f32]),
        ValueRef::Null => Err(user_fn_error(
            "vector argument cannot be NULL; expected BLOB or text literal",
        )),
    }
}

fn text_from_value(value: ValueRef<'_>) -> Result<String, SqlError> {
    match value {
        ValueRef::Text(bytes) => Ok(String::from_utf8_lossy(bytes).to_string()),
        ValueRef::Blob(bytes) => Ok(format!("blob:{}bytes", bytes.len())),
        ValueRef::Integer(v) => Ok(v.to_string()),
        ValueRef::Real(v) => Ok(v.to_string()),
        ValueRef::Null => Err(user_fn_error("text argument cannot be NULL")),
    }
}

fn tokenize_terms(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_vector_literal(raw: &str) -> Result<Vec<f32>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty vector literal".to_string());
    }

    let inner = if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() >= 2 {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    let values = inner
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| {
            token
                .parse::<f32>()
                .map_err(|_| format!("invalid vector element `{token}`"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if values.is_empty() {
        return Err("vector literal must contain at least one value".to_string());
    }

    Ok(values)
}

fn encode_vector_blob(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn decode_vector_blob(bytes: &[u8]) -> Result<Vec<f32>, SqlError> {
    if bytes.is_empty() {
        return Err(user_fn_error("vector blob cannot be empty"));
    }
    if !bytes.len().is_multiple_of(4) {
        return Err(user_fn_error(format!(
            "invalid vector blob byte length {}; expected multiple of 4",
            bytes.len()
        )));
    }

    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(out)
}

fn l2_distance(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right.iter())
        .map(|(a, b)| {
            let delta = a - b;
            delta * delta
        })
        .sum::<f32>()
        .sqrt()
}

fn cosine_distance(left: &[f32], right: &[f32]) -> f32 {
    let dot = left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| a * b)
        .sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        return 1.0;
    }
    1.0 - (dot / (left_norm * right_norm))
}

fn neg_inner_product(left: &[f32], right: &[f32]) -> f32 {
    -left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| a * b)
        .sum::<f32>()
}

fn embed_text(text: &str, dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim.max(1)];
    let terms = tokenize_terms(text);
    if terms.is_empty() {
        out[0] = 1.0;
        return out;
    }

    for (position, term) in terms.iter().enumerate() {
        let hash = fnv1a64(term.as_bytes());
        let slot = (hash as usize) % out.len();
        let sign = if hash & 1 == 0 { 1.0 } else { -1.0 };
        let weight = 1.0 / ((position + 1) as f32).sqrt();
        out[slot] += sign * weight;
    }

    let norm = out.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut out {
            *value /= norm;
        }
    }

    out
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn bm25_score(query: &str, document: &str) -> f32 {
    let query_terms = tokenize_terms(query);
    let doc_terms = tokenize_terms(document);
    if query_terms.is_empty() || doc_terms.is_empty() {
        return 0.0;
    }

    let mut tf: HashMap<String, usize> = HashMap::new();
    for term in &doc_terms {
        *tf.entry(term.clone()).or_insert(0) += 1;
    }

    let mut unique_query_terms = HashSet::new();
    let dl = doc_terms.len() as f32;
    let avgdl = 50.0f32;
    let k1 = 1.2f32;
    let b = 0.75f32;
    let mut score = 0.0f32;

    for term in query_terms {
        if !unique_query_terms.insert(term.clone()) {
            continue;
        }

        let tf_value = tf.get(&term).copied().unwrap_or(0) as f32;
        if tf_value == 0.0 {
            continue;
        }

        let idf = ((1.0 + (1.0 / (tf_value + 1.0))).ln() + 1.0).max(0.01);
        let denominator = tf_value + k1 * (1.0 - b + b * (dl / avgdl));
        score += idf * (tf_value * (k1 + 1.0)) / denominator;
    }

    score
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VectorOperator {
    L2,
    Cosine,
    NegInner,
}

fn rewrite_sql_vector_operators(statement: &str) -> String {
    let mut rewritten = statement.to_string();
    for _ in 0..512 {
        let Some((operator_start, operator_end, operator)) = find_next_vector_operator(&rewritten)
        else {
            break;
        };
        let Some(left_start) = find_left_operand_start(&rewritten, operator_start) else {
            break;
        };
        let Some(right_end) = find_right_operand_end(&rewritten, operator_end) else {
            break;
        };

        let left_expr = rewritten[left_start..operator_start].trim();
        let right_expr = rewritten[operator_end..right_end].trim();
        if left_expr.is_empty() || right_expr.is_empty() {
            break;
        }

        let replacement = format!(
            "{}({}, {})",
            vector_operator_function(operator),
            left_expr,
            right_expr
        );
        rewritten = format!(
            "{}{}{}",
            &rewritten[..left_start],
            replacement,
            &rewritten[right_end..]
        );
    }
    rewritten
}

fn vector_operator_function(operator: VectorOperator) -> &'static str {
    match operator {
        VectorOperator::L2 => "l2_distance",
        VectorOperator::Cosine => "cosine_distance",
        VectorOperator::NegInner => "neg_inner_product",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanState {
    Normal,
    SingleQuoted,
    DoubleQuoted,
    LineComment,
    BlockComment,
}

fn find_next_vector_operator(statement: &str) -> Option<(usize, usize, VectorOperator)> {
    let bytes = statement.as_bytes();
    let mut i = 0usize;
    let mut state = ScanState::Normal;
    while i < bytes.len() {
        match state {
            ScanState::Normal => {
                if bytes[i] == b'\'' {
                    state = ScanState::SingleQuoted;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'"' {
                    state = ScanState::DoubleQuoted;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                    state = ScanState::LineComment;
                    i += 2;
                    continue;
                }
                if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    state = ScanState::BlockComment;
                    i += 2;
                    continue;
                }
                if bytes[i] == b'<' && i + 2 < bytes.len() {
                    if bytes[i + 1] == b'-' && bytes[i + 2] == b'>' {
                        return Some((i, i + 3, VectorOperator::L2));
                    }
                    if bytes[i + 1] == b'=' && bytes[i + 2] == b'>' {
                        return Some((i, i + 3, VectorOperator::Cosine));
                    }
                    if bytes[i + 1] == b'#' && bytes[i + 2] == b'>' {
                        return Some((i, i + 3, VectorOperator::NegInner));
                    }
                }
                i += 1;
            }
            ScanState::SingleQuoted => {
                if bytes[i] == b'\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2;
                    } else {
                        state = ScanState::Normal;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            ScanState::DoubleQuoted => {
                if bytes[i] == b'"' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                        i += 2;
                    } else {
                        state = ScanState::Normal;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            ScanState::LineComment => {
                if bytes[i] == b'\n' {
                    state = ScanState::Normal;
                }
                i += 1;
            }
            ScanState::BlockComment => {
                if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    state = ScanState::Normal;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        }
    }
    None
}

fn find_left_operand_start(statement: &str, operator_start: usize) -> Option<usize> {
    let bytes = statement.as_bytes();
    let mut end = operator_start;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }

    let last = bytes[end - 1];
    if last == b')' {
        let open = seek_balanced_backward(bytes, end - 1, b'(', b')')?;
        let mut start = open;
        while start > 0 && is_token_char(bytes[start - 1]) {
            start -= 1;
        }
        return Some(start);
    }
    if last == b']' {
        return seek_balanced_backward(bytes, end - 1, b'[', b']');
    }
    if last == b'\'' || last == b'"' {
        return seek_quoted_start(bytes, end - 1, last);
    }

    let mut start = end;
    while start > 0 && !is_left_boundary(bytes[start - 1]) {
        start -= 1;
    }
    Some(start)
}

fn find_right_operand_end(statement: &str, operator_end: usize) -> Option<usize> {
    let bytes = statement.as_bytes();
    let mut start = operator_end;
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    if start >= bytes.len() {
        return None;
    }

    match bytes[start] {
        b'(' => return seek_balanced_forward(bytes, start, b'(', b')').map(|idx| idx + 1),
        b'[' => return seek_balanced_forward(bytes, start, b'[', b']').map(|idx| idx + 1),
        b'\'' | b'"' => {
            return seek_quoted_end(bytes, start).map(|idx| idx + 1);
        }
        _ => {}
    }

    if is_token_char(bytes[start]) {
        let mut token_end = start;
        while token_end < bytes.len() && is_token_char(bytes[token_end]) {
            token_end += 1;
        }
        let mut probe = token_end;
        while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
            probe += 1;
        }
        if probe < bytes.len()
            && bytes[probe] == b'('
            && is_callable_token(&bytes[start..token_end])
            && let Some(close) = seek_balanced_forward(bytes, probe, b'(', b')')
        {
            return Some(close + 1);
        }
        return Some(token_end);
    }

    let mut end = start;
    while end < bytes.len() && !is_right_boundary(bytes[end]) {
        end += 1;
    }

    Some(end)
}

fn seek_balanced_backward(bytes: &[u8], close_index: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut idx = close_index;
    loop {
        let current = bytes[idx];
        if current == close {
            depth += 1;
        } else if current == open {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx);
            }
        }
        if idx == 0 {
            break;
        }
        idx -= 1;
    }
    None
}

fn seek_balanced_forward(bytes: &[u8], open_index: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut idx = open_index;
    let mut state = ScanState::Normal;
    while idx < bytes.len() {
        match state {
            ScanState::Normal => {
                if bytes[idx] == b'\'' {
                    state = ScanState::SingleQuoted;
                    idx += 1;
                    continue;
                }
                if bytes[idx] == b'"' {
                    state = ScanState::DoubleQuoted;
                    idx += 1;
                    continue;
                }
                if bytes[idx] == open {
                    depth += 1;
                } else if bytes[idx] == close {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(idx);
                    }
                }
                idx += 1;
            }
            ScanState::SingleQuoted => {
                if bytes[idx] == b'\'' {
                    if idx + 1 < bytes.len() && bytes[idx + 1] == b'\'' {
                        idx += 2;
                    } else {
                        state = ScanState::Normal;
                        idx += 1;
                    }
                } else {
                    idx += 1;
                }
            }
            ScanState::DoubleQuoted => {
                if bytes[idx] == b'"' {
                    if idx + 1 < bytes.len() && bytes[idx + 1] == b'"' {
                        idx += 2;
                    } else {
                        state = ScanState::Normal;
                        idx += 1;
                    }
                } else {
                    idx += 1;
                }
            }
            ScanState::LineComment | ScanState::BlockComment => {
                idx += 1;
            }
        }
    }
    None
}

fn seek_quoted_end(bytes: &[u8], quote_start: usize) -> Option<usize> {
    let quote = bytes[quote_start];
    let mut idx = quote_start + 1;
    while idx < bytes.len() {
        if bytes[idx] == quote {
            if idx + 1 < bytes.len() && bytes[idx + 1] == quote {
                idx += 2;
                continue;
            }
            return Some(idx);
        }
        idx += 1;
    }
    None
}

fn seek_quoted_start(bytes: &[u8], quote_end: usize, quote: u8) -> Option<usize> {
    let mut idx = quote_end;
    loop {
        if bytes[idx] == quote {
            if idx > 0 && bytes[idx - 1] == quote {
                if idx < 2 {
                    return None;
                }
                idx -= 2;
                continue;
            }
            return Some(idx);
        }
        if idx == 0 {
            break;
        }
        idx -= 1;
    }
    None
}

fn is_left_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b',' | b';'
                | b'('
                | b')'
                | b'+'
                | b'-'
                | b'*'
                | b'/'
                | b'%'
                | b'='
                | b'<'
                | b'>'
                | b'!'
                | b'|'
                | b'&'
                | b'^'
        )
}

fn is_right_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b',' | b';'
                | b')'
                | b'+'
                | b'-'
                | b'*'
                | b'/'
                | b'%'
                | b'='
                | b'<'
                | b'>'
                | b'!'
                | b'|'
                | b'&'
                | b'^'
        )
}

fn is_token_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$')
}

fn is_callable_token(token: &[u8]) -> bool {
    !token.is_empty() && token.iter().all(|byte| is_token_char(*byte))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetrievalIndexKind {
    Vector,
    Text,
}

impl RetrievalIndexKind {
    fn parse(token: &str) -> Result<Self, String> {
        match token.to_ascii_uppercase().as_str() {
            "VECTOR" => Ok(Self::Vector),
            "TEXT" => Ok(Self::Text),
            other => Err(format!(
                "unsupported retrieval index kind `{other}`; expected VECTOR or TEXT"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            RetrievalIndexKind::Vector => "vector",
            RetrievalIndexKind::Text => "text",
        }
    }
}

#[derive(Debug, Clone)]
struct CreateRetrievalIndexDdl {
    kind: RetrievalIndexKind,
    if_not_exists: bool,
    name: String,
    table_name: String,
    column_name: String,
    using_engine: String,
    options: Value,
}

#[derive(Debug, Clone)]
struct DropRetrievalIndexDdl {
    kind: RetrievalIndexKind,
    if_exists: bool,
    name: String,
}

#[derive(Debug, Clone)]
enum RetrievalIndexDdl {
    Create(CreateRetrievalIndexDdl),
    Drop(DropRetrievalIndexDdl),
}

fn try_execute_retrieval_index_ddl(
    conn: &Connection,
    statement: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let ddl = match parse_retrieval_index_ddl(statement).map_err(std::io::Error::other)? {
        Some(ddl) => ddl,
        None => return Ok(None),
    };

    ensure_retrieval_index_catalog(conn)?;

    match ddl {
        RetrievalIndexDdl::Create(create) => {
            validate_retrieval_index_target(conn, &create.table_name, &create.column_name)?;
            validate_retrieval_index_engine(create.kind, &create.using_engine)?;

            let existing_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM retrieval_indexes WHERE name = ?1",
                [create.name.as_str()],
                |row| row.get(0),
            )?;
            if existing_count > 0 {
                if create.if_not_exists {
                    return Ok(Some(format!(
                        "retrieval index `{}` already exists; skipped",
                        create.name
                    )));
                }
                return Err(std::io::Error::other(format!(
                    "retrieval index `{}` already exists",
                    create.name
                ))
                .into());
            }

            let options_json = serde_json::to_string(&create.options)?;
            conn.execute(
                "
                INSERT INTO retrieval_indexes
                    (name, index_kind, table_name, column_name, using_engine, options_json, status)
                VALUES
                    (?1, ?2, ?3, ?4, ?5, ?6, 'active')
                ",
                rusqlite::params![
                    create.name,
                    create.kind.as_str(),
                    create.table_name,
                    create.column_name,
                    create.using_engine.to_ascii_lowercase(),
                    options_json,
                ],
            )?;
            Ok(Some(format!(
                "created {} retrieval index `{}` on {}({}) using {}",
                create.kind.as_str(),
                create.name,
                create.table_name,
                create.column_name,
                create.using_engine.to_ascii_uppercase()
            )))
        }
        RetrievalIndexDdl::Drop(drop) => {
            let deleted = conn.execute(
                "DELETE FROM retrieval_indexes WHERE name = ?1 AND index_kind = ?2",
                rusqlite::params![drop.name, drop.kind.as_str()],
            )?;

            if deleted == 0 {
                if drop.if_exists {
                    return Ok(Some(format!(
                        "retrieval index `{}` does not exist; skipped",
                        drop.name
                    )));
                }
                return Err(std::io::Error::other(format!(
                    "retrieval index `{}` does not exist",
                    drop.name
                ))
                .into());
            }

            Ok(Some(format!(
                "dropped {} retrieval index `{}`",
                drop.kind.as_str(),
                drop.name
            )))
        }
    }
}

fn ensure_retrieval_index_catalog(conn: &Connection) -> Result<(), SqlError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS retrieval_indexes (
            name TEXT PRIMARY KEY,
            index_kind TEXT NOT NULL CHECK (index_kind IN ('vector', 'text')),
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            using_engine TEXT NOT NULL,
            options_json TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL DEFAULT 'active',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_retrieval_indexes_kind_table
            ON retrieval_indexes(index_kind, table_name, status);

        CREATE VIEW IF NOT EXISTS retrieval_index_catalog AS
        SELECT
            name,
            index_kind,
            table_name,
            column_name,
            using_engine,
            options_json,
            status,
            created_at
        FROM retrieval_indexes;
        ",
    )?;
    Ok(())
}

fn validate_retrieval_index_target(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let pragma = format!(
        "PRAGMA table_info({});",
        sqlite_quote_identifier(table_name).ok_or_else(|| {
            std::io::Error::other(format!(
                "invalid table identifier `{table_name}` for retrieval index"
            ))
        })?
    );
    let mut stmt = conn.prepare(&pragma)?;
    let mut rows = stmt.query([])?;
    let mut table_found = false;
    let mut column_found = false;
    while let Some(row) = rows.next()? {
        table_found = true;
        let name: String = row.get(1)?;
        if name == column_name {
            column_found = true;
        }
    }
    if !table_found {
        return Err(
            std::io::Error::other(format!("target table `{table_name}` does not exist")).into(),
        );
    }
    if !column_found {
        return Err(std::io::Error::other(format!(
            "target column `{column_name}` not found on table `{table_name}`"
        ))
        .into());
    }
    Ok(())
}

fn validate_retrieval_index_engine(
    kind: RetrievalIndexKind,
    engine: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let upper = engine.to_ascii_uppercase();
    match kind {
        RetrievalIndexKind::Vector if upper == "HNSW" => Ok(()),
        RetrievalIndexKind::Text if upper == "FTS5" => Ok(()),
        RetrievalIndexKind::Vector => Err(std::io::Error::other(format!(
            "VECTOR index supports USING HNSW only; found `{engine}`"
        ))
        .into()),
        RetrievalIndexKind::Text => Err(std::io::Error::other(format!(
            "TEXT index supports USING FTS5 only; found `{engine}`"
        ))
        .into()),
    }
}

fn parse_retrieval_index_ddl(statement: &str) -> Result<Option<RetrievalIndexDdl>, String> {
    let trimmed = statement.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let stripped = trimmed.trim_end_matches(';').trim();
    if stripped.is_empty() {
        return Ok(None);
    }

    let tokens = tokenize_ddl_statement(stripped);
    if tokens.is_empty() {
        return Ok(None);
    }

    match tokens[0].to_ascii_uppercase().as_str() {
        "CREATE" => {
            if tokens.len() >= 3
                && matches!(tokens[1].to_ascii_uppercase().as_str(), "VECTOR" | "TEXT")
                && tokens[2].eq_ignore_ascii_case("INDEX")
            {
                parse_create_retrieval_index_ddl(&tokens).map(Some)
            } else {
                Ok(None)
            }
        }
        "DROP" => {
            if tokens.len() >= 3
                && matches!(tokens[1].to_ascii_uppercase().as_str(), "VECTOR" | "TEXT")
                && tokens[2].eq_ignore_ascii_case("INDEX")
            {
                parse_drop_retrieval_index_ddl(&tokens).map(Some)
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

fn parse_create_retrieval_index_ddl(tokens: &[String]) -> Result<RetrievalIndexDdl, String> {
    let mut i = 1usize;
    let kind = RetrievalIndexKind::parse(token_at(tokens, i, "index kind")?)?;
    i += 1;
    expect_keyword(tokens, i, "INDEX")?;
    i += 1;

    let mut if_not_exists = false;
    if has_keywords(tokens, i, &["IF", "NOT", "EXISTS"]) {
        if_not_exists = true;
        i += 3;
    }

    let name = validate_identifier(token_at(tokens, i, "index name")?, "index name")?;
    i += 1;

    expect_keyword(tokens, i, "ON")?;
    i += 1;

    let table_name = validate_identifier(token_at(tokens, i, "table name")?, "table name")?;
    i += 1;
    expect_symbol(tokens, i, "(")?;
    i += 1;
    let column_name = validate_identifier(token_at(tokens, i, "column name")?, "column name")?;
    i += 1;
    expect_symbol(tokens, i, ")")?;
    i += 1;

    expect_keyword(tokens, i, "USING")?;
    i += 1;
    let using_engine = validate_identifier(token_at(tokens, i, "USING engine")?, "USING engine")?;
    i += 1;

    let mut options = Value::Object(Map::new());
    if i < tokens.len() {
        expect_keyword(tokens, i, "WITH")?;
        i += 1;
        let (parsed, consumed) = parse_with_options(&tokens[i..])?;
        options = parsed;
        i += consumed;
    }

    if i != tokens.len() {
        return Err(format!(
            "unexpected trailing tokens in retrieval index DDL: {}",
            tokens[i..].join(" ")
        ));
    }

    Ok(RetrievalIndexDdl::Create(CreateRetrievalIndexDdl {
        kind,
        if_not_exists,
        name,
        table_name,
        column_name,
        using_engine,
        options,
    }))
}

fn parse_drop_retrieval_index_ddl(tokens: &[String]) -> Result<RetrievalIndexDdl, String> {
    let mut i = 1usize;
    let kind = RetrievalIndexKind::parse(token_at(tokens, i, "index kind")?)?;
    i += 1;
    expect_keyword(tokens, i, "INDEX")?;
    i += 1;

    let mut if_exists = false;
    if has_keywords(tokens, i, &["IF", "EXISTS"]) {
        if_exists = true;
        i += 2;
    }

    let name = validate_identifier(token_at(tokens, i, "index name")?, "index name")?;
    i += 1;

    if i != tokens.len() {
        return Err(format!(
            "unexpected trailing tokens in DROP retrieval index DDL: {}",
            tokens[i..].join(" ")
        ));
    }

    Ok(RetrievalIndexDdl::Drop(DropRetrievalIndexDdl {
        kind,
        if_exists,
        name,
    }))
}

fn parse_with_options(tokens: &[String]) -> Result<(Value, usize), String> {
    let mut i = 0usize;
    expect_symbol(tokens, i, "(")?;
    i += 1;

    let mut options = Map::new();
    while i < tokens.len() {
        if tokens[i] == ")" {
            i += 1;
            return Ok((Value::Object(options), i));
        }

        let key = validate_identifier(token_at(tokens, i, "option key")?, "option key")?;
        i += 1;
        expect_symbol(tokens, i, "=")?;
        i += 1;
        let value_token = token_at(tokens, i, "option value")?;
        i += 1;
        options.insert(key, parse_option_value(value_token));

        if i < tokens.len() && tokens[i] == "," {
            i += 1;
        }
    }

    Err("unterminated WITH (...) clause in retrieval index DDL".to_string())
}

fn parse_option_value(raw: &str) -> Value {
    if raw.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if raw.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    if let Ok(value) = raw.parse::<i64>() {
        return Value::Number(value.into());
    }
    if let Ok(value) = raw.parse::<f64>() {
        return Value::from(value);
    }

    if (raw.starts_with('\'') && raw.ends_with('\''))
        || (raw.starts_with('"') && raw.ends_with('"'))
    {
        let unquoted = raw[1..raw.len().saturating_sub(1)].to_string();
        return Value::String(unquoted);
    }

    Value::String(raw.to_string())
}

fn tokenize_ddl_statement(statement: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for ch in statement.chars() {
        if let Some(active_quote) = quote {
            current.push(ch);
            if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                current.push(ch);
                quote = Some(ch);
            }
            '(' | ')' | ',' | '=' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                tokens.push(ch.to_string());
            }
            ';' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn token_at<'a>(tokens: &'a [String], index: usize, label: &str) -> Result<&'a str, String> {
    tokens
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("missing {label} in retrieval index DDL"))
}

fn expect_keyword(tokens: &[String], index: usize, keyword: &str) -> Result<(), String> {
    let token = token_at(tokens, index, keyword)?;
    if token.eq_ignore_ascii_case(keyword) {
        Ok(())
    } else {
        Err(format!(
            "expected keyword `{keyword}` but found `{token}` in retrieval index DDL"
        ))
    }
}

fn expect_symbol(tokens: &[String], index: usize, symbol: &str) -> Result<(), String> {
    let token = token_at(tokens, index, symbol)?;
    if token == symbol {
        Ok(())
    } else {
        Err(format!(
            "expected symbol `{symbol}` but found `{token}` in retrieval index DDL"
        ))
    }
}

fn has_keywords(tokens: &[String], start: usize, keywords: &[&str]) -> bool {
    keywords.iter().enumerate().all(|(offset, keyword)| {
        tokens
            .get(start + offset)
            .is_some_and(|token| token.eq_ignore_ascii_case(keyword))
    })
}

fn validate_identifier(raw: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(format!("{label} cannot be empty"));
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Ok(value.to_string());
    }
    Err(format!(
        "invalid {label} `{value}`; expected letters, numbers, or underscore"
    ))
}

fn sqlite_quote_identifier(raw: &str) -> Option<String> {
    if raw
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(format!("\"{}\"", raw.replace('"', "\"\"")))
    } else {
        None
    }
}

fn try_execute_explain_retrieval(
    conn: &Connection,
    statement: &str,
) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    let trimmed = statement.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("EXPLAIN RETRIEVAL") {
        return Ok(None);
    }

    let query = trimmed["EXPLAIN RETRIEVAL".len()..]
        .trim()
        .trim_end_matches(';')
        .trim();
    if query.is_empty() {
        return Err(std::io::Error::other(
            "EXPLAIN RETRIEVAL requires a SQL query after the keyword",
        )
        .into());
    }

    ensure_retrieval_index_catalog(conn)?;
    Ok(Some(build_explain_retrieval_payload(conn, query)))
}

fn build_explain_retrieval_payload(conn: &Connection, query: &str) -> Value {
    let rewritten = rewrite_sql_vector_operators(query);
    let lowered = rewritten.to_ascii_lowercase();

    let uses_vector = query.contains("<->")
        || query.contains("<=>")
        || query.contains("<#>")
        || lowered.contains("l2_distance(")
        || lowered.contains("cosine_distance(")
        || lowered.contains("neg_inner_product(")
        || lowered.contains(" vector(");
    let uses_text = lowered.contains("bm25_score(")
        || lowered.contains("chunks_fts")
        || lowered.contains(" match ");
    let uses_hybrid = lowered.contains("hybrid_score(") || (uses_vector && uses_text);

    let vector_index_count = query_retrieval_index_count(conn, "vector");
    let text_index_count = query_retrieval_index_count(conn, "text");

    let vector_path = if uses_vector {
        if vector_index_count > 0 {
            "ann_index"
        } else {
            "brute_force_fallback"
        }
    } else {
        "not_used"
    };

    let text_path = if uses_text {
        if lowered.contains("chunks_fts") || text_index_count > 0 {
            "fts_index_or_bm25"
        } else {
            "lexical_fallback"
        }
    } else {
        "not_used"
    };

    let order_by_clause = extract_order_by_clause(&rewritten);
    let has_order_by = order_by_clause.is_some();
    let has_explicit_tie_break = order_by_clause
        .as_ref()
        .is_some_and(|clause| has_deterministic_tie_break(clause));

    let fusion_mode = if lowered.contains("hybrid_score(") {
        "hybrid_score"
    } else if uses_vector && uses_text {
        "implicit_weighted"
    } else if uses_vector {
        "vector_only"
    } else if uses_text {
        "text_only"
    } else {
        "none"
    };

    let hybrid_alpha = parse_function_numeric_arg(&rewritten, "hybrid_score", 2);

    let sqlite_query_plan = match capture_sqlite_query_plan(conn, &rewritten) {
        Ok(rows) => Value::Array(rows),
        Err(error) => json!({
            "error": error.to_string()
        }),
    };

    let mut notes = Vec::new();
    if uses_vector && vector_path == "brute_force_fallback" {
        notes.push(
            "Vector ANN path unavailable; planner will use brute-force fallback scoring."
                .to_string(),
        );
    }
    if !has_order_by {
        notes.push(
            "No ORDER BY clause detected; repeated runs may not be deterministic.".to_string(),
        );
    } else if !has_explicit_tie_break {
        notes.push(
            "ORDER BY does not include an explicit id/chunk_id tie-break column.".to_string(),
        );
    }

    json!({
        "kind": "retrieval_explain",
        "query": {
            "original": query,
            "rewritten": rewritten,
        },
        "signals": {
            "uses_vector": uses_vector,
            "uses_text": uses_text,
            "uses_hybrid": uses_hybrid,
        },
        "execution_path": {
            "vector": vector_path,
            "text": text_path,
            "index_catalog": {
                "active_vector_indexes": vector_index_count,
                "active_text_indexes": text_index_count,
            }
        },
        "score_attribution": {
            "vector_component": if uses_vector { "enabled" } else { "none" },
            "text_component": if uses_text { "enabled" } else { "none" },
            "fusion_mode": fusion_mode,
            "hybrid_alpha": hybrid_alpha,
        },
        "determinism": {
            "has_order_by": has_order_by,
            "has_explicit_tie_break": has_explicit_tie_break,
            "order_by_clause": order_by_clause,
        },
        "sqlite_query_plan": sqlite_query_plan,
        "notes": notes,
    })
}

fn query_retrieval_index_count(conn: &Connection, index_kind: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM retrieval_indexes WHERE index_kind = ?1 AND status = 'active'",
        [index_kind],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0)
}

fn extract_order_by_clause(statement: &str) -> Option<String> {
    let lowered = statement.to_ascii_lowercase();
    let order_start = lowered.find(" order by ")?;
    let tail = &statement[(order_start + " order by ".len())..];
    let tail_lower = &lowered[(order_start + " order by ".len())..];

    let mut end = tail.len();
    for marker in [" limit ", " offset ", " fetch ", ";"] {
        if let Some(idx) = tail_lower.find(marker) {
            end = end.min(idx);
        }
    }

    let clause = tail[..end].trim();
    if clause.is_empty() {
        None
    } else {
        Some(clause.to_string())
    }
}

fn has_deterministic_tie_break(order_by_clause: &str) -> bool {
    let normalized = order_by_clause.to_ascii_lowercase();
    normalized
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|token| matches!(token, "id" | "chunk_id" | "rowid"))
}

fn parse_function_numeric_arg(
    statement: &str,
    function_name: &str,
    arg_index: usize,
) -> Option<f64> {
    let args = parse_function_arguments(statement, function_name)?;
    args.get(arg_index)?.trim().parse::<f64>().ok()
}

fn parse_function_arguments(statement: &str, function_name: &str) -> Option<Vec<String>> {
    let lowered = statement.to_ascii_lowercase();
    let needle = format!("{}(", function_name.to_ascii_lowercase());
    let found = lowered.find(&needle)?;
    let open_idx = found + function_name.len();
    let bytes = statement.as_bytes();

    let mut depth = 1usize;
    let mut idx = open_idx + 1;
    let mut quote: Option<u8> = None;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if let Some(active_quote) = quote {
            if byte == active_quote {
                if idx + 1 < bytes.len() && bytes[idx + 1] == active_quote {
                    idx += 2;
                    continue;
                }
                quote = None;
            }
            idx += 1;
            continue;
        }

        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            idx += 1;
            continue;
        }
        if byte == b'(' {
            depth += 1;
            idx += 1;
            continue;
        }
        if byte == b')' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                let raw = &statement[(open_idx + 1)..idx];
                return Some(split_top_level_sql_args(raw));
            }
            idx += 1;
            continue;
        }
        idx += 1;
    }

    None
}

fn split_top_level_sql_args(raw: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;

    for ch in raw.chars() {
        if let Some(active_quote) = quote {
            current.push(ch);
            if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '\'' | '"' => {
                current.push(ch);
                quote = Some(ch);
            }
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }

    args
}

fn capture_sqlite_query_plan(conn: &Connection, query: &str) -> Result<Vec<Value>, SqlError> {
    let explain_sql = format!("EXPLAIN QUERY PLAN {}", query.trim_end_matches(';'));
    let mut stmt = conn.prepare(&explain_sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let parent: i64 = row.get(1)?;
        let detail: String = row.get(3)?;
        out.push(json!({
            "id": id,
            "parent": parent,
            "detail": detail,
        }));
    }
    Ok(out)
}

fn execute_sql_statement(
    conn: &Connection,
    statement: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(explain) = try_execute_explain_retrieval(conn, statement)? {
        println!("{}", serde_json::to_string_pretty(&explain)?);
        return Ok(());
    }

    if let Some(message) = try_execute_retrieval_index_ddl(conn, statement)? {
        println!("{message}");
        return Ok(());
    }

    let rewritten = rewrite_sql_vector_operators(statement);

    if is_query_statement(&rewritten) {
        let mut stmt = conn.prepare(&rewritten)?;
        let column_count = stmt.column_count();
        let column_names = stmt
            .column_names()
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>();

        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let mut object = Map::new();
            for idx in 0..column_count {
                let key = column_names
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| format!("col_{idx}"));
                let value = sql_value_to_json(row.get_ref(idx)?);
                object.insert(key, value);
            }
            out.push(Value::Object(object));
        }
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        conn.execute_batch(&rewritten)?;
        println!("statement executed");
    }
    Ok(())
}

fn run_sql_repl(conn: &Connection, db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("sqlrite interactive SQL shell");
    println!("- db={}", db_path.display());
    println!("- type .help for shell commands, .exit to quit");

    let stdin = io::stdin();
    let mut line = String::new();
    loop {
        print!("sqlrite> ");
        io::stdout().flush()?;

        line.clear();
        let read = stdin.read_line(&mut line)?;
        if read == 0 {
            println!();
            break;
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        if input == ".quit" || input == ".exit" {
            break;
        }
        if input == ".help" {
            println!("{}", sql_repl_help());
            continue;
        }
        if input == ".tables" {
            execute_sql_statement(
                conn,
                "SELECT name FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name;",
            )?;
            continue;
        }
        if let Some(rest) = input.strip_prefix(".schema") {
            let target = rest.trim();
            let sql = if target.is_empty() {
                "SELECT name, sql FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name;"
                    .to_string()
            } else {
                let escaped = target.replace('\'', "''");
                format!(
                    "SELECT name, sql FROM sqlite_master WHERE name = '{}' ORDER BY name;",
                    escaped
                )
            };
            execute_sql_statement(conn, &sql)?;
            continue;
        }
        if let Some(rest) = input.strip_prefix(".example") {
            let tokens: Vec<&str> = rest.split_whitespace().collect();
            if tokens.is_empty() {
                println!("{}", sql_repl_example_catalog());
                continue;
            }
            let example_name = tokens[0];
            let should_run = tokens.iter().skip(1).any(|token| *token == "--run");
            if let Some(example_sql) = sql_retrieval_example(example_name) {
                println!("-- example: {example_name}\n{example_sql}");
                if should_run {
                    execute_sql_statement(conn, example_sql)?;
                }
            } else {
                println!("unknown example `{example_name}`");
                println!("{}", sql_repl_example_catalog());
            }
            continue;
        }

        if let Err(error) = execute_sql_statement(conn, input) {
            eprintln!("error: {error}");
        }
    }

    Ok(())
}

fn sql_repl_help() -> &'static str {
    "shell commands:
  .help                 Show shell help
  .tables               List tables/views
  .schema [table]       Show schema for all or one table
  .example              List retrieval example names
  .example <name>       Print retrieval example SQL
  .example <name> --run Execute retrieval example SQL
  .exit | .quit         Exit shell"
}

fn sql_repl_example_catalog() -> &'static str {
    "available examples:
  lexical   FTS/BM25 lexical retrieval
  vector    SQL-native vector operator retrieval
  hybrid    embed + bm25_score + hybrid_score ranking
  filter    metadata-filtered retrieval
  doc_scope doc-scoped retrieval
  rerank_ready produce vector/text signals for external rerankers
  explain   EXPLAIN RETRIEVAL output for hybrid query
  vector_ddl create VECTOR INDEX metadata entry
  index_catalog inspect retrieval index catalog
  tenant    tenant-scoped filtered query
  recent    recent chunks for operational debugging"
}

fn sql_retrieval_example(name: &str) -> Option<&'static str> {
    match name {
        "lexical" => Some(
            "SELECT c.id, c.doc_id, c.content
FROM chunks_fts AS f
JOIN chunks AS c ON c.id = f.chunk_id
WHERE chunks_fts MATCH 'local OR agent'
ORDER BY bm25(chunks_fts) ASC, c.id ASC
LIMIT 5;",
        ),
        "tenant" => Some(
            "SELECT id, doc_id, json_extract(metadata, '$.tenant') AS tenant
FROM chunks
WHERE json_extract(metadata, '$.tenant') = 'demo'
ORDER BY id ASC
LIMIT 10;",
        ),
        "vector" => Some(
            "SELECT id,
       embedding <-> vector('0.95,0.05,0.0') AS l2,
       embedding <=> vector('0.95,0.05,0.0') AS cosine_distance,
       embedding <#> vector('0.95,0.05,0.0') AS neg_inner
FROM chunks
ORDER BY l2 ASC, id ASC
LIMIT 5;",
        ),
        "hybrid" => Some(
            "SELECT id,
       1.0 - cosine_distance(embedding, embed('local-first agent memory')) AS vector_score,
       bm25_score('local-first agent memory', content) AS text_score,
       hybrid_score(
           1.0 - cosine_distance(embedding, embed('local-first agent memory')),
           bm25_score('local-first agent memory', content),
           0.65
       ) AS hybrid
FROM chunks
ORDER BY hybrid DESC, id ASC
LIMIT 5;",
        ),
        "filter" => Some(
            "SELECT id, doc_id, content
FROM chunks
WHERE json_extract(metadata, '$.topic') = 'retrieval'
ORDER BY id ASC
LIMIT 10;",
        ),
        "doc_scope" => Some(
            "SELECT id, doc_id, content
FROM chunks
WHERE doc_id = 'doc-a'
ORDER BY id ASC
LIMIT 10;",
        ),
        "rerank_ready" => Some(
            "SELECT id,
       content,
       1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score,
       bm25_score('local agent memory', content) AS text_score
FROM chunks
ORDER BY vector_score DESC, text_score DESC, id ASC
LIMIT 20;",
        ),
        "explain" => Some(
            "EXPLAIN RETRIEVAL
SELECT id,
       1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score,
       bm25_score('local agent memory', content) AS text_score,
       hybrid_score(
           1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')),
           bm25_score('local agent memory', content),
           0.65
       ) AS hybrid
FROM chunks
ORDER BY hybrid DESC, id ASC
LIMIT 5;",
        ),
        "vector_ddl" => Some(
            "CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw
ON chunks(embedding)
USING HNSW
WITH (m=16, ef_construction=64);",
        ),
        "index_catalog" => Some(
            "SELECT name, index_kind, table_name, column_name, using_engine, options_json, status
FROM retrieval_index_catalog
ORDER BY name;",
        ),
        "recent" => Some(
            "SELECT id, doc_id, created_at
FROM chunks
ORDER BY created_at DESC, rowid DESC
LIMIT 10;",
        ),
        _ => None,
    }
}

fn is_query_statement(sql: &str) -> bool {
    let normalized = sql.trim_start().to_ascii_lowercase();
    normalized.starts_with("select")
        || normalized.starts_with("with")
        || normalized.starts_with("pragma")
        || normalized.starts_with("explain")
}

fn sql_value_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(v) => json!(v),
        ValueRef::Real(v) => json!(v),
        ValueRef::Text(bytes) => Value::String(String::from_utf8_lossy(bytes).to_string()),
        ValueRef::Blob(bytes) => Value::String(format!("blob:{}bytes", bytes.len())),
    }
}

fn usage() -> &'static str {
    "sqlrite unified CLI\n\nusage:\n  sqlrite <command> [options]\n\ncommands:\n  init       Create/open a SQLRite database and apply runtime profile\n  sql        Execute SQL or enter interactive SQL shell\n  ingest     Ingest a single chunk directly from CLI flags\n  query      Run text/vector/hybrid retrieval query\n  quickstart Run init->query UX flow with telemetry/gates\n  serve      Start health/metrics HTTP server\n  backup     Create or verify backup files\n  compact    Run ingestion-compaction maintenance workflow\n  benchmark  Run synthetic retrieval benchmark\n  doctor     Run environment and database health checks\n\nenv overrides:\n  SQLRITE_VECTOR_STORAGE=f32|f16|int8\n  SQLRITE_ANN_MIN_CANDIDATES=<int>\n  SQLRITE_ANN_MAX_HAMMING_RADIUS=<int>\n  SQLRITE_ANN_MAX_CANDIDATE_MULTIPLIER=<int>\n  SQLRITE_ENABLE_ANN_PERSISTENCE=true|false\n  SQLRITE_SQLITE_MMAP_SIZE=<bytes>\n  SQLRITE_SQLITE_CACHE_SIZE_KIB=<kib>\n\nexamples:\n  sqlrite init --db sqlrite_demo.db --seed-demo\n  sqlrite quickstart --db sqlrite_demo.db --runs 5 --max-median-ms 180000 --min-success-rate 0.95\n  sqlrite query --db sqlrite_demo.db --text \"agents local memory\" --top-k 3\n  sqlrite compact --db sqlrite_demo.db --json\n  sqlrite doctor --db sqlrite_demo.db --json\n  sqlrite sql --db sqlrite_demo.db\n  sqlrite sql --db sqlrite_demo.db --execute \"SELECT id, doc_id FROM chunks LIMIT 3;\""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quickstart_args_default_to_seeded_query_flow() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse_quickstart_args(&[]).map_err(std::io::Error::other)?;
        assert_eq!(parsed.db_path, PathBuf::from("sqlrite_quickstart.db"));
        assert!(parsed.seed_demo);
        assert!(parsed.reset_db);
        assert_eq!(parsed.runs, 1);
        assert_eq!(parsed.query_text.as_deref(), Some("agents local memory"));
        Ok(())
    }

    #[test]
    fn quickstart_args_parse_thresholds() -> Result<(), Box<dyn std::error::Error>> {
        let args = vec![
            "--runs".to_string(),
            "7".to_string(),
            "--min-success-rate".to_string(),
            "0.95".to_string(),
            "--max-median-ms".to_string(),
            "180000".to_string(),
            "--fusion".to_string(),
            "rrf".to_string(),
            "--rrf-k".to_string(),
            "42".to_string(),
        ];
        let parsed = parse_quickstart_args(&args).map_err(std::io::Error::other)?;
        assert_eq!(parsed.runs, 7);
        assert_eq!(parsed.min_success_rate, Some(0.95));
        assert_eq!(parsed.max_median_ms, Some(180000.0));
        assert_eq!(parsed.fusion_mode, "rrf");
        assert_eq!(parsed.rrf_rank_constant, 42.0);
        Ok(())
    }

    #[test]
    fn median_handles_even_and_odd_inputs() {
        assert_eq!(median(&[1.0, 2.0, 3.0]), 2.0);
        assert_eq!(median(&[1.0, 3.0, 2.0, 5.0]), 2.5);
    }

    #[test]
    fn path_parent_writable_detects_temp_dir() {
        let path = std::env::temp_dir().join("sqlrite-parent-writable-test.db");
        assert!(path_parent_writable(&path));
    }

    #[test]
    fn parse_index_mode_accepts_hnsw_aliases() {
        assert!(matches!(
            parse_index_mode("hnsw_baseline"),
            Ok(VectorIndexMode::HnswBaseline)
        ));
        assert!(matches!(
            parse_index_mode("hnsw"),
            Ok(VectorIndexMode::HnswBaseline)
        ));
    }

    #[test]
    fn parse_boolish_supports_common_values() {
        assert_eq!(parse_boolish("true"), Some(true));
        assert_eq!(parse_boolish("1"), Some(true));
        assert_eq!(parse_boolish("false"), Some(false));
        assert_eq!(parse_boolish("0"), Some(false));
        assert_eq!(parse_boolish("not-a-bool"), None);
    }

    #[test]
    fn compact_args_defaults_to_safe_maintenance_actions() -> Result<(), Box<dyn std::error::Error>>
    {
        let parsed = parse_compact_args(&[]).map_err(std::io::Error::other)?;
        assert!(parsed.dedupe_by_content_hash);
        assert!(parsed.prune_orphan_documents);
        assert!(parsed.wal_checkpoint_truncate);
        assert!(parsed.analyze);
        assert!(parsed.vacuum);
        Ok(())
    }

    #[test]
    fn compact_args_support_disabling_actions() -> Result<(), Box<dyn std::error::Error>> {
        let args = vec![
            "--no-dedupe-by-content-hash".to_string(),
            "--no-prune-orphan-documents".to_string(),
            "--no-wal-checkpoint".to_string(),
            "--no-analyze".to_string(),
            "--no-vacuum".to_string(),
            "--json".to_string(),
        ];
        let parsed = parse_compact_args(&args).map_err(std::io::Error::other)?;
        assert!(!parsed.dedupe_by_content_hash);
        assert!(!parsed.prune_orphan_documents);
        assert!(!parsed.wal_checkpoint_truncate);
        assert!(!parsed.analyze);
        assert!(!parsed.vacuum);
        assert!(parsed.json_output);
        Ok(())
    }

    #[test]
    fn benchmark_args_default_concurrency_is_one() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse_benchmark_args(&[]).map_err(std::io::Error::other)?;
        assert_eq!(parsed.config.concurrency, 1);
        Ok(())
    }

    #[test]
    fn benchmark_args_parse_concurrency() -> Result<(), Box<dyn std::error::Error>> {
        let args = vec![
            "--corpus".to_string(),
            "3000".to_string(),
            "--queries".to_string(),
            "200".to_string(),
            "--concurrency".to_string(),
            "4".to_string(),
        ];
        let parsed = parse_benchmark_args(&args).map_err(std::io::Error::other)?;
        assert_eq!(parsed.config.concurrency, 4);
        assert_eq!(parsed.config.corpus_size, 3000);
        assert_eq!(parsed.config.query_count, 200);
        Ok(())
    }

    #[test]
    fn vector_operator_rewrite_maps_to_distance_functions() {
        let sql = "SELECT embedding <-> vector('1,0,0') AS l2, embedding <=> vector('1,0,0') AS cd, embedding <#> vector('1,0,0') AS nip FROM chunks;";
        let rewritten = rewrite_sql_vector_operators(sql);
        assert!(rewritten.contains("l2_distance(embedding, vector('1,0,0'))"));
        assert!(rewritten.contains("cosine_distance(embedding, vector('1,0,0'))"));
        assert!(rewritten.contains("neg_inner_product(embedding, vector('1,0,0'))"));
    }

    #[test]
    fn vector_operator_rewrite_ignores_literals_and_comments() {
        let sql = "SELECT '<->' AS marker, embedding <-> vector('1,0,0') -- <=> <#>\nFROM chunks;";
        let rewritten = rewrite_sql_vector_operators(sql);
        assert!(rewritten.contains("'<->' AS marker"));
        assert!(rewritten.contains("-- <=> <#>"));
        assert!(rewritten.contains("l2_distance(embedding, vector('1,0,0'))"));
    }

    #[test]
    fn retrieval_sql_functions_compute_expected_distances() -> Result<(), Box<dyn std::error::Error>>
    {
        let conn = Connection::open_in_memory()?;
        register_retrieval_sql_functions(&conn)?;

        let rewritten = rewrite_sql_vector_operators(
            "SELECT vector('1,0,0') <-> vector('0,1,0') AS l2, vector('1,0,0') <=> vector('0,1,0') AS cosine, vector('1,0,0') <#> vector('0,1,0') AS neg_ip;",
        );
        let (l2, cosine, neg_ip): (f64, f64, f64) = conn.query_row(&rewritten, [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;

        assert!((l2 - (2.0f64).sqrt()).abs() < 1e-6);
        assert!((cosine - 1.0).abs() < 1e-6);
        assert!(neg_ip.abs() < 1e-6);

        let dims: i64 =
            conn.query_row("SELECT vec_dims(vector('[1,2,3]'))", [], |row| row.get(0))?;
        assert_eq!(dims, 3);
        let as_json: String =
            conn.query_row("SELECT vec_to_json(vector('[1,2,3]'))", [], |row| {
                row.get(0)
            })?;
        assert_eq!(as_json, "[1.0,2.0,3.0]");
        Ok(())
    }

    #[test]
    fn retrieval_sql_functions_embed_bm25_and_hybrid() -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        register_retrieval_sql_functions(&conn)?;

        let (dims, bm25, hybrid): (i64, f64, f64) = conn.query_row(
            "SELECT
                vec_dims(embed('agent local memory')) AS dims,
                bm25_score('agent memory', 'agent systems keep local memory') AS bm25,
                hybrid_score(0.8, 0.2, 0.75) AS hybrid;",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        assert_eq!(dims, 16);
        assert!(bm25 > 0.0);
        assert!((hybrid - 0.65).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn retrieval_index_ddl_create_and_drop() -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        register_retrieval_sql_functions(&conn)?;
        conn.execute_batch(
            "
            CREATE TABLE chunks (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                embedding BLOB NOT NULL
            );
            ",
        )?;

        execute_sql_statement(
            &conn,
            "CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw
             ON chunks(embedding)
             USING HNSW
             WITH (m=16, ef_construction=64);",
        )?;
        execute_sql_statement(
            &conn,
            "CREATE TEXT INDEX IF NOT EXISTS idx_chunks_content_fts
             ON chunks(content)
             USING FTS5;",
        )?;

        let (kind, engine, options_json): (String, String, String) = conn.query_row(
            "SELECT index_kind, using_engine, options_json
             FROM retrieval_index_catalog
             WHERE name = 'idx_chunks_embedding_hnsw';",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(kind, "vector");
        assert_eq!(engine, "hnsw");
        let options: Value = serde_json::from_str(&options_json)?;
        assert_eq!(options["m"], 16);
        assert_eq!(options["ef_construction"], 64);

        let count_before_drop: i64 =
            conn.query_row("SELECT COUNT(*) FROM retrieval_index_catalog", [], |row| {
                row.get(0)
            })?;
        assert_eq!(count_before_drop, 2);

        execute_sql_statement(&conn, "DROP VECTOR INDEX idx_chunks_embedding_hnsw;")?;
        let count_after_drop: i64 = conn.query_row(
            "SELECT COUNT(*) FROM retrieval_index_catalog WHERE name = 'idx_chunks_embedding_hnsw'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count_after_drop, 0);
        Ok(())
    }

    #[test]
    fn explain_retrieval_reports_bruteforce_path_and_score_attribution()
    -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        register_retrieval_sql_functions(&conn)?;
        conn.execute_batch(
            "
            CREATE TABLE chunks (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                embedding BLOB NOT NULL
            );
            ",
        )?;

        let explain = try_execute_explain_retrieval(
            &conn,
            "EXPLAIN RETRIEVAL
             SELECT id,
                    1.0 - cosine_distance(embedding, vector('1,0')) AS vector_score,
                    bm25_score('agent memory', content) AS text_score,
                    hybrid_score(
                        1.0 - cosine_distance(embedding, vector('1,0')),
                        bm25_score('agent memory', content),
                        0.7
                    ) AS hybrid
             FROM chunks
             ORDER BY hybrid DESC, id ASC
             LIMIT 5;",
        )?
        .expect("expected EXPLAIN RETRIEVAL payload");

        assert_eq!(explain["execution_path"]["vector"], "brute_force_fallback");
        assert_eq!(explain["execution_path"]["text"], "lexical_fallback");
        assert_eq!(explain["score_attribution"]["fusion_mode"], "hybrid_score");
        assert_eq!(explain["score_attribution"]["hybrid_alpha"], 0.7);
        assert_eq!(explain["determinism"]["has_explicit_tie_break"], true);
        Ok(())
    }

    #[test]
    fn explain_retrieval_reports_ann_path_when_vector_index_exists()
    -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        register_retrieval_sql_functions(&conn)?;
        conn.execute_batch(
            "
            CREATE TABLE chunks (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                embedding BLOB NOT NULL
            );
            ",
        )?;

        execute_sql_statement(
            &conn,
            "CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw
             ON chunks(embedding)
             USING HNSW;",
        )?;

        let explain = try_execute_explain_retrieval(
            &conn,
            "EXPLAIN RETRIEVAL
             SELECT id,
                    1.0 - cosine_distance(embedding, vector('1,0')) AS vector_score
             FROM chunks
             ORDER BY vector_score DESC, id ASC
             LIMIT 5;",
        )?
        .expect("expected EXPLAIN RETRIEVAL payload");

        assert_eq!(explain["execution_path"]["vector"], "ann_index");
        assert_eq!(
            explain["execution_path"]["index_catalog"]["active_vector_indexes"],
            1
        );
        Ok(())
    }

    #[test]
    fn explain_retrieval_returns_none_for_non_explain_statement()
    -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        let explain = try_execute_explain_retrieval(&conn, "SELECT 1;")?;
        assert!(explain.is_none());
        Ok(())
    }

    #[test]
    fn parse_retrieval_index_ddl_ignores_non_retrieval_create()
    -> Result<(), Box<dyn std::error::Error>> {
        let ddl = parse_retrieval_index_ddl("CREATE TABLE t (id INTEGER);")
            .map_err(std::io::Error::other)?;
        assert!(ddl.is_none());
        Ok(())
    }
}
