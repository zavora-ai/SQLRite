mod adapter;
mod bench;
mod error;
mod eval;
mod ingest;
mod ops;
mod reindex;
mod security;
mod server;
mod vector_index;

pub use adapter::{SqlRiteToolAdapter, ToolRequest, ToolResponse, ToolSpec};
pub use bench::{BenchmarkConfig, BenchmarkLatency, BenchmarkReport, run_benchmark};
pub use error::{Result, SqlRiteError};
pub use eval::{
    EvalDataset, EvalMetricsAtK, EvalQuery, EvalReport, EvalSummary, QueryEvalResult,
    evaluate_dataset,
};
pub use ingest::{
    ChunkingStrategy, CustomHttpEmbeddingProvider, DeterministicEmbeddingProvider,
    EmbeddingProvider, EmbeddingRetryPolicy, IngestionBatchTuning, IngestionCheckpoint,
    IngestionReport, IngestionRequest, IngestionSource, IngestionWorker,
    OpenAiCompatibleEmbeddingProvider,
};
pub use ops::{HealthReport, backup_file, build_health_report, verify_backup_file};
pub use reindex::{ReindexCheckpoint, ReindexOptions, ReindexReport, reindex_embeddings};
pub use security::{
    AccessContext, AccessOperation, AccessPolicy, AllowAllPolicy, AuditEvent, AuditLogger,
    InMemoryTenantKeyRegistry, JsonlAuditLogger, SecureSqlRite, TenantKey, TenantKeyRegistry,
    rotate_tenant_encryption_key,
};
pub use server::{ServerConfig, serve_health_endpoints};
use vector_index::BuiltinVectorIndex;
pub use vector_index::{
    AnnTuningConfig, BruteForceVectorIndex, LshAnnVectorIndex, VectorCandidate, VectorIndex,
    VectorIndexMode, VectorIndexOptions, VectorStorageKind,
};

use half::f16;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, params, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const LATEST_SCHEMA_VERSION: i64 = 3;
const DOC_UPSERT_SQL: &str = "
    INSERT INTO documents (id, source, metadata) VALUES (?1, ?2, '{}')
    ON CONFLICT(id) DO UPDATE SET source = COALESCE(excluded.source, documents.source)
";
const CHUNK_UPSERT_SQL: &str = "
    INSERT INTO chunks (id, doc_id, content, metadata, embedding, embedding_dim)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6)
    ON CONFLICT(id) DO UPDATE SET
        doc_id = excluded.doc_id,
        content = excluded.content,
        metadata = excluded.metadata,
        embedding = excluded.embedding,
        embedding_dim = excluded.embedding_dim
