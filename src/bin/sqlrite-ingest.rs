use sqlrite::{
    ChunkingStrategy, DeterministicEmbeddingProvider, IngestionRequest, IngestionSource,
    IngestionWorker, RuntimeConfig, SqlRite,
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args =
        parse_args(std::env::args().skip(1).collect::<Vec<_>>()).map_err(std::io::Error::other)?;

    let db = SqlRite::open_with_config(&args.db_path, RuntimeConfig::default())?;
    let embedding_dim = db
        .vector_index_stats()
        .and_then(|stats| stats.dimension)
        .unwrap_or(args.embedding_dim);
    let provider = DeterministicEmbeddingProvider::new(embedding_dim, args.model_version)?;
    let worker = if let Some(checkpoint_path) = args.checkpoint_path {
        IngestionWorker::new(&db, provider).with_checkpoint_path(checkpoint_path)
    } else {
        IngestionWorker::new(&db, provider)
    };

    let source = match (args.file_path, args.url, args.direct_content) {
        (Some(path), None, None) => IngestionSource::File { path },
        (None, Some(url), None) => IngestionSource::Url { url },
        (None, None, Some(content)) => IngestionSource::Direct { content },
        _ => {
            return Err(std::io::Error::other(
                "provide exactly one source: --file, --url, or --content",
            )
            .into());
        }
    };

    let request = IngestionRequest {
        job_id: args.job_id,
        doc_id: args.doc_id,
        source_id: args.source_id,
        tenant_id: args.tenant,
        source,
        metadata: serde_json::json!({
            "source_kind": args.source_kind,
        }),
        chunking: match args.chunking_mode.as_str() {
            "fixed" => ChunkingStrategy::Fixed {
                max_chars: args.max_chars,
                overlap_chars: args.overlap_chars,
            },
            "semantic" => ChunkingStrategy::Semantic {
                max_chars: args.max_chars,
            },
            _ => ChunkingStrategy::HeadingAware {
                max_chars: args.max_chars,
                overlap_chars: args.overlap_chars,
            },
        },
        batch_size: args.batch_size,
        continue_on_partial_failure: args.continue_on_partial_failure,
    };

    let report = worker.ingest(request)?;
    println!("SQLRite ingestion complete");
    println!(
        "chunks(total={}, processed={}, failed={}, resumed_from={})",
        report.total_chunks,
        report.processed_chunks,
        report.failed_chunks,
        report.resumed_from_chunk
    );
    println!(
        "provider={} model={}",
        report.provider, report.model_version
    );
    println!("source={}", report.source);
    Ok(())
}

#[derive(Debug)]
struct Args {
    db_path: PathBuf,
    job_id: String,
    doc_id: String,
    source_id: String,
    tenant: String,
    source_kind: String,
    file_path: Option<PathBuf>,
    url: Option<String>,
    direct_content: Option<String>,
    checkpoint_path: Option<PathBuf>,
    embedding_dim: usize,
    model_version: String,
    chunking_mode: String,
    max_chars: usize,
    overlap_chars: usize,
    batch_size: usize,
    continue_on_partial_failure: bool,
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut out = Args {
        db_path: PathBuf::from("sqlrite_demo.db"),
        job_id: "job-1".to_string(),
        doc_id: "doc-1".to_string(),
        source_id: "source-1".to_string(),
        tenant: "default".to_string(),
        source_kind: "unknown".to_string(),
        file_path: None,
        url: None,
        direct_content: None,
        checkpoint_path: None,
        embedding_dim: 256,
        model_version: "det-v1".to_string(),
        chunking_mode: "heading".to_string(),
        max_chars: 1200,
        overlap_chars: 120,
        batch_size: 64,
        continue_on_partial_failure: false,
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                out.db_path = PathBuf::from(parse_string(&args, i, "--db")?);
            }
            "--job-id" => {
                i += 1;
                out.job_id = parse_string(&args, i, "--job-id")?;
            }
            "--doc-id" => {
                i += 1;
                out.doc_id = parse_string(&args, i, "--doc-id")?;
            }
            "--source-id" => {
                i += 1;
                out.source_id = parse_string(&args, i, "--source-id")?;
            }
            "--tenant" => {
                i += 1;
                out.tenant = parse_string(&args, i, "--tenant")?;
            }
            "--file" => {
                i += 1;
                out.file_path = Some(PathBuf::from(parse_string(&args, i, "--file")?));
                out.source_kind = "file".to_string();
            }
            "--url" => {
                i += 1;
                out.url = Some(parse_string(&args, i, "--url")?);
                out.source_kind = "url".to_string();
            }
            "--content" => {
                i += 1;
                out.direct_content = Some(parse_string(&args, i, "--content")?);
                out.source_kind = "direct".to_string();
            }
            "--checkpoint" => {
                i += 1;
                out.checkpoint_path = Some(PathBuf::from(parse_string(&args, i, "--checkpoint")?));
            }
            "--embedding-dim" => {
                i += 1;
                out.embedding_dim = parse_usize(&args, i, "--embedding-dim")?;
            }
            "--model-version" => {
                i += 1;
                out.model_version = parse_string(&args, i, "--model-version")?;
            }
            "--chunking" => {
                i += 1;
                out.chunking_mode = parse_string(&args, i, "--chunking")?;
            }
            "--max-chars" => {
                i += 1;
                out.max_chars = parse_usize(&args, i, "--max-chars")?;
            }
            "--overlap-chars" => {
                i += 1;
                out.overlap_chars = parse_usize(&args, i, "--overlap-chars")?;
            }
            "--batch-size" => {
                i += 1;
                out.batch_size = parse_usize(&args, i, "--batch-size")?;
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
    "usage: cargo run --bin sqlrite-ingest -- [--db PATH] [--job-id ID] [--doc-id ID] [--source-id ID] [--tenant TENANT] (--file PATH|--url URL|--content TEXT) [--checkpoint PATH] [--embedding-dim N] [--model-version STR] [--chunking heading|fixed|semantic] [--max-chars N] [--overlap-chars N] [--batch-size N] [--continue-on-partial-failure]".to_string()
}
