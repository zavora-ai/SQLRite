use crate::{Result, RuntimeConfig, SqlRite, build_health_report};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8099".to_string(),
        }
    }
}

pub fn serve_health_endpoints(
    db_path: impl AsRef<Path>,
    runtime: RuntimeConfig,
    config: ServerConfig,
) -> Result<()> {
    let db = SqlRite::open_with_config(db_path, runtime)?;
    let listener = TcpListener::bind(&config.bind_addr)?;

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(_) => continue,
        };

        if let Err(error) = handle_connection(&db, &mut stream) {
            let _ = write_response(
                &mut stream,
                500,
                "text/plain; charset=utf-8",
                &format!("internal error: {error}"),
            );
        }
    }

    Ok(())
}

fn handle_connection(db: &SqlRite, stream: &mut TcpStream) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first_line = String::new();
    reader.read_line(&mut first_line)?;

    let path = parse_http_path(&first_line).unwrap_or("/");
    let (status, content_type, body) = build_response(db, path)?;
    write_response(stream, status, content_type, &body)?;
    Ok(())
}

fn parse_http_path(first_line: &str) -> Option<&str> {
    let mut parts = first_line.split_whitespace();
    let _method = parts.next()?;
    parts.next()
}

fn build_response(db: &SqlRite, path: &str) -> Result<(u16, &'static str, String)> {
    match path {
        "/healthz" => {
            let report = build_health_report(db)?;
            Ok((200, "application/json", serde_json::to_string(&report)?))
        }
        "/readyz" => {
            let report = build_health_report(db)?;
            let ready = report.integrity_check_ok;
            let status = if ready { 200 } else { 503 };
            let payload = serde_json::json!({
                "ready": ready,
                "schema_version": report.schema_version,
            });
            Ok((status, "application/json", payload.to_string()))
        }
        "/metrics" => {
            let report = build_health_report(db)?;
            let body = format!(
                "sqlrite_chunk_count {}\nsqlrite_schema_version {}\nsqlrite_index_entries {}\n",
                report.chunk_count, report.schema_version, report.vector_index_entries
            );
            Ok((200, "text/plain; version=0.0.4", body))
        }
        _ => Ok((404, "text/plain; charset=utf-8", "not found".to_string())),
    }
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let status_text = match status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    };

    let header = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkInput, RuntimeConfig};
    use serde_json::json;

    #[test]
    fn parses_http_path() {
        assert_eq!(
            parse_http_path("GET /healthz HTTP/1.1\r\n"),
            Some("/healthz")
        );
        assert_eq!(parse_http_path(""), None);
    }

    #[test]
    fn builds_health_response() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "health endpoint".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "acme"}),
            source: None,
        })?;

        let (status, content_type, body) = build_response(&db, "/healthz")?;
        assert_eq!(status, 200);
        assert_eq!(content_type, "application/json");
        assert!(body.contains("chunk_count"));
        Ok(())
    }
}
