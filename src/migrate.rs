use crate::{
    ChunkInput, DurabilityProfile, Result, RuntimeConfig, SqlRite, SqlRiteError, VectorIndexMode,
    execute_sql_statement_json, prepare_sql_connection,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const DOC_UPSERT_SQL: &str = "
    INSERT INTO documents (id, source, metadata) VALUES (?1, ?2, ?3)
    ON CONFLICT(id) DO UPDATE SET
        source = COALESCE(excluded.source, documents.source),
        metadata = excluded.metadata
";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationEmbeddingFormat {
    BlobF32Le,
    JsonArray,
    Csv,
}

#[derive(Debug, Clone)]
pub struct SqliteMigrationConfig {
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub runtime: RuntimeConfig,
    pub doc_table: Option<String>,
    pub doc_id_col: String,
    pub doc_source_col: Option<String>,
    pub doc_metadata_col: Option<String>,
    pub chunk_table: String,
    pub chunk_id_col: String,
    pub chunk_doc_id_col: String,
    pub chunk_content_col: String,
    pub chunk_metadata_col: Option<String>,
    pub chunk_embedding_col: String,
    pub chunk_embedding_dim_col: Option<String>,
    pub chunk_source_col: Option<String>,
    pub embedding_format: MigrationEmbeddingFormat,
    pub batch_size: usize,
    pub create_indexes: bool,
}

