use sqlrite::{
    CustomHttpEmbeddingProvider, DeterministicEmbeddingProvider, OpenAiCompatibleEmbeddingProvider,
    ReindexOptions, RuntimeConfig, SqlRite, reindex_embeddings,
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args =
        parse_args(std::env::args().skip(1).collect::<Vec<_>>()).map_err(std::io::Error::other)?;
    let db = SqlRite::open_with_config(&args.db_path, RuntimeConfig::default())?;

    let mut options = ReindexOptions {
        batch_size: args.batch_size,
        tenant_id: args.tenant,
        target_model_version: args.target_model_version.clone(),
        only_if_model_mismatch: !args.force_all,
        continue_on_partial_failure: args.continue_on_partial_failure,
        checkpoint_path: args.checkpoint_path,
        ..ReindexOptions::default()
    };

    let report = match args.provider.as_str() {
        "openai" => {
            let endpoint = args.endpoint.ok_or_else(|| {
                std::io::Error::other("--endpoint is required for openai provider")
            })?;
            let model = args
                .model
                .ok_or_else(|| std::io::Error::other("--model is required for openai provider"))?;
            let api_key_env = args
                .api_key_env
                .unwrap_or_else(|| "OPENAI_API_KEY".to_string());
            let provider =
                OpenAiCompatibleEmbeddingProvider::from_env(endpoint, model, &api_key_env)?;
            reindex_embeddings(&db, provider, options)?
        }
        "custom" => {
            let endpoint = args.endpoint.ok_or_else(|| {
                std::io::Error::other("--endpoint is required for custom provider")
            })?;
            let mut provider =
                CustomHttpEmbeddingProvider::new(endpoint, &args.target_model_version)?;
            if let Some(model) = args.model {
                provider = provider.with_model(model);
            }
            for (key, value) in &args.headers {
                provider = provider.with_header(key.clone(), value.clone());
            }
            if let (Some(input_field), Some(embeddings_field)) =
                (args.input_field, args.embeddings_field)
            {
                provider = provider.with_fields(input_field, embeddings_field);
            }
            reindex_embeddings(&db, provider, options)?
        }
        _ => {
            let embedding_dim = db
                .vector_index_stats()
                .and_then(|stats| stats.dimension)
                .unwrap_or(args.embedding_dim);
            options.target_model_version = args.target_model_version;
            let provider = DeterministicEmbeddingProvider::new(
                embedding_dim,
                options.target_model_version.clone(),
            )?;
            reindex_embeddings(&db, provider, options)?
        }
    };

    println!("reindex complete");
    println!(
        "scanned={}, updated={}, skipped={}, failed={}, resumed_from={}",
        report.scanned_chunks,
        report.updated_chunks,
        report.skipped_chunks,
        report.failed_chunks,
        report.resumed_from_offset
    );
    println!(
        "provider={} model={}",
        report.provider, report.model_version
    );
    Ok(())
}

#[derive(Debug)]
struct Args {
    db_path: PathBuf,
    provider: String,
    endpoint: Option<String>,
    model: Option<String>,
    api_key_env: Option<String>,
    input_field: Option<String>,
    embeddings_field: Option<String>,
    headers: Vec<(String, String)>,
    target_model_version: String,
    embedding_dim: usize,
    batch_size: usize,
    tenant: Option<String>,
    force_all: bool,
    continue_on_partial_failure: bool,
    checkpoint_path: Option<PathBuf>,
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut out = Args {
        db_path: PathBuf::from("sqlrite_demo.db"),
        provider: "deterministic".to_string(),
        endpoint: None,
        model: None,
        api_key_env: None,
        input_field: None,
        embeddings_field: None,
        headers: Vec::new(),
        target_model_version: "det-v2".to_string(),
        embedding_dim: 256,
        batch_size: 256,
        tenant: None,
        force_all: false,
        continue_on_partial_failure: false,
        checkpoint_path: None,
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                out.db_path = PathBuf::from(parse_string(&args, i, "--db")?);
            }
            "--provider" => {
                i += 1;
                out.provider = parse_string(&args, i, "--provider")?;
            }
            "--endpoint" => {
                i += 1;
                out.endpoint = Some(parse_string(&args, i, "--endpoint")?);
            }
            "--model" => {
                i += 1;
                out.model = Some(parse_string(&args, i, "--model")?);
            }
            "--api-key-env" => {
                i += 1;
                out.api_key_env = Some(parse_string(&args, i, "--api-key-env")?);
            }
            "--input-field" => {
                i += 1;
                out.input_field = Some(parse_string(&args, i, "--input-field")?);
            }
            "--embeddings-field" => {
                i += 1;
                out.embeddings_field = Some(parse_string(&args, i, "--embeddings-field")?);
            }
            "--header" => {
                i += 1;
                let raw = parse_string(&args, i, "--header")?;
                let Some((key, value)) = raw.split_once(':') else {
                    return Err(format!("invalid --header `{raw}`, expected key:value"));
                };
                out.headers
                    .push((key.trim().to_string(), value.trim().to_string()));
            }
            "--target-model-version" => {
                i += 1;
                out.target_model_version = parse_string(&args, i, "--target-model-version")?;
            }
            "--embedding-dim" => {
                i += 1;
                out.embedding_dim = parse_usize(&args, i, "--embedding-dim")?;
            }
            "--batch-size" => {
                i += 1;
                out.batch_size = parse_usize(&args, i, "--batch-size")?;
            }
            "--tenant" => {
                i += 1;
                out.tenant = Some(parse_string(&args, i, "--tenant")?);
            }
            "--checkpoint" => {
                i += 1;
                out.checkpoint_path = Some(PathBuf::from(parse_string(&args, i, "--checkpoint")?));
            }
            "--force-all" => {
                out.force_all = true;
            }
            "--continue-on-partial-failure" => {
                out.continue_on_partial_failure = true;
            }
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown argument `{other}`\n{}", usage())),
        }
        i += 1;
    }

    Ok(out)
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

fn usage() -> String {
    "usage: cargo run --bin sqlrite-reindex -- [--db PATH] [--provider deterministic|openai|custom] [--endpoint URL] [--model MODEL] [--api-key-env ENV] [--input-field FIELD] [--embeddings-field FIELD] [--header key:value] [--target-model-version VERSION] [--embedding-dim N] [--batch-size N] [--tenant TENANT] [--checkpoint PATH] [--force-all] [--continue-on-partial-failure]".to_string()
}