";

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "baseline_schema",
        sql: "
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                source TEXT,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS chunks (
                rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                doc_id TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}',
                embedding BLOB NOT NULL,
                embedding_dim INTEGER NOT NULL CHECK (embedding_dim > 0),
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (doc_id) REFERENCES documents(id) ON DELETE CASCADE
            );
        ",
    },
    Migration {
        version: 2,
        name: "chunk_indexes",
        sql: "
            CREATE INDEX IF NOT EXISTS idx_chunks_doc_id ON chunks(doc_id);
            CREATE INDEX IF NOT EXISTS idx_chunks_created_at ON chunks(created_at DESC, rowid DESC);
            CREATE INDEX IF NOT EXISTS idx_documents_created_at ON documents(created_at DESC);
        ",
    },
    Migration {
        version: 3,
        name: "retrieval_index_catalog",
        sql: "
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
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInput {
    pub id: String,
    pub doc_id: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: Value,
    pub source: Option<String>,
}

impl ChunkInput {
    pub fn new(
        id: impl Into<String>,
        doc_id: impl Into<String>,
        content: impl Into<String>,
        embedding: Vec<f32>,
    ) -> Self {
        Self {
            id: id.into(),
            doc_id: doc_id.into(),
            content: content.into(),
            embedding,
            metadata: Value::Object(serde_json::Map::new()),
            source: None,
        }
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredChunk {
    pub id: String,
    pub doc_id: String,
    pub content: String,
    pub metadata: Value,
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query_text: Option<String>,
    pub query_embedding: Option<Vec<f32>>,
    pub top_k: usize,
    pub alpha: f32,
    pub candidate_limit: usize,
    pub metadata_filters: HashMap<String, String>,
    pub doc_id: Option<String>,
    pub fusion_strategy: FusionStrategy,
}

impl Default for SearchRequest {
    fn default() -> Self {
        Self {
            query_text: None,
            query_embedding: None,
            top_k: 5,
            alpha: 0.65,
            candidate_limit: 1000,
            metadata_filters: HashMap::new(),
            doc_id: None,
            fusion_strategy: FusionStrategy::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum FusionStrategy {
    #[default]
    Weighted,
    ReciprocalRankFusion {
        rank_constant: f32,
    },
}

impl SearchRequest {
    pub fn text(query_text: impl Into<String>, top_k: usize) -> Self {
        Self {
            query_text: Some(query_text.into()),
            top_k,
            ..Self::default()
        }
    }

    pub fn vector(query_embedding: Vec<f32>, top_k: usize) -> Self {
        Self {
            query_embedding: Some(query_embedding),
            top_k,
            ..Self::default()
        }
    }

    pub fn hybrid(query_text: impl Into<String>, query_embedding: Vec<f32>, top_k: usize) -> Self {
        Self {
            query_text: Some(query_text.into()),
            query_embedding: Some(query_embedding),
            top_k,
            ..Self::default()
        }
    }

    pub fn builder() -> SearchRequestBuilder {
        SearchRequestBuilder::default()
    }

    pub fn validate(&self) -> Result<()> {
        if self.query_text.is_none() && self.query_embedding.is_none() {
            return Err(SqlRiteError::MissingQuery);
        }
        if self.top_k == 0 {
            return Err(SqlRiteError::InvalidTopK);
        }
        if self.candidate_limit == 0 {
            return Err(SqlRiteError::InvalidCandidateLimit);
        }
        if self.candidate_limit < self.top_k {
            return Err(SqlRiteError::CandidateLimitTooSmall);
        }
        if !(0.0..=1.0).contains(&self.alpha) {
            return Err(SqlRiteError::InvalidAlpha);
        }
        if let FusionStrategy::ReciprocalRankFusion { rank_constant } = self.fusion_strategy
            && rank_constant <= 0.0
        {
            return Err(SqlRiteError::InvalidRrfRankConstant);
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct SearchRequestBuilder {
    inner: SearchRequest,
}

impl SearchRequestBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn query_text(mut self, value: impl Into<String>) -> Self {
        self.inner.query_text = Some(value.into());
        self
    }

    pub fn query_embedding(mut self, value: Vec<f32>) -> Self {
        self.inner.query_embedding = Some(value);
        self
    }

    pub fn top_k(mut self, value: usize) -> Self {
        self.inner.top_k = value;
        self
    }

    pub fn alpha(mut self, value: f32) -> Self {
        self.inner.alpha = value;
        self
    }

    pub fn candidate_limit(mut self, value: usize) -> Self {
        self.inner.candidate_limit = value;
        self
    }

    pub fn metadata_filter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.inner.metadata_filters.insert(key.into(), value.into());
        self
    }

    pub fn doc_id(mut self, value: impl Into<String>) -> Self {
        self.inner.doc_id = Some(value.into());
        self
    }

    pub fn fusion_strategy(mut self, value: FusionStrategy) -> Self {
        self.inner.fusion_strategy = value;
        self
    }

    pub fn reciprocal_rank_fusion(mut self, rank_constant: f32) -> Self {
        self.inner.fusion_strategy = FusionStrategy::ReciprocalRankFusion { rank_constant };
        self
    }

    pub fn build(self) -> Result<SearchRequest> {
        self.inner.validate()?;
        Ok(self.inner)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurabilityProfile {
    Balanced,
    Durable,
    FastUnsafe,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub durability_profile: DurabilityProfile,
    pub busy_timeout_ms: u64,
    pub enable_wal: bool,
    pub temp_store_memory: bool,
    pub vector_index_mode: VectorIndexMode,
    pub vector_storage_kind: VectorStorageKind,
    pub ann_tuning: AnnTuningConfig,
    pub enable_ann_persistence: bool,
    pub sqlite_mmap_size_bytes: i64,
    pub sqlite_cache_size_kib: i64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            durability_profile: DurabilityProfile::Balanced,
            busy_timeout_ms: 5_000,
            enable_wal: true,
            temp_store_memory: true,
            vector_index_mode: VectorIndexMode::BruteForce,
            vector_storage_kind: VectorStorageKind::F32,
            ann_tuning: AnnTuningConfig::default(),
            enable_ann_persistence: true,
            sqlite_mmap_size_bytes: 268_435_456,
            sqlite_cache_size_kib: 65_536,
        }
    }
}

impl RuntimeConfig {
    pub fn durable() -> Self {
        Self {
            durability_profile: DurabilityProfile::Durable,
            ..Self::default()
        }
    }

    pub fn fast_unsafe() -> Self {
        Self {
            durability_profile: DurabilityProfile::FastUnsafe,
            ..Self::default()
        }
    }

    pub fn with_vector_index_mode(mut self, mode: VectorIndexMode) -> Self {
        self.vector_index_mode = mode;
        self
    }

    pub fn with_vector_storage_kind(mut self, kind: VectorStorageKind) -> Self {
        self.vector_storage_kind = kind;
        self
    }

    pub fn with_ann_tuning(mut self, tuning: AnnTuningConfig) -> Self {
        self.ann_tuning = tuning;
        self
    }

    pub fn with_ann_persistence(mut self, enabled: bool) -> Self {
        self.enable_ann_persistence = enabled;
        self
    }

    pub fn with_sqlite_mmap_size(mut self, bytes: i64) -> Self {
        self.sqlite_mmap_size_bytes = bytes.max(0);
        self
    }

    pub fn with_sqlite_cache_size_kib(mut self, kib: i64) -> Self {
        self.sqlite_cache_size_kib = kib.max(0);
        self
    }

    fn synchronous_sql(&self) -> &'static str {
        match self.durability_profile {
            DurabilityProfile::Balanced => "NORMAL",
            DurabilityProfile::Durable => "FULL",
            DurabilityProfile::FastUnsafe => "OFF",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk_id: String,
    pub doc_id: String,
    pub content: String,
    pub metadata: Value,
    pub vector_score: f32,
    pub text_score: f32,
    pub hybrid_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorIndexStats {
    pub mode: String,
    pub storage_kind: String,
    pub dimension: Option<usize>,
    pub entries: usize,
    pub estimated_memory_bytes: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CompactionOptions {
    pub dedupe_by_content_hash: bool,
    pub prune_orphan_documents: bool,
    pub wal_checkpoint_truncate: bool,
    pub analyze: bool,
    pub vacuum: bool,
}

impl Default for CompactionOptions {
    fn default() -> Self {
        Self {
            dedupe_by_content_hash: true,
            prune_orphan_documents: true,
            wal_checkpoint_truncate: true,
            analyze: true,
            vacuum: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionReport {
    pub before_chunks: usize,
    pub after_chunks: usize,
    pub removed_chunks: usize,
    pub deduplicated_chunks: usize,
    pub before_documents: usize,
    pub after_documents: usize,
    pub orphan_documents_removed: usize,
    pub wal_checkpoint_applied: bool,
    pub analyze_applied: bool,
    pub vacuum_applied: bool,
    pub vector_index_rebuilt: bool,
    pub database_size_before_bytes: Option<u64>,
    pub database_size_after_bytes: Option<u64>,
    pub reclaimed_bytes: Option<u64>,
    pub duration_ms: f64,
}

#[derive(Debug)]
pub struct SqlRite {
    conn: Connection,
    fts_enabled: bool,
    runtime_config: RuntimeConfig,
    schema_version: i64,
    vector_index: Option<RefCell<BuiltinVectorIndex>>,
    db_path: Option<PathBuf>,
}

#[derive(Debug)]
struct ChunkRecord {
    id: String,
    doc_id: String,
    content: String,
    metadata: Value,
    embedding: Vec<f32>,
}

#[derive(Debug)]
struct ScoredChunk {
    chunk: ChunkRecord,
    vector_score: f32,
    text_score: f32,
}

#[derive(Debug, Default)]
struct FtsCandidates {
    ordered_chunk_ids: Vec<String>,
    scores: HashMap<String, f32>,
}

impl SqlRite {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let conn = Connection::open(path)?;
        Self::from_connection_with_config(conn, RuntimeConfig::default(), Some(path.to_path_buf()))
    }

    pub fn open_with_config(path: impl AsRef<Path>, config: RuntimeConfig) -> Result<Self> {
        let path = path.as_ref();
        let conn = Connection::open(path)?;
        Self::from_connection_with_config(conn, config, Some(path.to_path_buf()))
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection_with_config(conn, RuntimeConfig::default(), None)
    }

    pub fn open_in_memory_with_config(config: RuntimeConfig) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::from_connection_with_config(conn, config, None)
    }

    pub fn chunk_count(&self) -> Result<usize> {
        let count = self
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |row| {
                row.get::<_, i64>(0)
            })?;
        Ok(count as usize)
    }

    pub fn integrity_check_ok(&self) -> Result<bool> {
        let result: String = self
            .conn
            .query_row("PRAGMA integrity_check;", [], |row| row.get(0))?;
        Ok(result.eq_ignore_ascii_case("ok"))
    }

    pub fn compact(&self, options: CompactionOptions) -> Result<CompactionReport> {
        if !options.dedupe_by_content_hash
            && !options.prune_orphan_documents
            && !options.wal_checkpoint_truncate
            && !options.analyze
            && !options.vacuum
        {
            return Err(SqlRiteError::InvalidCompactionConfig(
                "at least one compaction action must be enabled".to_string(),
            ));
        }

        let started = Instant::now();
        let before_chunks = self.chunk_count()?;
        let before_documents = self.document_count()?;
        let database_size_before_bytes = self.database_file_size_bytes();

        let deduplicated_chunks = if options.dedupe_by_content_hash {
            self.delete_content_hash_duplicates()?
        } else {
            0
        };

        let orphan_documents_removed = if options.prune_orphan_documents {
            self.conn.execute(
                "DELETE FROM documents
                 WHERE NOT EXISTS (
                    SELECT 1 FROM chunks
                    WHERE chunks.doc_id = documents.id
                 )",
                [],
            )?
        } else {
            0
        };

        let mut vector_index_rebuilt = false;
        if deduplicated_chunks > 0 {
            self.rebuild_vector_index()?;
            self.persist_ann_snapshot_if_enabled()?;
            vector_index_rebuilt = true;
        }

        let wal_checkpoint_applied = options.wal_checkpoint_truncate;
        if wal_checkpoint_applied {
            self.conn
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        }

        let analyze_applied = options.analyze;
        if analyze_applied {
            self.conn.execute_batch("ANALYZE;")?;
        }

        let vacuum_applied = options.vacuum && self.db_path.is_some();
        if vacuum_applied {
            self.conn.execute_batch("VACUUM;")?;
        }

        let after_chunks = self.chunk_count()?;
        let after_documents = self.document_count()?;
        let database_size_after_bytes = self.database_file_size_bytes();
        let reclaimed_bytes = match (database_size_before_bytes, database_size_after_bytes) {
            (Some(before), Some(after)) if before >= after => Some(before - after),
            _ => None,
        };

        Ok(CompactionReport {
            before_chunks,
            after_chunks,
            removed_chunks: before_chunks.saturating_sub(after_chunks),
            deduplicated_chunks,
            before_documents,
            after_documents,
            orphan_documents_removed,
            wal_checkpoint_applied,
            analyze_applied,
            vacuum_applied,
            vector_index_rebuilt,
            database_size_before_bytes,
            database_size_after_bytes,
            reclaimed_bytes,
            duration_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    pub fn delete_chunks_by_metadata(&self, key: &str, value: &str) -> Result<usize> {
        let safe_key = sanitize_metadata_key(key)?;
        let sql = format!(
            "DELETE FROM chunks WHERE json_extract(metadata, '$.{}') = ?",
            safe_key
        );
        let deleted = self.conn.execute(&sql, params![value])?;
        if deleted > 0 {
            self.rebuild_vector_index()?;
        }
        Ok(deleted)
    }

    pub fn list_chunks_page(
        &self,
        offset: usize,
        limit: usize,
        tenant_id: Option<&str>,
    ) -> Result<Vec<StoredChunk>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut sql = String::from(
            "
            SELECT c.id, c.doc_id, c.content, c.metadata, d.source
            FROM chunks AS c
            LEFT JOIN documents AS d ON d.id = c.doc_id
            ",
        );
        let mut params = Vec::new();
        if let Some(tenant_id) = tenant_id {
            sql.push_str(" WHERE json_extract(c.metadata, '$.tenant') = ?");
            params.push(SqlValue::from(tenant_id.to_string()));
        }
        sql.push_str(" ORDER BY c.rowid ASC LIMIT ? OFFSET ?");
        params.push(SqlValue::Integer(limit as i64));
        params.push(SqlValue::Integer(offset as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params), |row| {
            let metadata_text: String = row.get(3)?;
            let metadata = serde_json::from_str::<Value>(&metadata_text).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Ok(StoredChunk {
                id: row.get(0)?,
                doc_id: row.get(1)?,
                content: row.get(2)?,
                metadata,
                source: row.get(4)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn update_chunk_metadata(&self, chunk_id: &str, metadata: &Value) -> Result<()> {
        let metadata_json = serde_json::to_string(metadata)?;
        self.conn.execute(
            "UPDATE chunks SET metadata = ?1 WHERE id = ?2",
            params![metadata_json, chunk_id],
        )?;
        Ok(())
    }

    pub fn schema_version(&self) -> i64 {
        self.schema_version
    }

    pub fn runtime_config(&self) -> &RuntimeConfig {
        &self.runtime_config
    }

    pub fn vector_index_stats(&self) -> Option<VectorIndexStats> {
        let index = self.vector_index.as_ref()?;
        let index = index.borrow();
        Some(VectorIndexStats {
            mode: index.name().to_string(),
            storage_kind: index.storage_kind().as_str().to_string(),
            dimension: index.dimension(),
            entries: index.len(),
            estimated_memory_bytes: index.estimated_memory_bytes(),
        })
    }

    pub fn ingest_chunk(&self, chunk: &ChunkInput) -> Result<()> {
        self.ingest_chunks(std::slice::from_ref(chunk))
    }

    pub fn ingest_chunks(&self, chunks: &[ChunkInput]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        self.validate_ingest_chunks(chunks)?;

        let tx = self.conn.unchecked_transaction()?;
        {
            let mut doc_stmt = tx.prepare(DOC_UPSERT_SQL)?;
            let mut chunk_stmt = tx.prepare(CHUNK_UPSERT_SQL)?;

            for chunk in chunks {
                let metadata_json = serde_json::to_string(&chunk.metadata)?;
                let embedding_dim = chunk.embedding.len() as i64;
                let embedding_blob = encode_embedding(&chunk.embedding);

                doc_stmt.execute(params![chunk.doc_id, chunk.source.as_deref()])?;
                chunk_stmt.execute(params![
                    chunk.id,
                    chunk.doc_id,
                    chunk.content,
                    metadata_json,
                    embedding_blob,
                    embedding_dim
                ])?;
            }
        }
        tx.commit()?;

        if let Some(index) = &self.vector_index {
            let mut index = index.borrow_mut();
            let upserts: Vec<(&str, &[f32])> = chunks
                .iter()
                .map(|chunk| (chunk.id.as_str(), chunk.embedding.as_slice()))
                .collect();
            index.upsert_batch(&upserts)?;
        }

        self.persist_ann_snapshot_if_enabled()?;
        Ok(())
    }

    pub fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        request.validate()?;

        let query_embedding = request.query_embedding.as_ref();
        let query_text = request.query_text.as_deref();
        let query_tokens = query_text.map(tokenize);
        let use_vector = query_embedding.is_some();
        let use_text = query_text.is_some();

        let vector_candidates = if let Some(query_vector) = query_embedding {
            self.vector_candidates(query_vector, &request)?
        } else {
            Vec::new()
        };

        let vector_score_lookup: HashMap<String, f32> = vector_candidates
            .iter()
            .map(|candidate| (candidate.chunk_id.clone(), candidate.score))
            .collect();
        let vector_candidate_ids: Vec<String> = vector_candidates
            .iter()
            .map(|candidate| candidate.chunk_id.clone())
            .collect();
        let vector_fast_path =
            !use_text && request.doc_id.is_none() && request.metadata_filters.is_empty();

        let mut text_scores = HashMap::new();
        let mut text_candidate_ids = Vec::new();
        if let Some(text) = query_text
            && self.fts_enabled
        {
            let need_text_candidates =
                !use_vector || vector_candidate_ids.len() < request.candidate_limit;
            if need_text_candidates {
                let text_limit = if use_vector {
                    request.candidate_limit.saturating_mul(2)
                } else {
                    request.candidate_limit
                };
                let fts_candidates = self
                    .fts_text_candidates(text, &request, text_limit)
                    .unwrap_or_default();
                text_candidate_ids = fts_candidates.ordered_chunk_ids;
                if !use_vector {
                    text_scores = fts_candidates.scores;
                }
            }
        }

        let fetch_ids = if vector_fast_path {
            vector_candidate_ids
                .iter()
                .take(request.top_k)
                .cloned()
                .collect()
        } else {
            merge_candidate_ids(
                &vector_candidate_ids,
                &text_candidate_ids,
                request.candidate_limit,
                use_vector,
                use_text,
            )
        };

        let candidates = if fetch_ids.is_empty() {
            self.fetch_candidate_chunks(&request)?
        } else {
            let mut items = self.fetch_chunks_by_ids(&fetch_ids)?;
            items.retain(|chunk| chunk_matches_request(chunk, &request));
            if !vector_fast_path && items.len() < request.candidate_limit {
                let fallback = self.fetch_candidate_chunks(&request)?;
                let mut seen_ids: HashSet<String> =
                    items.iter().map(|chunk| chunk.id.clone()).collect();
                for chunk in fallback {
                    if seen_ids.insert(chunk.id.clone()) {
                        items.push(chunk);
                        if items.len() >= request.candidate_limit {
                            break;
                        }
                    }
                }
            }
            items.truncate(request.candidate_limit);
            items
        };

        if let Some(text) = query_text
            && self.fts_enabled
            && text_scores.is_empty()
        {
            let candidate_ids: Vec<String> =
                candidates.iter().map(|chunk| chunk.id.clone()).collect();
            text_scores = self
                .fts_text_scores_for_ids(text, &candidate_ids)
                .unwrap_or_default();
        }

        let mut scored = Vec::with_capacity(candidates.len());

        for chunk in candidates {
            let vector_score = if let Some(query_vector) = query_embedding {
                if let Some(score) = vector_score_lookup.get(&chunk.id).copied() {
                    score
                } else if query_vector.len() == chunk.embedding.len() {
                    cosine_similarity(query_vector, &chunk.embedding)
                } else {
                    if !use_text {
                        continue;
                    }
                    0.0
                }
            } else {
                0.0
            };

            let text_score = if let Some(text) = query_text {
                let fts_score = text_scores.get(&chunk.id).copied().unwrap_or(0.0);
                if self.fts_enabled && fts_score > 0.0 {
                    fts_score
                } else {
                    lexical_overlap_score(
                        query_tokens.as_ref().expect("tokens exist"),
                        text,
                        &chunk.content,
                    )
                }
            } else {
                0.0
            };

            scored.push(ScoredChunk {
                chunk,
                vector_score,
                text_score,
            });
        }

        let hybrid_scores = compute_hybrid_scores(
            &scored,
            use_vector,
            use_text,
            request.alpha,
            request.fusion_strategy,
        );
        let mut results = Vec::with_capacity(scored.len());
        for entry in scored {
            let hybrid_score = hybrid_scores.get(&entry.chunk.id).copied().unwrap_or(0.0);
            results.push(SearchResult {
                chunk_id: entry.chunk.id,
                doc_id: entry.chunk.doc_id,
                content: entry.chunk.content,
                metadata: entry.chunk.metadata,
                vector_score: entry.vector_score,
                text_score: entry.text_score,
                hybrid_score,
            });
        }

        results.sort_by(|left, right| {
            right
                .hybrid_score
                .total_cmp(&left.hybrid_score)
                .then_with(|| right.vector_score.total_cmp(&left.vector_score))
                .then_with(|| right.text_score.total_cmp(&left.text_score))
                .then_with(|| left.chunk_id.cmp(&right.chunk_id))
        });
        results.truncate(request.top_k);
        Ok(results)
    }

    fn from_connection_with_config(
        mut conn: Connection,
        runtime_config: RuntimeConfig,
        db_path: Option<PathBuf>,
    ) -> Result<Self> {
        apply_runtime_config(&conn, &runtime_config)?;
        let schema_version = run_migrations(&mut conn)?;
        let fts_enabled = initialize_fts(&conn);
        let vector_index =
            load_vector_index(&conn, &runtime_config, db_path.as_deref())?.map(RefCell::new);

        Ok(Self {
            conn,
            fts_enabled,
            runtime_config,
            schema_version,
            vector_index,
            db_path,
        })
    }

    fn persist_ann_snapshot_if_enabled(&self) -> Result<()> {
        if !self.runtime_config.enable_ann_persistence
            || !self.runtime_config.vector_index_mode.is_ann()
        {
            return Ok(());
        }
        let Some(db_path) = self.db_path.as_deref() else {
            return Ok(());
        };
        let Some(index) = self.vector_index.as_ref() else {
            return Ok(());
        };
        let Some(snapshot_path) = ann_snapshot_path(
            db_path,
            self.runtime_config.vector_index_mode,
            self.runtime_config.vector_storage_kind,
        ) else {
            return Ok(());
        };

        let index = index.borrow();
        let entries = index.export_entries();
        save_ann_snapshot(
            &snapshot_path,
            self.runtime_config.vector_index_mode,
            self.runtime_config.vector_storage_kind,
            &entries,
        )
    }

    fn fetch_candidate_chunks(&self, request: &SearchRequest) -> Result<Vec<ChunkRecord>> {
        let mut sql = String::from(
            "SELECT id, doc_id, content, metadata, embedding, embedding_dim FROM chunks",
        );
        let mut clauses = Vec::new();
        let mut params = Vec::new();

        if let Some(doc_id) = &request.doc_id {
            clauses.push("doc_id = ?".to_string());
            params.push(SqlValue::from(doc_id.clone()));
        }

        for (key, value) in &request.metadata_filters {
            let safe_key = sanitize_metadata_key(key)?;
            clauses.push(format!("json_extract(metadata, '$.{}') = ?", safe_key));
            params.push(SqlValue::from(value.clone()));
        }

        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }

        sql.push_str(" ORDER BY created_at DESC, rowid DESC LIMIT ?");
        params.push(SqlValue::Integer(request.candidate_limit as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params), map_chunk_row)?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    fn fetch_chunks_by_ids(&self, ids: &[String]) -> Result<Vec<ChunkRecord>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut by_id: HashMap<String, ChunkRecord> = HashMap::new();
        for chunk_ids in ids.chunks(900) {
            let placeholders = std::iter::repeat_n("?", chunk_ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT id, doc_id, content, metadata, embedding, embedding_dim
                 FROM chunks
                 WHERE id IN ({})",
                placeholders
            );

            let params: Vec<SqlValue> = chunk_ids
                .iter()
                .map(|id| SqlValue::from(id.clone()))
                .collect();
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(params), map_chunk_row)?;
            for row in rows {
                let record = row?;
                by_id.insert(record.id.clone(), record);
            }
        }

        let mut ordered = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(record) = by_id.remove(id) {
                ordered.push(record);
            }
        }
        Ok(ordered)
    }

    fn vector_candidates(
        &self,
        query_embedding: &[f32],
        request: &SearchRequest,
    ) -> Result<Vec<VectorCandidate>> {
        let indexed_candidates = if let Some(index) = &self.vector_index {
            let index = index.borrow();
            if index.dimension() != Some(query_embedding.len()) {
                None
            } else {
                match index.query(query_embedding, request.candidate_limit) {
                    Ok(candidates) if !candidates.is_empty() || index.len() == 0 => {
                        Some(candidates)
                    }
                    Ok(_) | Err(_) => None,
                }
            }
        } else {
            None
        };

        if let Some(candidates) = indexed_candidates {
            return Ok(candidates);
        }

        self.brute_force_vector_candidates(query_embedding, request)
    }

    fn brute_force_vector_candidates(
        &self,
        query_embedding: &[f32],
        request: &SearchRequest,
    ) -> Result<Vec<VectorCandidate>> {
        let mut sql = String::from("SELECT id, embedding, embedding_dim FROM chunks");
        let mut clauses = Vec::new();
        let mut params = Vec::new();

        if let Some(doc_id) = &request.doc_id {
            clauses.push("doc_id = ?".to_string());
            params.push(SqlValue::from(doc_id.clone()));
        }

        for (key, value) in &request.metadata_filters {
            let safe_key = sanitize_metadata_key(key)?;
            clauses.push(format!("json_extract(metadata, '$.{}') = ?", safe_key));
            params.push(SqlValue::from(value.clone()));
        }

        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params), |row| {
            let chunk_id: String = row.get(0)?;
            let embedding_blob: Vec<u8> = row.get(1)?;
            let embedding_dim: i64 = row.get(2)?;
            Ok((chunk_id, embedding_blob, embedding_dim))
        })?;

        let mut candidates = Vec::new();
        for row in rows {
            let (chunk_id, embedding_blob, embedding_dim) = row?;
            if embedding_dim <= 0 || embedding_dim as usize != query_embedding.len() {
                continue;
            }

            let embedding = decode_embedding(&embedding_blob, embedding_dim as usize)?;
            let score = cosine_similarity(query_embedding, &embedding);
            candidates.push(VectorCandidate { chunk_id, score });
        }

        candidates.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.chunk_id.cmp(&right.chunk_id))
        });
        candidates.truncate(request.candidate_limit);
        Ok(candidates)
    }

    fn fts_text_candidates(
        &self,
        query_text: &str,
        request: &SearchRequest,
        limit: usize,
    ) -> Result<FtsCandidates> {
        if limit == 0 {
            return Ok(FtsCandidates::default());
        }

        let Some(match_query) = build_fts_match_query(query_text) else {
            return Ok(FtsCandidates::default());
        };

        let mut sql;
        let mut params = vec![SqlValue::from(match_query)];
        if request.metadata_filters.is_empty() {
            sql = String::from(
                "SELECT chunk_id, bm25(chunks_fts) AS rank
                 FROM chunks_fts
                 WHERE chunks_fts MATCH ?",
            );
            if let Some(doc_id) = &request.doc_id {
                sql.push_str(" AND doc_id = ?");
                params.push(SqlValue::from(doc_id.clone()));
            }
            sql.push_str(" ORDER BY rank ASC, chunk_id ASC LIMIT ?");
            params.push(SqlValue::Integer(limit as i64));
        } else {
            sql = String::from(
                "SELECT f.chunk_id, bm25(chunks_fts) AS rank
                 FROM chunks_fts AS f
                 INNER JOIN chunks AS c ON c.id = f.chunk_id
                 WHERE chunks_fts MATCH ?",
            );
            if let Some(doc_id) = &request.doc_id {
                sql.push_str(" AND c.doc_id = ?");
                params.push(SqlValue::from(doc_id.clone()));
            }
            for (key, value) in &request.metadata_filters {
                let safe_key = sanitize_metadata_key(key)?;
                sql.push_str(&format!(
                    " AND json_extract(c.metadata, '$.{}') = ?",
                    safe_key
                ));
                params.push(SqlValue::from(value.clone()));
            }
            sql.push_str(" ORDER BY rank ASC, f.chunk_id ASC LIMIT ?");
            params.push(SqlValue::Integer(limit as i64));
        }

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params), |row| {
            let chunk_id: String = row.get(0)?;
            let rank: f64 = row.get(1)?;
            Ok((chunk_id, rank))
        })?;

        let mut ranked = Vec::new();
        for row in rows {
            ranked.push(row?);
        }
        if ranked.is_empty() {
            return Ok(FtsCandidates::default());
        }

        let min_rank = ranked
            .iter()
            .map(|(_, rank)| *rank)
            .fold(f64::INFINITY, f64::min);

        let mut scores = HashMap::with_capacity(ranked.len());
        let mut ordered_chunk_ids = Vec::with_capacity(ranked.len());
        for (chunk_id, rank) in ranked {
            let normalized = 1.0 / (1.0 + (rank - min_rank).max(0.0) as f32);
            scores.insert(chunk_id.clone(), normalized);
            ordered_chunk_ids.push(chunk_id);
        }
        Ok(FtsCandidates {
            ordered_chunk_ids,
            scores,
        })
    }

    fn fts_text_scores_for_ids(
        &self,
        query_text: &str,
        candidate_ids: &[String],
    ) -> Result<HashMap<String, f32>> {
        if candidate_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let Some(match_query) = build_fts_match_query(query_text) else {
            return Ok(HashMap::new());
        };

        let mut ranked = Vec::new();
        for ids in candidate_ids.chunks(900) {
            let placeholders = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT chunk_id, bm25(chunks_fts) AS rank
                 FROM chunks_fts
                 WHERE chunks_fts MATCH ? AND chunk_id IN ({})",
                placeholders
            );

            let mut params = Vec::with_capacity(ids.len() + 1);
            params.push(SqlValue::from(match_query.clone()));
            for id in ids {
                params.push(SqlValue::from(id.clone()));
            }

            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(params), |row| {
                let chunk_id: String = row.get(0)?;
                let rank: f64 = row.get(1)?;
                Ok((chunk_id, rank))
            })?;
            for row in rows {
                ranked.push(row?);
            }
        }

        if ranked.is_empty() {
            return Ok(HashMap::new());
        }

        let min_rank = ranked
            .iter()
            .map(|(_, rank)| *rank)
            .fold(f64::INFINITY, f64::min);

        let mut scores = HashMap::with_capacity(ranked.len());
        for (chunk_id, rank) in ranked {
            let normalized = 1.0 / (1.0 + (rank - min_rank).max(0.0) as f32);
            scores
                .entry(chunk_id)
                .and_modify(|existing| {
                    if normalized > *existing {
                        *existing = normalized;
                    }
                })
                .or_insert(normalized);
        }
        Ok(scores)
    }

    fn document_count(&self) -> Result<usize> {
        let count = self
            .conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| {
                row.get::<_, i64>(0)
            })?;
        Ok(count as usize)
    }

    fn database_file_size_bytes(&self) -> Option<u64> {
        self.db_path
            .as_ref()
            .and_then(|path| fs::metadata(path).ok())
            .map(|meta| meta.len())
    }

    fn delete_content_hash_duplicates(&self) -> Result<usize> {
        let deleted = self.conn.execute(
            "
            DELETE FROM chunks
            WHERE rowid IN (
                SELECT c.rowid
                FROM chunks AS c
                JOIN (
                    SELECT
                        doc_id,
                        COALESCE(json_extract(metadata, '$.tenant'), '') AS tenant,
                        json_extract(metadata, '$.content_hash') AS content_hash,
                        MAX(rowid) AS keep_rowid
                    FROM chunks
                    WHERE json_extract(metadata, '$.content_hash') IS NOT NULL
                    GROUP BY
                        doc_id,
                        COALESCE(json_extract(metadata, '$.tenant'), ''),
                        json_extract(metadata, '$.content_hash')
                    HAVING COUNT(*) > 1
                ) AS dup
                ON c.doc_id = dup.doc_id
                AND COALESCE(json_extract(c.metadata, '$.tenant'), '') = dup.tenant
                AND json_extract(c.metadata, '$.content_hash') = dup.content_hash
                WHERE c.rowid <> dup.keep_rowid
            )
            ",
            [],
        )?;
        Ok(deleted)
    }

    fn validate_ingest_chunks(&self, chunks: &[ChunkInput]) -> Result<()> {
        let enforce_dimension = self.vector_index.is_some();
        let mut expected_dimension = self
            .vector_index
            .as_ref()
            .and_then(|index| index.borrow().dimension());

        for chunk in chunks {
            if chunk.embedding.is_empty() {
                return Err(SqlRiteError::EmptyEmbedding);
            }

            if let Some(expected) = expected_dimension {
                if expected != chunk.embedding.len() {
                    return Err(SqlRiteError::EmbeddingDimensionMismatch {
                        expected,
                        found: chunk.embedding.len(),
                    });
                }
            } else if enforce_dimension {
                expected_dimension = Some(chunk.embedding.len());
            }
        }

        Ok(())
    }

    fn rebuild_vector_index(&self) -> Result<()> {
        let Some(index) = &self.vector_index else {
            return Ok(());
        };

        let mut index = index.borrow_mut();
        index.reset()?;

        let mut stmt = self.conn.prepare(
            "SELECT id, embedding, embedding_dim
             FROM chunks
             ORDER BY rowid ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let embedding_blob: Vec<u8> = row.get(1)?;
            let embedding_dim: i64 = row.get(2)?;
            let embedding =
                decode_embedding(&embedding_blob, embedding_dim as usize).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Blob,
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                    )
                })?;
            Ok((id, embedding))
        })?;

        let mut batch: Vec<(String, Vec<f32>)> = Vec::with_capacity(1024);
        for row in rows {
            let (id, embedding) = row?;
            batch.push((id, embedding));
            if batch.len() >= 1024 {
                let refs: Vec<(&str, &[f32])> = batch
                    .iter()
                    .map(|(chunk_id, embedding)| (chunk_id.as_str(), embedding.as_slice()))
                    .collect();
                index.upsert_batch(&refs)?;
                batch.clear();
            }
        }

        if !batch.is_empty() {
            let refs: Vec<(&str, &[f32])> = batch
                .iter()
                .map(|(chunk_id, embedding)| (chunk_id.as_str(), embedding.as_slice()))
                .collect();
            index.upsert_batch(&refs)?;
        }

        Ok(())
    }
}