#[derive(Debug, Clone)]
pub struct PgvectorJsonlMigrationConfig {
    pub input_path: PathBuf,
    pub target_path: PathBuf,
    pub runtime: RuntimeConfig,
    pub batch_size: usize,
    pub create_indexes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiFirstSourceKind {
    Qdrant,
    Weaviate,
    Milvus,
}

#[derive(Debug, Clone)]
pub struct ApiJsonlMigrationConfig {
    pub source_kind: ApiFirstSourceKind,
    pub input_path: PathBuf,
    pub target_path: PathBuf,
    pub runtime: RuntimeConfig,
    pub batch_size: usize,
    pub create_indexes: bool,
    pub id_field: String,
    pub doc_id_field: String,
    pub content_field: String,
    pub embedding_field: String,
    pub metadata_field: Option<String>,
    pub source_field: Option<String>,
    pub doc_metadata_field: Option<String>,
    pub doc_source_field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    pub kind: String,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub documents_upserted: usize,
    pub chunks_migrated: usize,
    pub batch_size: usize,
    pub embedding_format: String,
    pub create_indexes: bool,
    pub vector_index_mode: String,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PgvectorJsonlRecord {
    id: String,
    doc_id: String,
    content: String,
    #[serde(default)]
    metadata: Value,
    embedding: Value,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    doc_metadata: Option<Value>,
    #[serde(default)]
    doc_source: Option<String>,
}

#[derive(Debug, Clone)]
struct SourceDocument {
    id: String,
    source: Option<String>,
    metadata: Value,
}

pub fn migrate_sqlite(config: &SqliteMigrationConfig) -> Result<MigrationReport> {
    let started = Instant::now();
    let source = Connection::open(&config.source_path)?;
    let target = SqlRite::open_with_config(&config.target_path, config.runtime.clone())?;

    let documents = load_sqlite_documents(&source, config)?;
    upsert_documents(&config.target_path, &documents)?;

    let chunks = load_sqlite_chunks(&source, config)?;
    ingest_chunks_in_batches(&target, &chunks, config.batch_size)?;
    create_indexes_if_requested(
        &config.target_path,
        config.runtime.vector_index_mode,
        config.create_indexes,
    )?;

    Ok(MigrationReport {
        kind: "sqlite".to_string(),
        source_path: config.source_path.clone(),
        target_path: config.target_path.clone(),
        documents_upserted: documents.len(),
        chunks_migrated: chunks.len(),
        batch_size: config.batch_size,
        embedding_format: embedding_format_name(config.embedding_format).to_string(),
        create_indexes: config.create_indexes,
        vector_index_mode: vector_index_mode_name(config.runtime.vector_index_mode).to_string(),
        duration_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

pub fn migrate_pgvector_jsonl(config: &PgvectorJsonlMigrationConfig) -> Result<MigrationReport> {
    let started = Instant::now();
    let db = SqlRite::open_with_config(&config.target_path, config.runtime.clone())?;
    let payload = fs::read_to_string(&config.input_path)?;

    let mut chunks = Vec::new();
    let mut documents = HashMap::<String, SourceDocument>::new();
    for (line_no, line) in payload.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record = serde_json::from_str::<PgvectorJsonlRecord>(trimmed).map_err(|error| {
            SqlRiteError::UnsupportedOperation(format!(
                "failed to parse jsonl line {}: {}",
                line_no + 1,
                error
            ))
        })?;
        let embedding =
            parse_embedding_value(&record.embedding, MigrationEmbeddingFormat::JsonArray, None)?;
        documents
            .entry(record.doc_id.clone())
            .or_insert_with(|| SourceDocument {
                id: record.doc_id.clone(),
                source: record.doc_source.clone().or(record.source.clone()),
                metadata: record.doc_metadata.clone().unwrap_or_else(|| json!({})),
            });
        chunks.push(ChunkInput {
            id: record.id,
            doc_id: record.doc_id,
            content: record.content,
            metadata: normalize_json_value(Some(record.metadata))?,
            embedding,
            source: record.source,
        });
    }

    let docs = documents.into_values().collect::<Vec<_>>();
    upsert_documents(&config.target_path, &docs)?;
    ingest_chunks_in_batches(&db, &chunks, config.batch_size)?;
    create_indexes_if_requested(
        &config.target_path,
        config.runtime.vector_index_mode,
        config.create_indexes,
    )?;

    Ok(MigrationReport {
        kind: "pgvector_jsonl".to_string(),
        source_path: config.input_path.clone(),
        target_path: config.target_path.clone(),
        documents_upserted: docs.len(),
        chunks_migrated: chunks.len(),
        batch_size: config.batch_size,
        embedding_format: "json_array".to_string(),
        create_indexes: config.create_indexes,
        vector_index_mode: vector_index_mode_name(config.runtime.vector_index_mode).to_string(),
        duration_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

pub fn migrate_api_jsonl(config: &ApiJsonlMigrationConfig) -> Result<MigrationReport> {
    let started = Instant::now();
    let db = SqlRite::open_with_config(&config.target_path, config.runtime.clone())?;
    let payload = fs::read_to_string(&config.input_path)?;

    let mut chunks = Vec::new();
    let mut documents = HashMap::<String, SourceDocument>::new();
    for (line_no, line) in payload.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record = serde_json::from_str::<Value>(trimmed).map_err(|error| {
            SqlRiteError::UnsupportedOperation(format!(
                "failed to parse jsonl line {}: {}",
                line_no + 1,
                error
            ))
        })?;
        let id = extract_required_string_field(&record, &config.id_field)?;
        let doc_id = extract_required_string_field(&record, &config.doc_id_field)?;
        let content = extract_required_string_field(&record, &config.content_field)?;
        let embedding_value = extract_required_field(&record, &config.embedding_field)?;
        let source = config
            .source_field
            .as_deref()
            .map(|path| extract_optional_string_field(&record, path))
            .transpose()?
            .flatten();
        let doc_source = config
            .doc_source_field
            .as_deref()
            .map(|path| extract_optional_string_field(&record, path))
            .transpose()?
            .flatten();
        let metadata = config
            .metadata_field
            .as_deref()
            .map(|path| extract_optional_json_field(&record, path))
            .transpose()?
            .flatten();
        let doc_metadata = config
            .doc_metadata_field
            .as_deref()
            .map(|path| extract_optional_json_field(&record, path))
            .transpose()?
            .flatten();
        let embedding =
            parse_embedding_value(embedding_value, MigrationEmbeddingFormat::JsonArray, None)?;

        documents
            .entry(doc_id.clone())
            .or_insert_with(|| SourceDocument {
                id: doc_id.clone(),
                source: doc_source.clone().or_else(|| source.clone()),
                metadata: doc_metadata
                    .clone()
                    .or_else(|| metadata.clone())
                    .unwrap_or_else(|| json!({})),
            });
        chunks.push(ChunkInput {
            id,
            doc_id,
            content,
            metadata: normalize_json_value(metadata)?,
            embedding,
            source,
        });
    }

    let docs = documents.into_values().collect::<Vec<_>>();
    upsert_documents(&config.target_path, &docs)?;
    ingest_chunks_in_batches(&db, &chunks, config.batch_size)?;
    create_indexes_if_requested(
        &config.target_path,
        config.runtime.vector_index_mode,
        config.create_indexes,
    )?;

    Ok(MigrationReport {
        kind: api_source_kind_name(config.source_kind).to_string(),
        source_path: config.input_path.clone(),
        target_path: config.target_path.clone(),
        documents_upserted: docs.len(),
        chunks_migrated: chunks.len(),
        batch_size: config.batch_size,
        embedding_format: "json_array".to_string(),
        create_indexes: config.create_indexes,
        vector_index_mode: vector_index_mode_name(config.runtime.vector_index_mode).to_string(),
        duration_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

fn load_sqlite_documents(
    source: &Connection,
    config: &SqliteMigrationConfig,
) -> Result<Vec<SourceDocument>> {
    let Some(table) = config.doc_table.as_deref() else {
        return Ok(Vec::new());
    };
    let table = sanitize_identifier(table)?;
    let id_col = sanitize_identifier(&config.doc_id_col)?;
    let source_col = config
        .doc_source_col
        .as_deref()
        .map(sanitize_identifier)
        .transpose()?;
    let metadata_col = config
        .doc_metadata_col
        .as_deref()
        .map(sanitize_identifier)
        .transpose()?;

    let sql = format!(
        "SELECT {id_col}, {source_expr}, {metadata_expr} FROM {table}",
        source_expr = source_col
            .as_deref()
            .map_or("NULL".to_string(), str::to_string),
        metadata_expr = metadata_col
            .as_deref()
            .map_or("NULL".to_string(), str::to_string),
    );
    let mut stmt = source.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;

    let mut documents = Vec::new();
    for row in rows {
        let (id, source, metadata_raw) = row?;
        documents.push(SourceDocument {
            id,
            source,
            metadata: normalize_json_text(metadata_raw)?,
        });
    }
    Ok(documents)
}

fn load_sqlite_chunks(
    source: &Connection,
    config: &SqliteMigrationConfig,
) -> Result<Vec<ChunkInput>> {
    let table = sanitize_identifier(&config.chunk_table)?;
    let id_col = sanitize_identifier(&config.chunk_id_col)?;
    let doc_id_col = sanitize_identifier(&config.chunk_doc_id_col)?;
    let content_col = sanitize_identifier(&config.chunk_content_col)?;
    let metadata_col = config
        .chunk_metadata_col
        .as_deref()
        .map(sanitize_identifier)
        .transpose()?;
    let embedding_col = sanitize_identifier(&config.chunk_embedding_col)?;
    let embedding_dim_col = config
        .chunk_embedding_dim_col
        .as_deref()
        .map(sanitize_identifier)
        .transpose()?;
    let source_col = config
        .chunk_source_col
        .as_deref()
        .map(sanitize_identifier)
        .transpose()?;

    let sql = format!(
        "SELECT {id_col}, {doc_id_col}, {content_col}, {metadata_expr}, {embedding_col}, {embedding_dim_expr}, {source_expr} FROM {table}",
        metadata_expr = metadata_col
            .as_deref()
            .map_or("NULL".to_string(), str::to_string),
        embedding_dim_expr = embedding_dim_col
            .as_deref()
            .map_or("NULL".to_string(), str::to_string),
        source_expr = source_col
            .as_deref()
            .map_or("NULL".to_string(), str::to_string),
    );
    let mut stmt = source.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let embedding_value = row.get_ref(4)?;
        let embedding_payload = match embedding_value {
            rusqlite::types::ValueRef::Blob(bytes) => EmbeddingPayload::Blob(bytes.to_vec()),
            rusqlite::types::ValueRef::Text(bytes) => {
                EmbeddingPayload::Text(String::from_utf8_lossy(bytes).to_string())
            }
            rusqlite::types::ValueRef::Null => EmbeddingPayload::Null,
            _ => {
                return Err(rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Blob,
                    Box::new(std::io::Error::other("unsupported embedding column type")),
                ));
            }
        };
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            embedding_payload,
            row.get::<_, Option<usize>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;

    let mut chunks = Vec::new();
    for row in rows {
        let (id, doc_id, content, metadata_raw, embedding_payload, embedding_dim, source) = row?;
        let embedding = match embedding_payload {
            EmbeddingPayload::Blob(bytes) => parse_blob_embedding(&bytes, embedding_dim)?,
            EmbeddingPayload::Text(text) => parse_embedding_text(&text, config.embedding_format)?,
            EmbeddingPayload::Null => {
                return Err(SqlRiteError::UnsupportedOperation(
                    "embedding column cannot be null".to_string(),
                ));
            }
        };
        chunks.push(ChunkInput {
            id,
            doc_id,
            content,
            metadata: normalize_json_text(metadata_raw)?,
            embedding,
            source,
        });
    }
    Ok(chunks)
}

fn upsert_documents(target_path: &Path, documents: &[SourceDocument]) -> Result<()> {
    if documents.is_empty() {
        return Ok(());
    }
    let mut conn = Connection::open(target_path)?;
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(DOC_UPSERT_SQL)?;
        for doc in documents {
            stmt.execute(params![
                doc.id,
                doc.source.as_deref(),
                serde_json::to_string(&doc.metadata)?,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn ingest_chunks_in_batches(db: &SqlRite, chunks: &[ChunkInput], batch_size: usize) -> Result<()> {
    let batch_size = batch_size.max(1);
    for batch in chunks.chunks(batch_size) {
        db.ingest_chunks(batch)?;
    }
    Ok(())
}

fn create_indexes_if_requested(
    target_path: &Path,
    mode: VectorIndexMode,
    create_indexes: bool,
) -> Result<()> {
    if !create_indexes {
        return Ok(());
    }
    let conn = Connection::open(target_path)?;
    prepare_sql_connection(&conn, DurabilityProfile::Balanced).map_err(SqlRiteError::from)?;
    if mode != VectorIndexMode::Disabled {
        let _ = execute_sql_statement_json(
            &conn,
            "CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw ON chunks(embedding) USING HNSW;",
        );
    }
    let _ = execute_sql_statement_json(
        &conn,
        "CREATE TEXT INDEX IF NOT EXISTS idx_chunks_content_fts ON chunks(content) USING FTS5;",
    );
    Ok(())
}

fn embedding_format_name(value: MigrationEmbeddingFormat) -> &'static str {
    match value {
        MigrationEmbeddingFormat::BlobF32Le => "blob_f32le",
        MigrationEmbeddingFormat::JsonArray => "json_array",
        MigrationEmbeddingFormat::Csv => "csv",
    }
}

fn vector_index_mode_name(value: VectorIndexMode) -> &'static str {
    match value {
        VectorIndexMode::BruteForce => "brute_force",
        VectorIndexMode::LshAnn => "lsh_ann",
        VectorIndexMode::HnswBaseline => "hnsw_baseline",
        VectorIndexMode::Disabled => "disabled",
    }
}

fn api_source_kind_name(value: ApiFirstSourceKind) -> &'static str {
    match value {
        ApiFirstSourceKind::Qdrant => "qdrant_jsonl",
        ApiFirstSourceKind::Weaviate => "weaviate_jsonl",
        ApiFirstSourceKind::Milvus => "milvus_jsonl",
    }
}

fn sanitize_identifier(value: &str) -> Result<String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(SqlRiteError::UnsupportedOperation(format!(
            "invalid identifier `{value}`"
        )));
    }
    Ok(value.to_string())
}

fn normalize_json_text(raw: Option<String>) -> Result<Value> {
    match raw
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None => Ok(json!({})),
        Some(value) => serde_json::from_str(value)
            .map_err(|error| SqlRiteError::UnsupportedOperation(error.to_string())),
    }
}

fn normalize_json_value(value: Option<Value>) -> Result<Value> {
    match value {
        None | Some(Value::Null) => Ok(json!({})),
        Some(other) => Ok(other),
    }
}

fn extract_required_field<'a>(value: &'a Value, path: &str) -> Result<&'a Value> {
    extract_json_path(value, path).ok_or_else(|| {
        SqlRiteError::UnsupportedOperation(format!("missing required field `{path}`"))
    })
}

fn extract_required_string_field(value: &Value, path: &str) -> Result<String> {
    let field = extract_required_field(value, path)?;
    match field {
        Value::String(text) => Ok(text.clone()),
        Value::Number(number) => Ok(number.to_string()),
        other => Err(SqlRiteError::UnsupportedOperation(format!(
            "field `{path}` must be string-compatible, found {other}"
        ))),
    }
}

fn extract_optional_string_field(value: &Value, path: &str) -> Result<Option<String>> {
    let Some(field) = extract_json_path(value, path) else {
        return Ok(None);
    };
    match field {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text.clone())),
        Value::Number(number) => Ok(Some(number.to_string())),
        other => Err(SqlRiteError::UnsupportedOperation(format!(
            "field `{path}` must be string-compatible, found {other}"
        ))),
    }
}

fn extract_optional_json_field(value: &Value, path: &str) -> Result<Option<Value>> {
    let Some(field) = extract_json_path(value, path) else {
        return Ok(None);
    };
    match field {
        Value::Null => Ok(None),
        other => Ok(Some(other.clone())),
    }
}

fn extract_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            return None;
        }
        current = match current {
            Value::Object(map) => map.get(segment)?,
            _ => return None,
        };
    }
    Some(current)
}

