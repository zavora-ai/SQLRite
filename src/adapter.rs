use crate::{ChunkInput, Result, SearchRequest, SqlRite, ops::build_health_report};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum ToolRequest {
    Search {
        query_text: Option<String>,
        query_embedding: Option<Vec<f32>>,
        top_k: Option<usize>,
        alpha: Option<f32>,
        candidate_limit: Option<usize>,
        metadata_filters: Option<HashMap<String, String>>,
        doc_id: Option<String>,
    },
    Ingest {
        chunks: Vec<ChunkInput>,
    },
    Health,
    DeleteByMetadata {
        key: String,
        value: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolResponse {
    Ok { payload: serde_json::Value },
    Error { message: String },
}

pub struct SqlRiteToolAdapter<'a> {
    db: &'a SqlRite,
}

impl<'a> SqlRiteToolAdapter<'a> {
    pub fn new(db: &'a SqlRite) -> Self {
        Self { db }
    }

    pub fn handle_request(&self, request: ToolRequest) -> Result<ToolResponse> {
        match request {
            ToolRequest::Search {
                query_text,
                query_embedding,
                top_k,
                alpha,
                candidate_limit,
                metadata_filters,
                doc_id,
            } => {
                let mut search = SearchRequest {
                    query_text,
                    query_embedding,
                    ..SearchRequest::default()
                };
                if let Some(top_k) = top_k {
                    search.top_k = top_k;
                }
                if let Some(alpha) = alpha {
                    search.alpha = alpha;
                }
                if let Some(candidate_limit) = candidate_limit {
                    search.candidate_limit = candidate_limit;
                }
                if let Some(metadata_filters) = metadata_filters {
                    search.metadata_filters = metadata_filters;
                }
                search.doc_id = doc_id;

                let results = self.db.search(search)?;
                Ok(ToolResponse::Ok {
                    payload: serde_json::to_value(results)?,
                })
            }
            ToolRequest::Ingest { chunks } => {
                self.db.ingest_chunks(&chunks)?;
                Ok(ToolResponse::Ok {
                    payload: serde_json::json!({
                        "ingested": chunks.len(),
                        "chunk_count": self.db.chunk_count()?,
                    }),
                })
            }
            ToolRequest::Health => {
                let report = build_health_report(self.db)?;
                Ok(ToolResponse::Ok {
                    payload: serde_json::to_value(report)?,
                })
            }
            ToolRequest::DeleteByMetadata { key, value } => {
                let deleted = self.db.delete_chunks_by_metadata(&key, &value)?;
                Ok(ToolResponse::Ok {
                    payload: serde_json::json!({"deleted": deleted}),
                })
            }
        }
    }

    pub fn handle_json(&self, payload: &str) -> String {
        let request = serde_json::from_str::<ToolRequest>(payload);
        let response = match request {
            Ok(request) => {
                self.handle_request(request)
                    .unwrap_or_else(|error| ToolResponse::Error {
                        message: error.to_string(),
                    })
            }
            Err(error) => ToolResponse::Error {
                message: format!("invalid request json: {error}"),
            },
        };

        serde_json::to_string_pretty(&response).unwrap_or_else(|error| {
            format!("{{\"status\":\"error\",\"message\":\"serialization failure: {error}\"}}")
        })
    }

    pub fn handle_named_call(&self, name: &str, arguments: serde_json::Value) -> ToolResponse {
        let request = match name {
            "search" => ToolRequest::Search {
                query_text: arguments
                    .get("query_text")
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned),
                query_embedding: arguments
                    .get("query_embedding")
                    .and_then(serde_json::Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(serde_json::Value::as_f64)
                            .map(|v| v as f32)
                            .collect::<Vec<_>>()
                    }),
                top_k: arguments
                    .get("top_k")
                    .and_then(serde_json::Value::as_u64)
                    .map(|v| v as usize),
                alpha: arguments
                    .get("alpha")
                    .and_then(serde_json::Value::as_f64)
                    .map(|v| v as f32),
                candidate_limit: arguments
                    .get("candidate_limit")
                    .and_then(serde_json::Value::as_u64)
                    .map(|v| v as usize),
                metadata_filters: arguments.get("metadata_filters").and_then(|value| {
                    serde_json::from_value::<HashMap<String, String>>(value.clone()).ok()
                }),
                doc_id: arguments
                    .get("doc_id")
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned),
            },
            "ingest" => match serde_json::from_value::<Vec<ChunkInput>>(
                arguments
                    .get("chunks")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Array(Vec::new())),
            ) {
                Ok(chunks) => ToolRequest::Ingest { chunks },
                Err(error) => {
                    return ToolResponse::Error {
                        message: format!("invalid ingest payload: {error}"),
                    };
                }
            },
            "health" => ToolRequest::Health,
            "delete_by_metadata" => {
                let key = arguments
                    .get("key")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let value = arguments
                    .get("value")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                ToolRequest::DeleteByMetadata { key, value }
            }
            _ => {
                return ToolResponse::Error {
                    message: format!("unknown tool `{name}`"),
                };
            }
        };

        self.handle_request(request)
            .unwrap_or_else(|error| ToolResponse::Error {
                message: error.to_string(),
            })
    }

    pub fn mcp_tools_manifest() -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "search".to_string(),
                description: "Hybrid vector+text retrieval over SQLRite chunks".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query_text": {"type": "string"},
                        "query_embedding": {"type": "array", "items": {"type": "number"}},
                        "top_k": {"type": "integer"},
                        "alpha": {"type": "number"},
                        "candidate_limit": {"type": "integer"},
                        "metadata_filters": {"type": "object", "additionalProperties": {"type": "string"}},
                        "doc_id": {"type": "string"}
                    }
                }),
            },
            ToolSpec {
                name: "ingest".to_string(),
                description: "Ingest chunks into SQLRite".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "required": ["chunks"],
                    "properties": {
                        "chunks": {"type": "array"}
                    }
                }),
            },
            ToolSpec {
                name: "health".to_string(),
                description: "Get integrity and index health report".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolSpec {
                name: "delete_by_metadata".to_string(),
                description: "Delete chunks by exact metadata key/value".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "required": ["key", "value"],
                    "properties": {
                        "key": {"type": "string"},
                        "value": {"type": "string"}
                    }
                }),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RuntimeConfig, SqlRite};
    use serde_json::json;

    #[test]
    fn adapter_can_ingest_search_and_report_health() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);

        let ingest = ToolRequest::Ingest {
            chunks: vec![ChunkInput {
                id: "c1".to_string(),
                doc_id: "d1".to_string(),
                content: "adapter search data".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({"tenant": "acme"}),
                source: None,
            }],
        };
        let ingest_response = adapter.handle_request(ingest)?;
        assert!(matches!(ingest_response, ToolResponse::Ok { .. }));

        let search_response = adapter.handle_request(ToolRequest::Search {
            query_text: Some("adapter".to_string()),
            query_embedding: None,
            top_k: Some(1),
            alpha: None,
            candidate_limit: None,
            metadata_filters: None,
            doc_id: None,
        })?;
        match search_response {
            ToolResponse::Ok { payload } => {
                let rows = payload.as_array().expect("search rows array");
                assert_eq!(rows.len(), 1);
            }
            ToolResponse::Error { message } => panic!("unexpected error: {message}"),
        }

        let health_response = adapter.handle_request(ToolRequest::Health)?;
        assert!(matches!(health_response, ToolResponse::Ok { .. }));
        Ok(())
    }

    #[test]
    fn adapter_supports_named_call_and_manifest() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let adapter = SqlRiteToolAdapter::new(&db);
        let manifest = SqlRiteToolAdapter::mcp_tools_manifest();
        assert!(manifest.iter().any(|tool| tool.name == "search"));

        let response = adapter.handle_named_call(
            "ingest",
            serde_json::json!({
                "chunks": [{
                    "id": "c2",
                    "doc_id": "d2",
                    "content": "named call data",
                    "embedding": [1.0, 0.0],
                    "metadata": {"tenant": "acme"},
                    "source": null
                }]
            }),
        );
        assert!(matches!(response, ToolResponse::Ok { .. }));
        Ok(())
    }
}