fn map_chunk_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChunkRecord> {
    let metadata_text: String = row.get(3)?;
    let embedding_blob: Vec<u8> = row.get(4)?;
    let embedding_dim: i64 = row.get(5)?;
    let metadata = serde_json::from_str::<Value>(&metadata_text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let embedding = decode_embedding(&embedding_blob, embedding_dim as usize).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Blob,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;

    Ok(ChunkRecord {
        id: row.get(0)?,
        doc_id: row.get(1)?,
        content: row.get(2)?,
        metadata,
        embedding,
    })
}

fn chunk_matches_request(chunk: &ChunkRecord, request: &SearchRequest) -> bool {
    if let Some(doc_id) = &request.doc_id
        && &chunk.doc_id != doc_id
    {
        return false;
    }

    for (key, value) in &request.metadata_filters {
        let Some(actual) = chunk.metadata.get(key) else {
            return false;
        };

        let matches = if let Some(actual_text) = actual.as_str() {
            actual_text == value
        } else if let Ok(parsed_value) = serde_json::from_str::<Value>(value) {
            &parsed_value == actual
        } else {
            false
        };

        if !matches {
            return false;
        }
    }

    true
}

fn merge_candidate_ids(
    vector_ids: &[String],
    text_ids: &[String],
    limit: usize,
    use_vector: bool,
    use_text: bool,
) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }

    let mut merged = Vec::with_capacity(limit);
    let mut seen = HashSet::with_capacity(limit.saturating_mul(2));

    if use_vector {
        for id in vector_ids {
            if seen.insert(id.clone()) {
                merged.push(id.clone());
                if merged.len() >= limit {
                    return merged;
                }
            }
        }
    }

    if use_text {
        for id in text_ids {
            if seen.insert(id.clone()) {
                merged.push(id.clone());
                if merged.len() >= limit {
                    break;
                }
            }
        }
    }

    merged
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnnSnapshotFile {
    version: u32,
    mode: String,
    storage_kind: String,
    entries: Vec<AnnSnapshotEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnnSnapshotEntry {
    chunk_id: String,
    vector: AnnSnapshotVector,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "encoding", rename_all = "snake_case")]