fn parse_embedding_value(
    value: &Value,
    format: MigrationEmbeddingFormat,
    embedding_dim: Option<usize>,
) -> Result<Vec<f32>> {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| match value {
                Value::Number(number) => {
                    number.as_f64().map(|value| value as f32).ok_or_else(|| {
                        SqlRiteError::UnsupportedOperation(
                            "embedding number out of range".to_string(),
                        )
                    })
                }
                _ => Err(SqlRiteError::UnsupportedOperation(
                    "embedding array must contain only numbers".to_string(),
                )),
            })
            .collect(),
        Value::Object(map) => {
            if let Some((_, inner)) = map
                .iter()
                .find(|(_, value)| matches!(value, Value::Array(_)))
            {
                parse_embedding_value(inner, format, embedding_dim)
            } else {
                Err(SqlRiteError::UnsupportedOperation(
                    "embedding object must contain at least one array value".to_string(),
                ))
            }
        }
        Value::String(text) => parse_embedding_text(text, format),
        Value::Null => Err(SqlRiteError::UnsupportedOperation(
            "embedding cannot be null".to_string(),
        )),
        _ => Err(SqlRiteError::UnsupportedOperation(
            "unsupported embedding value".to_string(),
        )),
    }
    .and_then(|embedding| {
        if let Some(expected_dim) = embedding_dim
            && embedding.len() != expected_dim
        {
            return Err(SqlRiteError::InvalidEmbeddingBytes {
                expected_bytes: expected_dim * 4,
                found_bytes: embedding.len() * 4,
            });
        }
        Ok(embedding)
    })
}

