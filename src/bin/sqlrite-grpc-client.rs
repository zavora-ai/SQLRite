use serde_json::json;
use sqlrite::grpc::proto::query_service_client::QueryServiceClient;
use sqlrite::grpc::proto::{HealthRequest, QueryRequest, SqlRequest};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let parsed = parse_args(&args).map_err(std::io::Error::other)?;

    let endpoint = format!("http://{}", parsed.addr);
    let mut client = QueryServiceClient::connect(endpoint).await?;

    match parsed.command {
        Command::Health => {
            let response = client
                .health(tonic::Request::new(HealthRequest {}))
                .await?
                .into_inner();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "status": response.status,
                    "version": response.version,
                }))?
            );
        }
        Command::Query {
            text,
            top_k,
            alpha,
            candidate_limit,
            doc_id,
        } => {
            let response = client
                .query(tonic::Request::new(QueryRequest {
                    query_text: text,
                    query_embedding: Vec::new(),
                    top_k,
                    alpha,
                    candidate_limit,
                    metadata_filters: Default::default(),
                    doc_id,
                }))
                .await?
                .into_inner();
            let payload: serde_json::Value = serde_json::from_str(&response.json_payload)?;
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        Command::Sql { statement } => {
            let response = client
                .sql(tonic::Request::new(SqlRequest { statement }))
                .await?
                .into_inner();
            let payload: serde_json::Value = serde_json::from_str(&response.json_payload)?;
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
    }

    Ok(())
}

#[derive(Debug)]
struct Args {
    addr: String,
    command: Command,
}

#[derive(Debug)]
enum Command {
    Health,
    Query {
        text: Option<String>,
        top_k: Option<u32>,
        alpha: Option<f32>,
        candidate_limit: Option<u32>,
        doc_id: Option<String>,
    },
    Sql {
        statement: String,
    },
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut addr = "127.0.0.1:50051".to_string();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--addr" {
            i += 1;
            addr = parse_string(args, i, "--addr")?;
            i += 1;
            continue;
        }
        break;
    }

    let command = args
        .get(i)
        .ok_or_else(|| usage().to_string())?
        .as_str()
        .to_string();
    i += 1;

    let command = match command.as_str() {
        "health" => Command::Health,
        "query" => {
            let mut text = None;
            let mut top_k = None;
            let mut alpha = None;
            let mut candidate_limit = None;
            let mut doc_id = None;

            while i < args.len() {
                match args[i].as_str() {
                    "--text" => {
                        i += 1;
                        text = Some(parse_string(args, i, "--text")?);
                    }
                    "--top-k" => {
                        i += 1;
                        top_k = Some(parse_u32(args, i, "--top-k")?);
                    }
                    "--alpha" => {
                        i += 1;
                        alpha = Some(parse_f32(args, i, "--alpha")?);
                    }
                    "--candidate-limit" => {
                        i += 1;
                        candidate_limit = Some(parse_u32(args, i, "--candidate-limit")?);
                    }
                    "--doc-id" => {
                        i += 1;
                        doc_id = Some(parse_string(args, i, "--doc-id")?);
                    }
                    "--help" | "-h" => return Err(usage().to_string()),
                    other => return Err(format!("unknown argument `{other}`\n{}", usage())),
                }
                i += 1;
            }

            Command::Query {
                text,
                top_k,
                alpha,
                candidate_limit,
                doc_id,
            }
        }
        "sql" => {
            let mut statement = None;
            while i < args.len() {
                match args[i].as_str() {
                    "--statement" => {
                        i += 1;
                        statement = Some(parse_string(args, i, "--statement")?);
                    }
                    "--help" | "-h" => return Err(usage().to_string()),
                    other => return Err(format!("unknown argument `{other}`\n{}", usage())),
                }
                i += 1;
            }
            Command::Sql {
                statement: statement.ok_or_else(|| "missing required --statement".to_string())?,
            }
        }
        "--help" | "-h" => return Err(usage().to_string()),
        other => return Err(format!("unknown command `{other}`\n{}", usage())),
    };

    Ok(Args { addr, command })
}

fn parse_string(args: &[String], i: usize, flag: &str) -> Result<String, String> {
    args.get(i)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_u32(args: &[String], i: usize, flag: &str) -> Result<u32, String> {
    let raw = parse_string(args, i, flag)?;
    raw.parse::<u32>()
        .map_err(|_| format!("invalid integer `{raw}` for {flag}"))
}

fn parse_f32(args: &[String], i: usize, flag: &str) -> Result<f32, String> {
    let raw = parse_string(args, i, flag)?;
    raw.parse::<f32>()
        .map_err(|_| format!("invalid float `{raw}` for {flag}"))
}

fn usage() -> &'static str {
    "usage: sqlrite-grpc-client [--addr HOST:PORT] <health|query|sql> [options]\n\ncommands:\n  health\n  query [--text QUERY] [--top-k N] [--alpha F] [--candidate-limit N] [--doc-id ID]\n  sql --statement SQL"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_health_command_defaults_addr() {
        let parsed = parse_args(&["health".to_string()]).expect("args");
        assert_eq!(parsed.addr, "127.0.0.1:50051");
        assert!(matches!(parsed.command, Command::Health));
    }

    #[test]
    fn parse_query_command_with_overrides() {
        let parsed = parse_args(&[
            "--addr".to_string(),
            "127.0.0.1:50071".to_string(),
            "query".to_string(),
            "--text".to_string(),
            "agent".to_string(),
            "--top-k".to_string(),
            "3".to_string(),
        ])
        .expect("args");

        assert_eq!(parsed.addr, "127.0.0.1:50071");
        match parsed.command {
            Command::Query { text, top_k, .. } => {
                assert_eq!(text.as_deref(), Some("agent"));
                assert_eq!(top_k, Some(3));
            }
            _ => panic!("expected query command"),
        }
    }
}