enum AnnSnapshotVector {
    F32 { values: Vec<f32> },
    F16 { values: Vec<u16> },
    Int8 { values: Vec<i8>, scale: f32 },
}

fn load_vector_index(
    conn: &Connection,
    runtime_config: &RuntimeConfig,
    db_path: Option<&Path>,
) -> Result<Option<BuiltinVectorIndex>> {
    let options = VectorIndexOptions {
        storage_kind: runtime_config.vector_storage_kind,
        ann_tuning: runtime_config.ann_tuning,
    };
    let Some(mut index) = BuiltinVectorIndex::from_mode(runtime_config.vector_index_mode, options)
    else {
        return Ok(None);
    };

    let snapshot_path =
        if runtime_config.enable_ann_persistence && runtime_config.vector_index_mode.is_ann() {
            db_path.and_then(|path| {
                ann_snapshot_path(
                    path,
                    runtime_config.vector_index_mode,
                    runtime_config.vector_storage_kind,
                )
            })
        } else {
            None
        };

    if let (Some(path), Some(db_file)) = (snapshot_path.as_ref(), db_path)
        && ann_snapshot_is_fresh(path, db_file)
        && let Ok(snapshot) = load_ann_snapshot(path)
        && snapshot.mode == runtime_config.vector_index_mode.as_str()
        && snapshot.storage_kind == runtime_config.vector_storage_kind.as_str()
    {
        let entries = snapshot
            .entries
            .into_iter()
            .map(|entry| (entry.chunk_id, decode_snapshot_vector(entry.vector)))
            .collect::<Vec<_>>();
        if index.import_entries(&entries).is_ok() {
            return Ok(Some(index));
        }
    }

    let mut stmt = conn.prepare(
        "SELECT id, embedding, embedding_dim
         FROM chunks
         ORDER BY rowid ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let embedding_blob: Vec<u8> = row.get(1)?;
        let embedding_dim: i64 = row.get(2)?;
        let embedding = decode_embedding(&embedding_blob, embedding_dim as usize).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Blob,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?;
        Ok((id, embedding))
    })?;

    let mut batch: Vec<(String, Vec<f32>)> = Vec::with_capacity(1024);
    for row in rows {
        let (id, embedding) = row?;
        batch.push((id, embedding));
        if batch.len() >= 1024 {
            let refs: Vec<(&str, &[f32])> = batch
                .iter()
                .map(|(chunk_id, embedding)| (chunk_id.as_str(), embedding.as_slice()))
                .collect();
            index.upsert_batch(&refs)?;
            batch.clear();
        }
    }

    if !batch.is_empty() {
        let refs: Vec<(&str, &[f32])> = batch
            .iter()
            .map(|(chunk_id, embedding)| (chunk_id.as_str(), embedding.as_slice()))
            .collect();
        index.upsert_batch(&refs)?;
    }

    if let Some(path) = snapshot_path.as_ref() {
        let entries = index.export_entries();
        let _ = save_ann_snapshot(
            path,
            runtime_config.vector_index_mode,
            runtime_config.vector_storage_kind,
            &entries,
        );
    }

    Ok(Some(index))
}