fn parse_embedding_text(text: &str, format: MigrationEmbeddingFormat) -> Result<Vec<f32>> {
    match format {
        MigrationEmbeddingFormat::Csv => text
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value.parse::<f32>().map_err(|_| {
                    SqlRiteError::UnsupportedOperation(format!(
                        "invalid csv embedding value `{value}`"
                    ))
                })
            })
            .collect(),
        MigrationEmbeddingFormat::JsonArray => {
            let value = serde_json::from_str::<Value>(text)
                .map_err(|error| SqlRiteError::UnsupportedOperation(error.to_string()))?;
            parse_embedding_value(&value, MigrationEmbeddingFormat::JsonArray, None)
        }
        MigrationEmbeddingFormat::BlobF32Le => Err(SqlRiteError::UnsupportedOperation(
            "blob_f32le embedding format requires BLOB source column".to_string(),
        )),
    }
}

fn parse_blob_embedding(bytes: &[u8], embedding_dim: Option<usize>) -> Result<Vec<f32>> {
    let Some(expected_dim) = embedding_dim else {
        return Err(SqlRiteError::UnsupportedOperation(
            "blob_f32le embedding format requires embedding_dim column".to_string(),
        ));
    };
    let expected_bytes = expected_dim * 4;
    if bytes.len() != expected_bytes {
        return Err(SqlRiteError::InvalidEmbeddingBytes {
            expected_bytes,
            found_bytes: bytes.len(),
        });
    }
    let mut embedding = Vec::with_capacity(expected_dim);
    for chunk in bytes.chunks_exact(4) {
        embedding.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(embedding)
}

enum EmbeddingPayload {
    Blob(Vec<u8>),
    Text(String),
    Null,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::{NamedTempFile, tempdir};

    fn legacy_blob(values: &[f32]) -> Vec<u8> {
        let mut out = Vec::new();
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }

    #[test]
    fn migrate_sqlite_imports_documents_and_chunks() -> Result<()> {
        let source = NamedTempFile::new().map_err(std::io::Error::other)?;
        let target = NamedTempFile::new().map_err(std::io::Error::other)?;
        let conn = Connection::open(source.path())?;
        conn.execute_batch(
            "
            CREATE TABLE legacy_documents (
                doc_id TEXT PRIMARY KEY,
                source_path TEXT,
                metadata_json TEXT
            );
            CREATE TABLE legacy_chunks (
                chunk_id TEXT PRIMARY KEY,
                doc_id TEXT NOT NULL,
                chunk_text TEXT NOT NULL,
                metadata_json TEXT,
                embedding_blob BLOB NOT NULL,
                embedding_dim INTEGER NOT NULL,
                source_path TEXT
            );
            ",
        )?;
        conn.execute(
            "INSERT INTO legacy_documents (doc_id, source_path, metadata_json) VALUES (?1, ?2, ?3)",
            params!["doc-1", "legacy/doc-1.md", "{\"tenant\":\"acme\"}"],
        )?;
        conn.execute(
            "INSERT INTO legacy_chunks (chunk_id, doc_id, chunk_text, metadata_json, embedding_blob, embedding_dim, source_path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                "chunk-1",
                "doc-1",
                "migrated sqlite content",
                "{\"topic\":\"retrieval\"}",
                legacy_blob(&[0.9_f32, 0.1_f32]),
                2_i64,
                "legacy/doc-1.md"
            ],
        )?;

        let report = migrate_sqlite(&SqliteMigrationConfig {
            source_path: source.path().to_path_buf(),
            target_path: target.path().to_path_buf(),
            runtime: RuntimeConfig::default(),
            doc_table: Some("legacy_documents".to_string()),
            doc_id_col: "doc_id".to_string(),
            doc_source_col: Some("source_path".to_string()),
            doc_metadata_col: Some("metadata_json".to_string()),
            chunk_table: "legacy_chunks".to_string(),
            chunk_id_col: "chunk_id".to_string(),
            chunk_doc_id_col: "doc_id".to_string(),
            chunk_content_col: "chunk_text".to_string(),
            chunk_metadata_col: Some("metadata_json".to_string()),
            chunk_embedding_col: "embedding_blob".to_string(),
            chunk_embedding_dim_col: Some("embedding_dim".to_string()),
            chunk_source_col: Some("source_path".to_string()),
            embedding_format: MigrationEmbeddingFormat::BlobF32Le,
            batch_size: 64,
            create_indexes: false,
        })?;

        assert_eq!(report.documents_upserted, 1);
        assert_eq!(report.chunks_migrated, 1);

        let db = SqlRite::open_with_config(target.path(), RuntimeConfig::default())?;
        assert_eq!(db.chunk_count()?, 1);
        let results = db.search(crate::SearchRequest::text("migrated", 1))?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, "chunk-1");
        Ok(())
    }

    #[test]
    fn migrate_pgvector_jsonl_imports_chunks() -> Result<()> {
        let tmp = tempdir()?;
        let input = tmp.path().join("pgvector.jsonl");
        let target = tmp.path().join("sqlrite.db");
        fs::write(
            &input,
            concat!(
                "{\"id\":\"p1\",\"doc_id\":\"doc-1\",\"content\":\"pgvector migrated\",\"metadata\":{\"tenant\":\"acme\"},\"embedding\":[0.8,0.2],\"source\":\"pg/doc-1.md\"}\n",
                "{\"id\":\"p2\",\"doc_id\":\"doc-2\",\"content\":\"second row\",\"metadata\":{\"tenant\":\"acme\"},\"embedding\":[0.7,0.3],\"source\":\"pg/doc-2.md\"}\n"
            ),
        )?;

        let report = migrate_pgvector_jsonl(&PgvectorJsonlMigrationConfig {
            input_path: input.clone(),
            target_path: target.clone(),
            runtime: RuntimeConfig::default(),
            batch_size: 32,
            create_indexes: false,
        })?;

        assert_eq!(report.chunks_migrated, 2);
        let db = SqlRite::open_with_config(target, RuntimeConfig::default())?;
        let results = db.search(crate::SearchRequest::text("pgvector", 1))?;
        assert_eq!(results[0].chunk_id, "p1");
        Ok(())
    }

    #[test]
    fn migrate_api_jsonl_imports_qdrant_shaped_rows() -> Result<()> {
        let tmp = tempdir()?;
        let input = tmp.path().join("qdrant.jsonl");
        let target = tmp.path().join("sqlrite.db");
        fs::write(
            &input,
            concat!(
                "{\"id\":\"pt-1\",\"payload\":{\"doc_id\":\"doc-1\",\"content\":\"qdrant migrated chunk\",\"source\":\"qdrant/doc-1.md\",\"tenant\":\"acme\"},\"vector\":[0.9,0.1]}\n",
                "{\"id\":\"pt-2\",\"payload\":{\"doc_id\":\"doc-2\",\"content\":\"qdrant second chunk\",\"source\":\"qdrant/doc-2.md\",\"tenant\":\"acme\"},\"vector\":{\"default\":[0.8,0.2]}}\n"
            ),
        )?;

        let report = migrate_api_jsonl(&ApiJsonlMigrationConfig {
            source_kind: ApiFirstSourceKind::Qdrant,
            input_path: input.clone(),
            target_path: target.clone(),
            runtime: RuntimeConfig::default(),
            batch_size: 16,
            create_indexes: false,
            id_field: "id".to_string(),
            doc_id_field: "payload.doc_id".to_string(),
            content_field: "payload.content".to_string(),
            embedding_field: "vector".to_string(),
            metadata_field: Some("payload".to_string()),
            source_field: Some("payload.source".to_string()),
            doc_metadata_field: Some("payload".to_string()),
            doc_source_field: Some("payload.source".to_string()),
        })?;

        assert_eq!(report.kind, "qdrant_jsonl");
        assert_eq!(report.chunks_migrated, 2);
        let db = SqlRite::open_with_config(target, RuntimeConfig::default())?;
        let results = db.search(crate::SearchRequest::text("qdrant migrated", 1))?;
        assert_eq!(results[0].chunk_id, "pt-1");
        Ok(())
    }

    #[test]
    fn sanitize_identifier_rejects_unsafe_names() {
        assert!(sanitize_identifier("safe_name").is_ok());
        assert!(sanitize_identifier("unsafe-name").is_err());
        assert!(sanitize_identifier("unsafe name").is_err());
    }
}
