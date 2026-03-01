use crate::{
    DurabilityProfile, Result, RuntimeConfig, SqlRite, SqlRiteToolAdapter, ToolResponse,
    VectorIndexMode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub db_path: PathBuf,
    pub runtime: RuntimeConfig,
    pub auth_token: Option<String>,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        let mut runtime = RuntimeConfig::default();
        runtime.durability_profile = DurabilityProfile::Balanced;
        runtime.vector_index_mode = VectorIndexMode::BruteForce;
        Self {
            db_path: PathBuf::from("sqlrite.db"),
            runtime,
            auth_token: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcErrorObject {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

pub fn mcp_tools_manifest_document(auth_required: bool) -> Value {
    let tools = SqlRiteToolAdapter::mcp_tools_manifest()
        .into_iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "name": "sqlrite-mcp",
        "version": env!("CARGO_PKG_VERSION"),
        "transport": {
            "type": "stdio",
            "command": "sqlrite",
            "args": ["mcp"]
        },
        "auth": {
            "type": "static_token",
            "required": auth_required,
            "argument": "auth_token"
        },
        "tools": tools,
    })
}

pub fn run_stdio_mcp_server(config: McpServerConfig) -> Result<()> {
    let db = SqlRite::open_with_config(&config.db_path, config.runtime)?;
    let adapter = SqlRiteToolAdapter::new(&db);

    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();

    loop {
        let Some(raw_message) = read_framed_message(&mut reader)? else {
            break;
        };

        let parsed = serde_json::from_str::<JsonRpcRequest>(&raw_message);
        let response = match parsed {
            Ok(request) => handle_request(&adapter, config.auth_token.as_deref(), request),
            Err(error) => Some(json_rpc_error(
                None,
                -32700,
                "parse error",
                Some(json!({"detail": error.to_string()})),
            )),
        };

        if let Some(payload) = response {
            let serialized = serde_json::to_string(&payload)?;
            write_framed_message(&mut writer, &serialized)?;
        }
    }

    Ok(())
}

fn handle_request(
    adapter: &SqlRiteToolAdapter<'_>,
    auth_token: Option<&str>,
    request: JsonRpcRequest,
) -> Option<Value> {
    let id = request.id.clone();

    match request.method.as_str() {
        "notifications/initialized" | "initialized" => None,
        "initialize" => Some(json_rpc_result(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": "sqlrite",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                }
            }),
        )),
        "ping" => Some(json_rpc_result(id, json!({}))),
        "tools/list" => {
            let tools = SqlRiteToolAdapter::mcp_tools_manifest()
                .into_iter()
                .map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema,
                    })
                })
                .collect::<Vec<_>>();

            Some(json_rpc_result(id, json!({"tools": tools})))
        }
        "tools/call" => {
            let Some(params) = request.params.as_object() else {
                return Some(json_rpc_error(id, -32602, "invalid params", None));
            };

            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(json_rpc_error(id, -32602, "missing params.name", None));
            };

            let mut arguments = params
                .get("arguments")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();

            if let Some(expected) = auth_token {
                let provided = arguments
                    .get("auth_token")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if provided != expected {
                    return Some(json_rpc_error(
                        id,
                        -32001,
                        "unauthorized tool call",
                        Some(json!({
                            "hint": "provide matching arguments.auth_token"
                        })),
                    ));
                }
                arguments.remove("auth_token");
            }

            let response = adapter.handle_named_call(name, Value::Object(arguments));
            Some(json_rpc_result(id, tool_response_to_mcp_result(response)))
        }
        _ => Some(json_rpc_error(id, -32601, "method not found", None)),
    }
}

fn tool_response_to_mcp_result(response: ToolResponse) -> Value {
    match response {
        ToolResponse::Ok { payload } => {
            let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
            json!({
                "content": [{"type": "text", "text": text}],
                "structuredContent": payload,
                "isError": false,
            })
        }
        ToolResponse::Error { message } => {
            json!({
                "content": [{"type": "text", "text": message}],
                "isError": true,
            })
        }
    }
}