fn ann_snapshot_path(
    db_path: &Path,
    mode: VectorIndexMode,
    storage_kind: VectorStorageKind,
) -> Option<PathBuf> {
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    let file_stem = db_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("sqlrite");
    Some(parent.join(format!(
        ".{file_stem}.ann.{}.{}.json",
        mode.as_str(),
        storage_kind.as_str()
    )))
}

fn ann_snapshot_is_fresh(snapshot_path: &Path, db_path: &Path) -> bool {
    let snapshot_meta = fs::metadata(snapshot_path).ok();
    let db_meta = fs::metadata(db_path).ok();
    let Some(snapshot_mtime) = snapshot_meta.and_then(|meta| meta.modified().ok()) else {
        return false;
    };
    let Some(db_mtime) = db_meta.and_then(|meta| meta.modified().ok()) else {
        return false;
    };
    snapshot_mtime >= db_mtime
}

fn load_ann_snapshot(path: &Path) -> Result<AnnSnapshotFile> {
    let raw = fs::read_to_string(path)?;
    let snapshot: AnnSnapshotFile = serde_json::from_str(&raw)?;
    Ok(snapshot)
}

fn save_ann_snapshot(
    path: &Path,
    mode: VectorIndexMode,
    storage_kind: VectorStorageKind,
    entries: &[(String, Vec<f32>)],
) -> Result<()> {
    let payload = AnnSnapshotFile {
        version: 1,
        mode: mode.as_str().to_string(),
        storage_kind: storage_kind.as_str().to_string(),
        entries: entries
            .iter()
            .map(|(chunk_id, embedding)| AnnSnapshotEntry {
                chunk_id: chunk_id.clone(),
                vector: encode_snapshot_vector(embedding, storage_kind),
            })
            .collect(),
    };

    let raw = serde_json::to_string_pretty(&payload)?;
    fs::write(path, raw)?;
    Ok(())
}

fn encode_snapshot_vector(embedding: &[f32], storage_kind: VectorStorageKind) -> AnnSnapshotVector {
    match storage_kind {
        VectorStorageKind::F32 => AnnSnapshotVector::F32 {
            values: embedding.to_vec(),
        },
        VectorStorageKind::F16 => AnnSnapshotVector::F16 {
            values: embedding
                .iter()
                .map(|value| f16::from_f32(*value).to_bits())
                .collect(),
        },
        VectorStorageKind::Int8 => {
            let max_abs = embedding
                .iter()
                .fold(0.0f32, |acc, value| acc.max(value.abs()))
                .max(1e-6);
            let scale = max_abs / 127.0;
            let values = embedding
                .iter()
                .map(|value| ((*value / scale).round().clamp(-127.0, 127.0)) as i8)
                .collect::<Vec<_>>();
            AnnSnapshotVector::Int8 { values, scale }
        }
    }
}

fn decode_snapshot_vector(vector: AnnSnapshotVector) -> Vec<f32> {
    match vector {
        AnnSnapshotVector::F32 { values } => values,
        AnnSnapshotVector::F16 { values } => values
            .into_iter()
            .map(|bits| f16::from_bits(bits).to_f32())
            .collect(),
        AnnSnapshotVector::Int8 { values, scale } => values
            .into_iter()
            .map(|value| value as f32 * scale)
            .collect(),
    }
}

