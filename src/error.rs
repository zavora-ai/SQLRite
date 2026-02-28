use thiserror::Error;

pub type Result<T> = std::result::Result<T, SqlRiteError>;

#[derive(Debug, Error)]
pub enum SqlRiteError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("embedding cannot be empty")]
    EmptyEmbedding,
    #[error("invalid embedding bytes: expected {expected_bytes} bytes, found {found_bytes}")]
    InvalidEmbeddingBytes {
        expected_bytes: usize,
        found_bytes: usize,
    },
    #[error("invalid metadata filter key `{0}`; only letters, numbers, and underscore are allowed")]
    InvalidFilterKey(String),
    #[error("at least one of query_text or query_embedding is required")]
    MissingQuery,
    #[error("invalid evaluation dataset: {0}")]
    InvalidEvaluationDataset(String),
    #[error("invalid benchmark config: {0}")]
    InvalidBenchmarkConfig(String),
    #[error("invalid compaction config: {0}")]
    InvalidCompactionConfig(String),
    #[error("embedding dimension mismatch: expected {expected}, found {found}")]
    EmbeddingDimensionMismatch { expected: usize, found: usize },
    #[error("top_k must be at least 1")]
    InvalidTopK,
    #[error("candidate_limit must be at least 1")]
    InvalidCandidateLimit,
    #[error("candidate_limit must be greater than or equal to top_k")]
    CandidateLimitTooSmall,
    #[error("alpha must be between 0.0 and 1.0")]
    InvalidAlpha,
    #[error("rrf rank_constant must be greater than 0.0")]
    InvalidRrfRankConstant,
    #[error("invalid tenant id")]
    InvalidTenantId,
    #[error("authorization denied: {0}")]
    AuthorizationDenied(String),
    #[error("embedding provider error: {0}")]
    EmbeddingProvider(String),
    #[error("embedding batch had {failed} failed item(s) after retries")]
    EmbeddingBatchPartialFailure { failed: usize },
    #[error("invalid ingestion checkpoint: {0}")]
    InvalidIngestionCheckpoint(String),
    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),
}