fn json_rpc_result(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
}

fn json_rpc_error(id: Option<Value>, code: i64, message: &str, data: Option<Value>) -> Value {
    let error = JsonRpcErrorObject {
        code,
        message: message.to_string(),
        data,
    };

    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": error,
    })
}

fn read_framed_message<R: BufRead + Read>(reader: &mut R) -> Result<Option<String>> {
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            if content_length.is_none() {
                return Ok(None);
            }
            return Err(std::io::Error::other("unexpected EOF while reading MCP headers").into());
        }

        if line == "\r\n" || line == "\n" {
            break;
        }

        let header = line.trim_end_matches(['\r', '\n']);
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            let parsed = value
                .trim()
                .parse::<usize>()
                .map_err(|_| std::io::Error::other("invalid Content-Length header"))?;
            content_length = Some(parsed);
        }
    }

    let content_length =
        content_length.ok_or_else(|| std::io::Error::other("missing Content-Length header"))?;
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    let payload = String::from_utf8(body)
        .map_err(|error| std::io::Error::other(format!("invalid utf-8 payload: {error}")))?;
    Ok(Some(payload))
}

fn write_framed_message<W: Write>(writer: &mut W, payload: &str) -> Result<()> {
    write!(
        writer,
        "Content-Length: {}\r\n\r\n{}",
        payload.len(),
        payload
    )?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkInput, RuntimeConfig};

    fn encode_frame(payload: &str) -> String {
        format!("Content-Length: {}\r\n\r\n{}", payload.len(), payload)
    }

    #[test]
    fn reads_framed_payload() -> Result<()> {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let framed = encode_frame(body);
        let mut cursor = std::io::Cursor::new(framed.into_bytes());
        let parsed = read_framed_message(&mut cursor)?.expect("payload expected");
        assert_eq!(parsed, body);
        Ok(())
    }

    #[test]
    fn handles_initialize_and_tools_list() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);

        let init = JsonRpcRequest {
            id: Some(json!(1)),
            method: "initialize".to_string(),
            params: json!({}),
        };
        let init_response = handle_request(&adapter, None, init).expect("response expected");
        assert_eq!(
            init_response["result"]["protocolVersion"],
            json!("2024-11-05")
        );

        let list = JsonRpcRequest {
            id: Some(json!(2)),
            method: "tools/list".to_string(),
            params: json!({}),
        };
        let list_response = handle_request(&adapter, None, list).expect("response expected");
        let tools = list_response["result"]["tools"]
            .as_array()
            .expect("tools array expected");
        assert!(tools.iter().any(|tool| tool["name"] == json!("search")));

        Ok(())
    }

    #[test]
    fn handles_tool_call_and_auth_baseline() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        db.ingest_chunks(&[ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "local memory for agents".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "demo"}),
            source: None,
        }])?;

        let adapter = SqlRiteToolAdapter::new(&db);

        let unauthorized = JsonRpcRequest {
            id: Some(json!(3)),
            method: "tools/call".to_string(),
            params: json!({
                "name": "search",
                "arguments": {
                    "query_text": "memory",
                    "top_k": 1
                }
            }),
        };
        let unauthorized_response =
            handle_request(&adapter, Some("token-1"), unauthorized).expect("response expected");
        assert_eq!(unauthorized_response["error"]["code"], json!(-32001));

        let authorized = JsonRpcRequest {
            id: Some(json!(4)),
            method: "tools/call".to_string(),
            params: json!({
                "name": "search",
                "arguments": {
                    "query_text": "memory",
                    "top_k": 1,
                    "auth_token": "token-1"
                }
            }),
        };
        let authorized_response =
            handle_request(&adapter, Some("token-1"), authorized).expect("response expected");
        assert_eq!(authorized_response["result"]["isError"], json!(false));

        Ok(())
    }

    #[test]
    fn manifest_document_contains_tools_and_auth_contract() {
        let manifest = mcp_tools_manifest_document(true);
        assert_eq!(manifest["auth"]["required"], json!(true));
        let tools = manifest["tools"].as_array().expect("tools array expected");
        assert!(tools.iter().any(|tool| tool["name"] == json!("health")));
    }

    #[test]
    fn tool_response_mapping_sets_error_flag() {
        let ok = tool_response_to_mcp_result(ToolResponse::Ok {
            payload: json!({"ok": true}),
        });
        assert_eq!(ok["isError"], json!(false));

        let err = tool_response_to_mcp_result(ToolResponse::Error {
            message: "boom".to_string(),
        });
        assert_eq!(err["isError"], json!(true));
    }

    #[test]
    fn write_framed_message_includes_content_length() -> Result<()> {
        let mut output = Vec::<u8>::new();
        write_framed_message(&mut output, "{}")?;
        let rendered = String::from_utf8(output).expect("utf-8 output");
        assert!(rendered.starts_with("Content-Length: 2\r\n\r\n{}"));
        Ok(())
    }

    #[test]
    fn parses_invalid_message_as_error_response() {
        let response = json_rpc_error(None, -32700, "parse error", None);
        assert_eq!(response["error"]["code"], json!(-32700));
    }

    #[test]
    fn initialized_notification_returns_no_response() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let notification = JsonRpcRequest {
            id: None,
            method: "notifications/initialized".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, notification);
        assert!(response.is_none());
        Ok(())
    }

    #[test]
    fn unknown_method_returns_method_not_found() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let unknown = JsonRpcRequest {
            id: Some(json!(9)),
            method: "does/not/exist".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, unknown).expect("response expected");
        assert_eq!(response["error"]["code"], json!(-32601));
        Ok(())
    }

    #[test]
    fn tools_call_requires_name() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let missing_name = JsonRpcRequest {
            id: Some(json!(10)),
            method: "tools/call".to_string(),
            params: json!({"arguments": {}}),
        };
        let response = handle_request(&adapter, None, missing_name).expect("response expected");
        assert_eq!(response["error"]["code"], json!(-32602));
        Ok(())
    }

    #[test]
    fn removes_auth_token_before_tool_dispatch() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let health = JsonRpcRequest {
            id: Some(json!(11)),
            method: "tools/call".to_string(),
            params: json!({
                "name": "health",
                "arguments": {
                    "auth_token": "a"
                }
            }),
        };
        let response = handle_request(&adapter, Some("a"), health).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(false));
        Ok(())
    }

    #[test]
    fn read_frame_requires_content_length() {
        let mut cursor = std::io::Cursor::new(b"Header: 1\r\n\r\n{}".to_vec());
        let error = read_framed_message(&mut cursor).expect_err("should fail");
        assert!(error.to_string().contains("missing Content-Length"));
    }

    #[test]
    fn reads_multiple_frames_sequentially() -> Result<()> {
        let one = encode_frame(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#);
        let two = encode_frame(r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#);
        let mut cursor = std::io::Cursor::new(format!("{}{}", one, two).into_bytes());

        let first = read_framed_message(&mut cursor)?.expect("first frame");
        let second = read_framed_message(&mut cursor)?.expect("second frame");
        let third = read_framed_message(&mut cursor)?;

        assert!(first.contains("\"id\":1"));
        assert!(second.contains("\"id\":2"));
        assert!(third.is_none());
        Ok(())
    }

    #[test]
    fn tools_call_with_unknown_tool_marks_result_error() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(13)),
            method: "tools/call".to_string(),
            params: json!({
                "name": "unknown_tool",
                "arguments": {}
            }),
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(true));
        Ok(())
    }

    #[test]
    fn ping_returns_empty_result() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let ping = JsonRpcRequest {
            id: Some(json!(14)),
            method: "ping".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, ping).expect("response expected");
        assert_eq!(response["result"], json!({}));
        Ok(())
    }

    #[test]
    fn initialize_includes_server_info() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let init = JsonRpcRequest {
            id: Some(json!(15)),
            method: "initialize".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, init).expect("response expected");
        assert_eq!(response["result"]["serverInfo"]["name"], json!("sqlrite"));
        Ok(())
    }

    #[test]
    fn tools_list_response_has_input_schema() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let list = JsonRpcRequest {
            id: Some(json!(16)),
            method: "tools/list".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, list).expect("response expected");
        let tools = response["result"]["tools"].as_array().expect("tools array");
        assert!(
            tools
                .iter()
                .any(|tool| tool.get("inputSchema").is_some() && tool["name"] == json!("search"))
        );
        Ok(())
    }

    #[test]
    fn unauthorized_error_contains_hint() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let unauthorized = JsonRpcRequest {
            id: Some(json!(17)),
            method: "tools/call".to_string(),
            params: json!({"name": "health", "arguments": {}}),
        };
        let response = handle_request(&adapter, Some("secret"), unauthorized)
            .expect("unauthorized response expected");
        assert_eq!(response["error"]["code"], json!(-32001));
        assert!(
            response["error"]["data"]["hint"]
                .as_str()
                .expect("hint present")
                .contains("auth_token")
        );
        Ok(())
    }

    #[test]
    fn manifest_transport_points_to_unified_cli() {
        let manifest = mcp_tools_manifest_document(false);
        assert_eq!(manifest["transport"]["command"], json!("sqlrite"));
        assert_eq!(manifest["transport"]["args"], json!(["mcp"]));
    }

    #[test]
    fn tools_call_rejects_non_object_params() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(18)),
            method: "tools/call".to_string(),
            params: json!([]),
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["error"]["code"], json!(-32602));
        Ok(())
    }

    #[test]
    fn parse_error_response_contains_jsonrpc_envelope() {
        let response = json_rpc_error(None, -32700, "parse error", None);
        assert_eq!(response["jsonrpc"], json!("2.0"));
        assert_eq!(response["id"], Value::Null);
    }

    #[test]
    fn result_response_contains_jsonrpc_envelope() {
        let response = json_rpc_result(Some(json!(1)), json!({"ok": true}));
        assert_eq!(response["jsonrpc"], json!("2.0"));
        assert_eq!(response["id"], json!(1));
    }

    #[test]
    fn tool_call_search_returns_structured_content() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        db.ingest_chunks(&[ChunkInput::new(
            "cx",
            "dx",
            "mcp structured search",
            vec![1.0, 0.0],
        )])?;
        let adapter = SqlRiteToolAdapter::new(&db);

        let request = JsonRpcRequest {
            id: Some(json!(19)),
            method: "tools/call".to_string(),
            params: json!({
                "name": "search",
                "arguments": {
                    "query_text": "structured",
                    "top_k": 1
                }
            }),
        };

        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(false));
        assert!(response["result"].get("structuredContent").is_some());
        Ok(())
    }

    #[test]
    fn default_config_uses_sqlrite_db_path() {
        let cfg = McpServerConfig::default();
        assert_eq!(cfg.db_path, PathBuf::from("sqlrite.db"));
    }

    #[test]
    fn tools_call_without_arguments_defaults_to_empty_object() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(20)),
            method: "tools/call".to_string(),
            params: json!({"name": "health"}),
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(false));
        Ok(())
    }

    #[test]
    fn read_framed_message_returns_none_on_clean_eof() -> Result<()> {
        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
        let output = read_framed_message(&mut cursor)?;
        assert!(output.is_none());
        Ok(())
    }

    #[test]
    fn read_framed_message_rejects_invalid_content_length() {
        let payload = b"Content-Length: nope\r\n\r\n{}".to_vec();
        let mut cursor = std::io::Cursor::new(payload);
        let error = read_framed_message(&mut cursor).expect_err("should fail");
        assert!(error.to_string().contains("invalid Content-Length"));
    }

    #[test]
    fn read_framed_message_rejects_partial_payload() {
        let payload = b"Content-Length: 20\r\n\r\n{}".to_vec();
        let mut cursor = std::io::Cursor::new(payload);
        let error = read_framed_message(&mut cursor).expect_err("should fail");
        assert!(error.to_string().contains("failed to fill whole buffer"));
    }

    #[test]
    fn tools_call_health_returns_text_content() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(21)),
            method: "tools/call".to_string(),
            params: json!({"name": "health", "arguments": {}}),
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["result"]["content"][0]["type"], json!("text"));
        Ok(())
    }

    #[test]
    fn tools_list_contains_delete_by_metadata() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let list = JsonRpcRequest {
            id: Some(json!(22)),
            method: "tools/list".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, list).expect("response expected");
        let tools = response["result"]["tools"].as_array().expect("tools array");
        assert!(
            tools
                .iter()
                .any(|tool| tool["name"] == json!("delete_by_metadata"))
        );
        Ok(())
    }

    #[test]
    fn unauthorized_tool_call_uses_original_id() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(99)),
            method: "tools/call".to_string(),
            params: json!({"name": "health", "arguments": {}}),
        };
        let response = handle_request(&adapter, Some("token"), request).expect("response expected");
        assert_eq!(response["id"], json!(99));
        Ok(())
    }

    #[test]
    fn tools_call_unknown_method_not_found_code() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(123)),
            method: "custom/method".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["error"]["code"], json!(-32601));
        Ok(())
    }

    #[test]
    fn mcp_manifest_includes_version() {
        let manifest = mcp_tools_manifest_document(false);
        assert!(manifest["version"].as_str().is_some());
    }

    #[test]
    fn write_frame_can_roundtrip_read() -> Result<()> {
        let payload = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let mut out = Vec::<u8>::new();
        write_framed_message(&mut out, payload)?;
        let mut cursor = std::io::Cursor::new(out);
        let parsed = read_framed_message(&mut cursor)?.expect("frame expected");
        assert_eq!(parsed, payload);
        Ok(())
    }

    #[test]
    fn initialize_ignores_params_shape() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let init = JsonRpcRequest {
            id: Some(json!(333)),
            method: "initialize".to_string(),
            params: json!({"unexpected": true}),
        };
        let response = handle_request(&adapter, None, init).expect("response expected");
        assert_eq!(response["id"], json!(333));
        Ok(())
    }

    #[test]
    fn tools_call_with_non_object_arguments_uses_empty_object() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(444)),
            method: "tools/call".to_string(),
            params: json!({"name": "health", "arguments": []}),
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(false));
        Ok(())
    }

    #[test]
    fn read_headers_accepts_case_insensitive_content_length() -> Result<()> {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let framed = format!("content-length: {}\r\n\r\n{}", body.len(), body);
        let mut cursor = std::io::Cursor::new(framed.into_bytes());
        let parsed = read_framed_message(&mut cursor)?.expect("payload expected");
        assert_eq!(parsed, body);
        Ok(())
    }

    #[test]
    fn json_rpc_error_can_include_data() {
        let response = json_rpc_error(Some(json!(1)), -32000, "x", Some(json!({"a": 1})));
        assert_eq!(response["error"]["data"]["a"], json!(1));
    }

    #[test]
    fn json_rpc_result_defaults_id_to_null() {
        let response = json_rpc_result(None, json!({"ok": true}));
        assert_eq!(response["id"], Value::Null);
    }

    #[test]
    fn manifest_lists_search_tool_first_class_schema() {
        let manifest = mcp_tools_manifest_document(false);
        let tools = manifest["tools"].as_array().expect("tools array");
        let search = tools
            .iter()
            .find(|tool| tool["name"] == json!("search"))
            .expect("search tool present");
        assert!(search["inputSchema"].is_object());
    }

    #[test]
    fn tools_call_returns_error_flag_for_bad_ingest_payload() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(555)),
            method: "tools/call".to_string(),
            params: json!({
                "name": "ingest",
                "arguments": {"chunks": [{"invalid": true}]}
            }),
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(true));
        Ok(())
    }

    #[test]
    fn tools_call_with_auth_and_empty_args_for_health_is_allowed() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(777)),
            method: "tools/call".to_string(),
            params: json!({"name": "health", "arguments": {"auth_token": "t"}}),
        };
        let response = handle_request(&adapter, Some("t"), request).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(false));
        Ok(())
    }

    #[test]
    fn parse_error_code_constant_is_standard() {
        let response = json_rpc_error(None, -32700, "parse error", None);
        assert_eq!(response["error"]["code"], json!(-32700));
    }

    #[test]
    fn tools_call_missing_params_object_returns_invalid_params() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(888)),
            method: "tools/call".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["error"]["code"], json!(-32602));
        Ok(())
    }

    #[test]
    fn tools_call_health_without_auth_when_not_required_passes() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(999)),
            method: "tools/call".to_string(),
            params: json!({"name": "health", "arguments": {}}),
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(false));
        Ok(())
    }

    #[test]
    fn tools_call_maps_delete_by_metadata_request() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        db.ingest_chunks(&[ChunkInput {
            id: "dmeta-1".to_string(),
            doc_id: "doc-1".to_string(),
            content: "delete me".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant":"x"}),
            source: None,
        }])?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(1000)),
            method: "tools/call".to_string(),
            params: json!({
                "name": "delete_by_metadata",
                "arguments": {"key":"tenant","value":"x"}
            }),
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(false));
        Ok(())
    }

    #[test]
    fn tools_call_error_still_returns_result_envelope() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(1001)),
            method: "tools/call".to_string(),
            params: json!({"name": "unknown", "arguments": {}}),
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert!(response.get("result").is_some());
        assert!(response.get("error").is_none());
        Ok(())
    }

    #[test]
    fn tools_call_auth_token_is_not_forwarded_to_tool() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(1002)),
            method: "tools/call".to_string(),
            params: json!({"name": "health", "arguments": {"auth_token":"z"}}),
        };
        let response = handle_request(&adapter, Some("z"), request).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(false));
        Ok(())
    }

    #[test]
    fn malformed_utf8_payload_returns_error() {
        let bytes = b"Content-Length: 1\r\n\r\n\xFF".to_vec();
        let mut cursor = std::io::Cursor::new(bytes);
        let error = read_framed_message(&mut cursor).expect_err("should fail");
        assert!(error.to_string().contains("invalid utf-8 payload"));
    }

    #[test]
    fn json_rpc_error_without_data_omits_data_field() {
        let response = json_rpc_error(Some(json!(1)), -32000, "x", None);
        assert!(response["error"].get("data").is_none() || response["error"]["data"].is_null());
    }

    #[test]
    fn tools_call_search_without_query_is_error_result() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let request = JsonRpcRequest {
            id: Some(json!(1003)),
            method: "tools/call".to_string(),
            params: json!({"name":"search","arguments":{}}),
        };
        let response = handle_request(&adapter, None, request).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(true));
        Ok(())
    }

    #[test]
    fn write_frame_length_matches_payload_bytes() -> Result<()> {
        let payload = "{\"x\":1}";
        let mut out = Vec::<u8>::new();
        write_framed_message(&mut out, payload)?;
        let rendered = String::from_utf8(out).expect("utf-8");
        assert!(rendered.starts_with("Content-Length: 7"));
        Ok(())
    }

    #[test]
    fn parse_error_response_has_null_id() {
        let response = json_rpc_error(None, -32700, "parse error", None);
        assert_eq!(response["id"], Value::Null);
    }

    #[test]
    fn tools_list_returns_non_empty_set() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let req = JsonRpcRequest {
            id: Some(json!(1004)),
            method: "tools/list".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, req).expect("response expected");
        let tools = response["result"]["tools"].as_array().expect("tools array");
        assert!(!tools.is_empty());
        Ok(())
    }

    #[test]
    fn initialize_capabilities_include_tools() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let req = JsonRpcRequest {
            id: Some(json!(1005)),
            method: "initialize".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, req).expect("response expected");
        assert!(response["result"]["capabilities"].get("tools").is_some());
        Ok(())
    }

    #[test]
    fn tools_call_health_with_auth_token_mismatch_fails() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let req = JsonRpcRequest {
            id: Some(json!(1006)),
            method: "tools/call".to_string(),
            params: json!({"name":"health","arguments":{"auth_token":"bad"}}),
        };
        let response = handle_request(&adapter, Some("good"), req).expect("response expected");
        assert_eq!(response["error"]["code"], json!(-32001));
        Ok(())
    }

    #[test]
    fn tools_call_health_with_auth_token_match_passes() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let req = JsonRpcRequest {
            id: Some(json!(1007)),
            method: "tools/call".to_string(),
            params: json!({"name":"health","arguments":{"auth_token":"good"}}),
        };
        let response = handle_request(&adapter, Some("good"), req).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(false));
        Ok(())
    }

    #[test]
    fn tools_call_with_missing_id_still_returns_null_id() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let req = JsonRpcRequest {
            id: None,
            method: "tools/list".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, req).expect("response expected");
        assert_eq!(response["id"], Value::Null);
        Ok(())
    }

    #[test]
    fn tools_call_search_returns_text_blob() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        db.ingest_chunks(&[ChunkInput::new("c", "d", "mcp", vec![1.0, 0.0])])?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let req = JsonRpcRequest {
            id: Some(json!(1008)),
            method: "tools/call".to_string(),
            params: json!({"name":"search","arguments":{"query_text":"mcp","top_k":1}}),
        };
        let response = handle_request(&adapter, None, req).expect("response expected");
        assert_eq!(response["result"]["content"][0]["type"], json!("text"));
        Ok(())
    }

    #[test]
    fn manifest_auth_argument_name_is_stable() {
        let manifest = mcp_tools_manifest_document(true);
        assert_eq!(manifest["auth"]["argument"], json!("auth_token"));
    }

    #[test]
    fn notification_initialized_is_silent_even_with_id_none() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let req = JsonRpcRequest {
            id: None,
            method: "initialized".to_string(),
            params: Value::Null,
        };
        assert!(handle_request(&adapter, None, req).is_none());
        Ok(())
    }

    #[test]
    fn tools_call_unknown_returns_is_error_true() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let req = JsonRpcRequest {
            id: Some(json!(1009)),
            method: "tools/call".to_string(),
            params: json!({"name":"nope","arguments":{}}),
        };
        let response = handle_request(&adapter, None, req).expect("response expected");
        assert_eq!(response["result"]["isError"], json!(true));
        Ok(())
    }

    #[test]
    fn parse_error_builder_includes_message() {
        let response = json_rpc_error(None, -32700, "parse error", None);
        assert_eq!(response["error"]["message"], json!("parse error"));
    }

    #[test]
    fn method_not_found_code_is_standard() {
        let response = json_rpc_error(Some(json!(1)), -32601, "method not found", None);
        assert_eq!(response["error"]["code"], json!(-32601));
    }

    #[test]
    fn invalid_params_code_is_standard() {
        let response = json_rpc_error(Some(json!(1)), -32602, "invalid params", None);
        assert_eq!(response["error"]["code"], json!(-32602));
    }

    #[test]
    fn unauthorized_error_code_is_stable() {
        let response = json_rpc_error(Some(json!(1)), -32001, "unauthorized", None);
        assert_eq!(response["error"]["code"], json!(-32001));
    }

    #[test]
    fn tools_list_uses_input_schema_key() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let req = JsonRpcRequest {
            id: Some(json!(1010)),
            method: "tools/list".to_string(),
            params: Value::Null,
        };
        let response = handle_request(&adapter, None, req).expect("response expected");
        assert!(
            response["result"]["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .all(|tool| tool.get("inputSchema").is_some())
        );
        Ok(())
    }

    #[test]
    fn tools_call_health_is_structured() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let req = JsonRpcRequest {
            id: Some(json!(1011)),
            method: "tools/call".to_string(),
            params: json!({"name":"health","arguments":{}}),
        };
        let response = handle_request(&adapter, None, req).expect("response expected");
        assert!(response["result"].get("structuredContent").is_some());
        Ok(())
    }
}