fn compute_hybrid_scores(
    scored: &[ScoredChunk],
    use_vector: bool,
    use_text: bool,
    alpha: f32,
    fusion_strategy: FusionStrategy,
) -> HashMap<String, f32> {
    if scored.is_empty() {
        return HashMap::new();
    }

    match (use_vector, use_text, fusion_strategy) {
        (true, true, FusionStrategy::Weighted) => scored
            .iter()
            .map(|entry| {
                (
                    entry.chunk.id.clone(),
                    alpha * entry.vector_score + (1.0 - alpha) * entry.text_score,
                )
            })
            .collect(),
        (true, true, FusionStrategy::ReciprocalRankFusion { rank_constant }) => {
            let vector_ranks = rank_lookup(
                scored
                    .iter()
                    .map(|entry| (&entry.chunk.id, entry.vector_score)),
            );
            let text_ranks = rank_lookup(
                scored
                    .iter()
                    .map(|entry| (&entry.chunk.id, entry.text_score)),
            );

            scored
                .iter()
                .map(|entry| {
                    let vector_term = vector_ranks
                        .get(&entry.chunk.id)
                        .copied()
                        .map(|rank| 1.0 / (rank_constant + rank as f32))
                        .unwrap_or(0.0);
                    let text_term = text_ranks
                        .get(&entry.chunk.id)
                        .copied()
                        .map(|rank| 1.0 / (rank_constant + rank as f32))
                        .unwrap_or(0.0);
                    (entry.chunk.id.clone(), vector_term + text_term)
                })
                .collect()
        }
        (true, false, _) => scored
            .iter()
            .map(|entry| (entry.chunk.id.clone(), entry.vector_score))
            .collect(),
        (false, true, _) => scored
            .iter()
            .map(|entry| (entry.chunk.id.clone(), entry.text_score))
            .collect(),
        (false, false, _) => scored
            .iter()
            .map(|entry| (entry.chunk.id.clone(), 0.0))
            .collect(),
    }
}

fn rank_lookup<'a>(items: impl Iterator<Item = (&'a String, f32)>) -> HashMap<String, usize> {
    let mut ranked: Vec<(String, f32)> = items.map(|(id, score)| (id.clone(), score)).collect();
    ranked.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });

    ranked
        .into_iter()
        .enumerate()
        .map(|(idx, (id, _))| (id, idx + 1))
        .collect()
}

fn apply_runtime_config(conn: &Connection, config: &RuntimeConfig) -> Result<()> {
    conn.busy_timeout(Duration::from_millis(config.busy_timeout_ms))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "synchronous", config.synchronous_sql())?;

    if config.enable_wal {
        let _: String = conn.query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))?;
    } else {
        let _: String = conn.query_row("PRAGMA journal_mode = DELETE;", [], |row| row.get(0))?;
    }

    if config.temp_store_memory {
        conn.pragma_update(None, "temp_store", "MEMORY")?;
    }

    if config.sqlite_cache_size_kib > 0 {
        let cache_pages_kib = -config.sqlite_cache_size_kib;
        conn.pragma_update(None, "cache_size", cache_pages_kib)?;
    }

    if config.sqlite_mmap_size_bytes > 0 {
        conn.pragma_update(None, "mmap_size", config.sqlite_mmap_size_bytes)?;
    }

    Ok(())
}

fn run_migrations(conn: &mut Connection) -> Result<i64> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        ",
    )?;

    let mut applied = HashSet::new();
    {
        let mut applied_stmt = conn.prepare("SELECT version FROM schema_migrations")?;
        let applied_rows = applied_stmt.query_map([], |row| row.get::<_, i64>(0))?;
        for row in applied_rows {
            applied.insert(row?);
        }
    }

    for migration in MIGRATIONS {
        if applied.contains(&migration.version) {
            continue;
        }

        let tx = conn.transaction()?;
        tx.execute_batch(migration.sql)?;
        tx.execute(
            "INSERT OR IGNORE INTO schema_migrations (version, name) VALUES (?1, ?2)",
            params![migration.version, migration.name],
        )?;
        tx.commit()?;
    }

    let schema_version = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )?;

    Ok(schema_version.min(LATEST_SCHEMA_VERSION))
}

fn initialize_fts(conn: &Connection) -> bool {
    let enabled = conn
        .execute_batch(
            "
            CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
                content,
                chunk_id UNINDEXED,
                doc_id UNINDEXED
            );
            ",
        )
        .is_ok();

    if !enabled {
        return false;
    }

    let _ = conn.execute_batch(
        "
        CREATE TRIGGER IF NOT EXISTS chunks_fts_after_insert
        AFTER INSERT ON chunks
        BEGIN
            INSERT INTO chunks_fts (content, chunk_id, doc_id)
            VALUES (new.content, new.id, new.doc_id);
        END;

        CREATE TRIGGER IF NOT EXISTS chunks_fts_after_update
        AFTER UPDATE ON chunks
        BEGIN
            DELETE FROM chunks_fts WHERE chunk_id = old.id;
            INSERT INTO chunks_fts (content, chunk_id, doc_id)
            VALUES (new.content, new.id, new.doc_id);
        END;

        CREATE TRIGGER IF NOT EXISTS chunks_fts_after_delete
        AFTER DELETE ON chunks
        BEGIN
            DELETE FROM chunks_fts WHERE chunk_id = old.id;
        END;
        ",
    );

    let _ = conn.execute(
        "
        INSERT INTO chunks_fts (content, chunk_id, doc_id)
        SELECT c.content, c.id, c.doc_id
        FROM chunks AS c
        WHERE NOT EXISTS (
            SELECT 1
            FROM chunks_fts AS f
            WHERE f.chunk_id = c.id
        )
        ",
        [],
    );

    true
}

fn sanitize_metadata_key(key: &str) -> Result<&str> {
    if !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Ok(key)
    } else {
        Err(SqlRiteError::InvalidFilterKey(key.to_string()))
    }
}

fn build_fts_match_query(query_text: &str) -> Option<String> {
    let mut terms: Vec<String> = query_text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect();
    if terms.is_empty() {
        return None;
    }
    terms.sort();
    terms.dedup();

    Some(terms.join(" OR "))
}

fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn decode_embedding(bytes: &[u8], dim: usize) -> std::result::Result<Vec<f32>, SqlRiteError> {
    let expected = dim * 4;
    if bytes.len() != expected {
        return Err(SqlRiteError::InvalidEmbeddingBytes {
            expected_bytes: expected,
            found_bytes: bytes.len(),
        });
    }

    let mut out = Vec::with_capacity(dim);
    for chunk in bytes.chunks_exact(4) {
        let arr = [chunk[0], chunk[1], chunk[2], chunk[3]];
        out.push(f32::from_le_bytes(arr));
    }
    Ok(out)
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let dot: f32 = left.iter().zip(right.iter()).map(|(a, b)| a * b).sum();
    let left_norm = left.iter().map(|v| v * v).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|v| v * v).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }
    dot / (left_norm * right_norm)
}

