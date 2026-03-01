use crate::{
    DurabilityProfile, SearchRequest, SearchResult, SqlRite, execute_sql_statement_json,
    prepare_sql_connection,
};
use rusqlite::Connection;
use serde_json::Value;
use sqlrite_sdk_core::{QueryEnvelope, QueryRequest, SqlRequest};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SdkRuntimeError {
    #[error("validation error: {0}")]
    Validation(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl SdkRuntimeError {
    pub fn is_validation(&self) -> bool {
        matches!(self, Self::Validation(_))
    }
}

pub fn execute_query(
    db: &SqlRite,
    input: QueryRequest,
) -> Result<QueryEnvelope<SearchResult>, SdkRuntimeError> {
    input
        .validate()
        .map_err(|error| SdkRuntimeError::Validation(error.to_string()))?;

    let request = SearchRequest {
        query_text: input.normalized_query_text(),
        query_embedding: input.normalized_query_embedding(),
        top_k: input.top_k_or_default(),
        alpha: input.alpha_or_default(),
        candidate_limit: input.candidate_limit_or_default(),
        metadata_filters: input.normalized_metadata_filters(),
        doc_id: input.normalized_doc_id(),
        ..SearchRequest::default()
    };

    request
        .validate()
        .map_err(|error| SdkRuntimeError::Validation(error.to_string()))?;

    let rows = db
        .search(request)
        .map_err(|error| SdkRuntimeError::Internal(error.to_string()))?;

    Ok(QueryEnvelope::from_rows(rows))
}

pub fn execute_sql(
    db_path: &Path,
    profile: DurabilityProfile,
    input: SqlRequest,
) -> Result<Value, SdkRuntimeError> {
    input
        .validate()
        .map_err(|error| SdkRuntimeError::Validation(error.to_string()))?;

    let conn = Connection::open(db_path)
        .map_err(|error| SdkRuntimeError::Internal(format!("failed to open database: {error}")))?;

    prepare_sql_connection(&conn, profile).map_err(|error| {
        SdkRuntimeError::Internal(format!("failed to initialize sql runtime: {error}"))
    })?;

    execute_sql_statement_json(&conn, &input.statement)
        .map_err(|error| SdkRuntimeError::Validation(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkInput, RuntimeConfig};
    use serde_json::json;
    use tempfile::NamedTempFile;

    #[test]
    fn execute_query_returns_rows() {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default()).expect("open db");
        db.ingest_chunk(&ChunkInput {
            id: "sdk-1".to_string(),
            doc_id: "doc-1".to_string(),
            content: "agent runtime".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "demo"}),
            source: None,
        })
        .expect("seed chunk");

        let envelope = execute_query(
            &db,
            QueryRequest {
                query_text: Some("agent".to_string()),
                top_k: Some(1),
                ..QueryRequest::default()
            },
        )
        .expect("query");

        assert_eq!(envelope.kind, "query");
        assert_eq!(envelope.row_count, 1);
    }

    #[test]
    fn execute_sql_rejects_empty_statement() {
        let db_file = NamedTempFile::new().expect("temp file");
        let error = execute_sql(
            db_file.path(),
            DurabilityProfile::Balanced,
            SqlRequest {
                statement: " ".to_string(),
            },
        )
        .expect_err("expected validation error");

        assert!(error.is_validation());
    }
}
