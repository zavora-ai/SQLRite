// Run: cargo run --example tool_adapter
// Demonstrates: named tool call + MCP-style tool manifest.

use serde_json::json;
use sqlrite::{ChunkInput, Result, RuntimeConfig, SqlRite, SqlRiteToolAdapter, ToolRequest};

fn main() -> Result<()> {
    let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
    let adapter = SqlRiteToolAdapter::new(&db);

    let _ = adapter.handle_request(ToolRequest::Ingest {
        chunks: vec![
            ChunkInput::new(
                "chunk-tool-1",
                "doc-tool-1",
                "Tool adapters expose SQLRite as callable functions.",
                vec![1.0, 0.0],
            )
            .with_metadata(json!({"tenant": "acme"})),
        ],
    })?;

    let response = adapter.handle_named_call(
        "search",
        json!({
            "query_text": "callable functions",
            "top_k": 3,
            "metadata_filters": {"tenant": "acme"}
        }),
    );

    println!("== tool_adapter results ==");
    println!(
        "named tool response: {}",
        serde_json::to_string_pretty(&response)?
    );
    println!(
        "tools exposed: {}",
        SqlRiteToolAdapter::mcp_tools_manifest().len()
    );
    Ok(())
}