fn tokenize(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn lexical_overlap_score(query_tokens: &HashSet<String>, query_text: &str, content: &str) -> f32 {
    if query_tokens.is_empty() {
        return 0.0;
    }

    let content_tokens = tokenize(content);
    let overlap = query_tokens.intersection(&content_tokens).count() as f32;
    let base = overlap / query_tokens.len() as f32;

    if content.to_lowercase().contains(&query_text.to_lowercase()) {
        (base + 0.15).min(1.0)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn seed(db: &SqlRite) -> Result<()> {
        db.ingest_chunks(&[
            ChunkInput {
                id: "c1".to_string(),
                doc_id: "d1".to_string(),
                content: "Rust powers AI agents with safe systems code.".to_string(),
                embedding: vec![1.0, 0.0, 0.0],
                metadata: json!({"tenant": "acme", "topic": "rust"}),
                source: Some("docs/rust.txt".to_string()),
            },
            ChunkInput {
                id: "c2".to_string(),
                doc_id: "d2".to_string(),
                content: "PostgreSQL handles relational workloads at scale.".to_string(),
                embedding: vec![0.0, 1.0, 0.0],
                metadata: json!({"tenant": "acme", "topic": "postgres"}),
                source: Some("docs/postgres.txt".to_string()),
            },
            ChunkInput {
                id: "c3".to_string(),
                doc_id: "d1".to_string(),
                content: "SQLite is excellent for local-first RAG memory.".to_string(),
                embedding: vec![0.8, 0.2, 0.0],
                metadata: json!({"tenant": "beta", "topic": "sqlite"}),
                source: Some("docs/sqlite.txt".to_string()),
            },
        ])?;
        Ok(())
    }

    #[test]
    fn vector_search_ranks_by_similarity() -> Result<()> {
        let db = SqlRite::open_in_memory()?;
        seed(&db)?;

        let results = db.search(SearchRequest {
            query_embedding: Some(vec![0.95, 0.05, 0.0]),
            top_k: 2,
            ..Default::default()
        })?;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].chunk_id, "c1");
        Ok(())
    }

    #[test]
    fn hybrid_search_matches_text_and_vector() -> Result<()> {
        let db = SqlRite::open_in_memory()?;
        seed(&db)?;

        let results = db.search(SearchRequest {
            query_text: Some("local rag memory sqlite".to_string()),
            query_embedding: Some(vec![0.7, 0.3, 0.0]),
            alpha: 0.5,
            top_k: 1,
            ..Default::default()
        })?;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, "c3");
        Ok(())
    }

    #[test]
    fn metadata_filter_restricts_results() -> Result<()> {
        let db = SqlRite::open_in_memory()?;
        seed(&db)?;

        let mut filters = HashMap::new();
        filters.insert("tenant".to_string(), "acme".to_string());

        let results = db.search(SearchRequest {
            query_text: Some("ai systems".to_string()),
            metadata_filters: filters,
            top_k: 10,
            ..Default::default()
        })?;

        assert!(results.iter().all(|r| r.metadata["tenant"] == "acme"));
        Ok(())
    }

    #[test]
    fn schema_migrations_are_applied() -> Result<()> {
        let db = SqlRite::open_in_memory()?;
        assert_eq!(db.schema_version(), LATEST_SCHEMA_VERSION);

        let migration_count =
            db.conn
                .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                    row.get::<_, i64>(0)
                })?;
        assert_eq!(migration_count, MIGRATIONS.len() as i64);
        Ok(())
    }

    #[test]
    fn retrieval_index_catalog_migration_objects_exist() -> Result<()> {
        let db = SqlRite::open_in_memory()?;

        let table_exists: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'retrieval_indexes'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(table_exists, 1);

        let view_exists: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'view' AND name = 'retrieval_index_catalog'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(view_exists, 1);

        Ok(())
    }

    #[test]
    fn builder_validates_parameters() {
        let err = SearchRequest::builder()
            .query_text("agents")
            .top_k(0)
            .build()
            .expect_err("invalid top_k should fail");
        assert!(matches!(err, SqlRiteError::InvalidTopK));

        let err = SearchRequest::builder()
            .query_text("agents")
            .reciprocal_rank_fusion(0.0)
            .build()
            .expect_err("invalid rrf constant should fail");
        assert!(matches!(err, SqlRiteError::InvalidRrfRankConstant));
    }

    #[test]
    fn convenience_constructors_work() -> Result<()> {
        let chunk = ChunkInput::new("c1", "d1", "content", vec![1.0, 0.0])
            .with_metadata(json!({"tenant": "acme"}))
            .with_source("docs/c1.md");
        assert_eq!(chunk.id, "c1");
        assert_eq!(chunk.source.as_deref(), Some("docs/c1.md"));

        let req = SearchRequest::hybrid("hello", vec![1.0, 0.0], 3);
        assert_eq!(req.top_k, 3);
        assert_eq!(req.query_text.as_deref(), Some("hello"));
        assert!(req.query_embedding.is_some());
        Ok(())
    }

    #[test]
    fn runtime_config_applies_synchronous_profile() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::durable())?;
        let synchronous = db
            .conn
            .query_row("PRAGMA synchronous;", [], |row| row.get::<_, i64>(0))?;
        assert_eq!(synchronous, 2);
        Ok(())
    }

    #[test]
    fn deterministic_tie_break_uses_chunk_id() -> Result<()> {
        let db = SqlRite::open_in_memory()?;
        db.ingest_chunks(&[
            ChunkInput {
                id: "z-chunk".to_string(),
                doc_id: "doc-1".to_string(),
                content: "same content".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({}),
                source: None,
            },
            ChunkInput {
                id: "a-chunk".to_string(),
                doc_id: "doc-2".to_string(),
                content: "same content".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({}),
                source: None,
            },
        ])?;

        let results = db.search(SearchRequest {
            query_embedding: Some(vec![1.0, 0.0]),
            top_k: 2,
            ..Default::default()
        })?;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].chunk_id, "a-chunk");
        Ok(())
    }

    #[test]
    fn deterministic_order_is_stable_across_repeated_runs() -> Result<()> {
        let db = SqlRite::open_in_memory()?;
        db.ingest_chunks(&[
            ChunkInput {
                id: "a-chunk".to_string(),
                doc_id: "d1".to_string(),
                content: "same".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({}),
                source: None,
            },
            ChunkInput {
                id: "b-chunk".to_string(),
                doc_id: "d2".to_string(),
                content: "same".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({}),
                source: None,
            },
        ])?;

        let request = SearchRequest::builder()
            .query_text("same")
            .query_embedding(vec![1.0, 0.0])
            .alpha(0.5)
            .top_k(2)
            .candidate_limit(2)
            .build()?;

        for _ in 0..5 {
            let results = db.search(request.clone())?;
            let ids: Vec<&str> = results.iter().map(|item| item.chunk_id.as_str()).collect();
            assert_eq!(ids, vec!["a-chunk", "b-chunk"]);
        }

        Ok(())
    }

    #[test]
    fn index_mode_rejects_mixed_embedding_dimensions() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(
            RuntimeConfig::default().with_vector_index_mode(VectorIndexMode::BruteForce),
        )?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "alpha".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;

        let err = db
            .ingest_chunk(&ChunkInput {
                id: "c2".to_string(),
                doc_id: "d2".to_string(),
                content: "beta".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({}),
                source: None,
            })
            .expect_err("mixed dimensions should fail in indexed mode");
        assert!(matches!(
            err,
            SqlRiteError::EmbeddingDimensionMismatch { .. }
        ));
        Ok(())
    }

    #[test]
    fn disabled_index_allows_mixed_embedding_dimensions() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(
            RuntimeConfig::default().with_vector_index_mode(VectorIndexMode::Disabled),
        )?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "alpha".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;
        db.ingest_chunk(&ChunkInput {
            id: "c2".to_string(),
            doc_id: "d2".to_string(),
            content: "beta".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;
        assert_eq!(db.chunk_count()?, 2);
        Ok(())
    }

    #[test]
    fn lsh_ann_mode_rejects_mixed_embedding_dimensions() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(
            RuntimeConfig::default().with_vector_index_mode(VectorIndexMode::LshAnn),
        )?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "alpha".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;

        let err = db
            .ingest_chunk(&ChunkInput {
                id: "c2".to_string(),
                doc_id: "d2".to_string(),
                content: "beta".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({}),
                source: None,
            })
            .expect_err("mixed dimensions should fail in lsh_ann mode");
        assert!(matches!(
            err,
            SqlRiteError::EmbeddingDimensionMismatch { .. }
        ));
        Ok(())
    }

    #[test]
    fn hnsw_baseline_mode_rejects_mixed_embedding_dimensions() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(
            RuntimeConfig::default().with_vector_index_mode(VectorIndexMode::HnswBaseline),
        )?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "alpha".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;

        let err = db
            .ingest_chunk(&ChunkInput {
                id: "c2".to_string(),
                doc_id: "d2".to_string(),
                content: "beta".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({}),
                source: None,
            })
            .expect_err("mixed dimensions should fail in hnsw_baseline mode");
        assert!(matches!(
            err,
            SqlRiteError::EmbeddingDimensionMismatch { .. }
        ));
        Ok(())
    }

    #[test]
    fn ann_snapshot_round_trip_f16_precision() {
        let original = vec![0.12345, -0.34567, 0.99991, -0.00123, 0.5, -0.5];
        let encoded = encode_snapshot_vector(&original, VectorStorageKind::F16);
        let decoded = decode_snapshot_vector(encoded);
        assert_eq!(decoded.len(), original.len());
        for (left, right) in decoded.iter().zip(original.iter()) {
            assert!(
                (left - right).abs() < 0.001,
                "f16 round-trip drift too high"
            );
        }
    }

    #[test]
    fn ann_snapshot_round_trip_int8_precision() {
        let original = vec![1.0, -1.0, 0.75, -0.5, 0.1, -0.05, 0.0];
        let encoded = encode_snapshot_vector(&original, VectorStorageKind::Int8);
        let decoded = decode_snapshot_vector(encoded);
        assert_eq!(decoded.len(), original.len());
        for (left, right) in decoded.iter().zip(original.iter()) {
            assert!(
                (left - right).abs() < 0.02,
                "int8 round-trip drift too high"
            );
        }
    }

    #[test]
    fn ann_snapshot_persists_for_file_backed_ann_index() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().join("ann_snapshot_test.db");
        let runtime = RuntimeConfig::default()
            .with_vector_index_mode(VectorIndexMode::HnswBaseline)
            .with_vector_storage_kind(VectorStorageKind::Int8)
            .with_ann_persistence(true);

        {
            let db = SqlRite::open_with_config(&db_path, runtime)?;
            db.ingest_chunks(&[
                ChunkInput {
                    id: "c1".to_string(),
                    doc_id: "d1".to_string(),
                    content: "alpha".to_string(),
                    embedding: vec![1.0, 0.0, 0.0],
                    metadata: json!({}),
                    source: None,
                },
                ChunkInput {
                    id: "c2".to_string(),
                    doc_id: "d2".to_string(),
                    content: "beta".to_string(),
                    embedding: vec![0.8, 0.2, 0.0],
                    metadata: json!({}),
                    source: None,
                },
            ])?;
        }

        let snapshot_path = ann_snapshot_path(
            &db_path,
            VectorIndexMode::HnswBaseline,
            VectorStorageKind::Int8,
        )
        .expect("expected snapshot path");
        assert!(snapshot_path.exists(), "snapshot file should be created");

        let snapshot = load_ann_snapshot(&snapshot_path)?;
        assert_eq!(snapshot.version, 1);
        assert_eq!(snapshot.mode, "hnsw_baseline");
        assert_eq!(snapshot.storage_kind, "int8");
        assert_eq!(snapshot.entries.len(), 2);
        assert!(
            snapshot
                .entries
                .iter()
                .all(|entry| matches!(entry.vector, AnnSnapshotVector::Int8 { .. })),
            "expected int8 encoded vectors"
        );
        Ok(())
    }

    #[test]
    fn compaction_deduplicates_chunks_and_rebuilds_index() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(
            RuntimeConfig::default().with_vector_index_mode(VectorIndexMode::BruteForce),
        )?;
        db.ingest_chunks(&[
            ChunkInput {
                id: "c1".to_string(),
                doc_id: "d1".to_string(),
                content: "same-content-a".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({"tenant": "acme", "content_hash": "hash-1"}),
                source: None,
            },
            ChunkInput {
                id: "c2".to_string(),
                doc_id: "d1".to_string(),
                content: "same-content-b".to_string(),
                embedding: vec![0.9, 0.1],
                metadata: json!({"tenant": "acme", "content_hash": "hash-1"}),
                source: None,
            },
            ChunkInput {
                id: "c3".to_string(),
                doc_id: "d1".to_string(),
                content: "different-content".to_string(),
                embedding: vec![0.0, 1.0],
                metadata: json!({"tenant": "acme", "content_hash": "hash-2"}),
                source: None,
            },
        ])?;

        let report = db.compact(CompactionOptions {
            dedupe_by_content_hash: true,
            prune_orphan_documents: false,
            wal_checkpoint_truncate: false,
            analyze: false,
            vacuum: false,
        })?;
        assert_eq!(report.before_chunks, 3);
        assert_eq!(report.after_chunks, 2);
        assert_eq!(report.removed_chunks, 1);
        assert_eq!(report.deduplicated_chunks, 1);
        assert!(report.vector_index_rebuilt);
        assert_eq!(
            db.vector_index_stats()
                .map(|stats| stats.entries)
                .unwrap_or(0),
            2
        );
        Ok(())
    }

    #[test]
    fn compaction_prunes_orphan_documents() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "active".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "acme", "content_hash": "hash-1"}),
            source: None,
        })?;
        db.conn.execute(
            "INSERT INTO documents (id, source, metadata) VALUES (?1, ?2, '{}')",
            params!["orphan-doc", Option::<String>::None],
        )?;

        let report = db.compact(CompactionOptions {
            dedupe_by_content_hash: false,
            prune_orphan_documents: true,
            wal_checkpoint_truncate: false,
            analyze: false,
            vacuum: false,
        })?;
        assert_eq!(report.before_documents, 2);
        assert_eq!(report.after_documents, 1);
        assert_eq!(report.orphan_documents_removed, 1);
        Ok(())
    }

    #[test]
    fn vector_search_falls_back_to_bruteforce_when_index_absent() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(
            RuntimeConfig::default().with_vector_index_mode(VectorIndexMode::Disabled),
        )?;
        db.ingest_chunk(&ChunkInput {
            id: "best".to_string(),
            doc_id: "d1".to_string(),
            content: "best".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;
        db.ingest_chunk(&ChunkInput {
            id: "recent-noise".to_string(),
            doc_id: "d2".to_string(),
            content: "noise".to_string(),
            embedding: vec![0.0, 1.0],
            metadata: json!({}),
            source: None,
        })?;

        let results = db.search(
            SearchRequest::builder()
                .query_embedding(vec![1.0, 0.0])
                .candidate_limit(1)
                .top_k(1)
                .build()?,
        )?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, "best");
        Ok(())
    }

    #[test]
    fn vector_search_falls_back_to_bruteforce_on_index_dimension_mismatch() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(
            RuntimeConfig::default().with_vector_index_mode(VectorIndexMode::BruteForce),
        )?;

        db.ingest_chunk(&ChunkInput {
            id: "indexed-3d".to_string(),
            doc_id: "d-indexed".to_string(),
            content: "indexed".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;

        db.conn.execute(
            "INSERT INTO documents (id, source, metadata) VALUES (?1, ?2, '{}')
             ON CONFLICT(id) DO UPDATE SET source = excluded.source",
            params!["d-raw-1", Option::<String>::None],
        )?;
        db.conn.execute(
            "INSERT INTO chunks (id, doc_id, content, metadata, embedding, embedding_dim)
             VALUES (?1, ?2, ?3, '{}', ?4, ?5)",
            params![
                "target-2d",
                "d-raw-1",
                "target",
                encode_embedding(&[1.0, 0.0]),
                2
            ],
        )?;

        db.conn.execute(
            "INSERT INTO documents (id, source, metadata) VALUES (?1, ?2, '{}')
             ON CONFLICT(id) DO UPDATE SET source = excluded.source",
            params!["d-raw-2", Option::<String>::None],
        )?;
        db.conn.execute(
            "INSERT INTO chunks (id, doc_id, content, metadata, embedding, embedding_dim)
             VALUES (?1, ?2, ?3, '{}', ?4, ?5)",
            params![
                "recent-noise-2d",
                "d-raw-2",
                "noise",
                encode_embedding(&[0.0, 1.0]),
                2
            ],
        )?;

        let results = db.search(
            SearchRequest::builder()
                .query_embedding(vec![1.0, 0.0])
                .candidate_limit(1)
                .top_k(1)
                .build()?,
        )?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, "target-2d");
        Ok(())
    }

    #[test]
    fn ingest_chunks_is_atomic_on_dimension_validation_error() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(
            RuntimeConfig::default().with_vector_index_mode(VectorIndexMode::BruteForce),
        )?;
        let err = db
            .ingest_chunks(&[
                ChunkInput {
                    id: "ok".to_string(),
                    doc_id: "d1".to_string(),
                    content: "first".to_string(),
                    embedding: vec![1.0, 0.0, 0.0],
                    metadata: json!({}),
                    source: None,
                },
                ChunkInput {
                    id: "bad".to_string(),
                    doc_id: "d2".to_string(),
                    content: "second".to_string(),
                    embedding: vec![1.0, 0.0],
                    metadata: json!({}),
                    source: None,
                },
            ])
            .expect_err("mixed dimensions in one batch should fail");
        assert!(matches!(
            err,
            SqlRiteError::EmbeddingDimensionMismatch { .. }
        ));
        assert_eq!(db.chunk_count()?, 0);
        Ok(())
    }

    #[test]
    fn text_search_uses_fts_candidates_not_recent_window() -> Result<()> {
        let db = SqlRite::open_in_memory()?;
        if !db.fts_enabled {
            return Ok(());
        }

        db.ingest_chunk(&ChunkInput {
            id: "target".to_string(),
            doc_id: "doc-target".to_string(),
            content: "ultrauniqueterm retrieval anchor".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "acme"}),
            source: None,
        })?;

        for idx in 0..20 {
            db.ingest_chunk(&ChunkInput {
                id: format!("noise-{idx}"),
                doc_id: format!("doc-noise-{idx}"),
                content: format!("background chunk {idx} with no lexical match"),
                embedding: vec![0.0, 1.0],
                metadata: json!({"tenant": "acme"}),
                source: None,
            })?;
        }

        let results = db.search(
            SearchRequest::builder()
                .query_text("ultrauniqueterm")
                .top_k(1)
                .candidate_limit(5)
                .build()?,
        )?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, "target");
        Ok(())
    }

    #[test]
    fn text_search_applies_filters_during_fts_candidate_selection() -> Result<()> {
        let db = SqlRite::open_in_memory()?;
        if !db.fts_enabled {
            return Ok(());
        }

        db.ingest_chunk(&ChunkInput {
            id: "beta-hit".to_string(),
            doc_id: "doc-beta".to_string(),
            content: "tenantscopedtoken appears here".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "beta"}),
            source: None,
        })?;
        db.ingest_chunk(&ChunkInput {
            id: "acme-hit".to_string(),
            doc_id: "doc-acme".to_string(),
            content: "tenantscopedtoken appears here".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "acme"}),
            source: None,
        })?;

        let results = db.search(
            SearchRequest::builder()
                .query_text("tenantscopedtoken")
                .metadata_filter("tenant", "beta")
                .top_k(1)
                .candidate_limit(1)
                .build()?,
        )?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, "beta-hit");
        Ok(())
    }

    #[test]
    fn rrf_changes_hybrid_ordering() -> Result<()> {
        let db = SqlRite::open_in_memory()?;
        db.ingest_chunks(&[
            ChunkInput {
                id: "z".to_string(),
                doc_id: "d1".to_string(),
                content: "noise".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({}),
                source: None,
            },
            ChunkInput {
                id: "b".to_string(),
                doc_id: "d2".to_string(),
                content: "target".to_string(),
                embedding: vec![0.97, 0.03],
                metadata: json!({}),
                source: None,
            },
            ChunkInput {
                id: "c".to_string(),
                doc_id: "d3".to_string(),
                content: "target".to_string(),
                embedding: vec![0.94, 0.06],
                metadata: json!({}),
                source: None,
            },
        ])?;

        let weighted = db.search(
            SearchRequest::builder()
                .query_text("target")
                .query_embedding(vec![1.0, 0.0])
                .alpha(1.0)
                .top_k(3)
                .fusion_strategy(FusionStrategy::Weighted)
                .build()?,
        )?;
        assert_eq!(weighted[0].chunk_id, "z");

        let rrf = db.search(
            SearchRequest::builder()
                .query_text("target")
                .query_embedding(vec![1.0, 0.0])
                .alpha(1.0)
                .top_k(3)
                .reciprocal_rank_fusion(1.0)
                .build()?,
        )?;
        assert_eq!(rrf[0].chunk_id, "b");
        Ok(())
    }
}
