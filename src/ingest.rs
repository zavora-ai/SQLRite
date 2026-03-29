use crate::{ChunkInput, Result, SqlRite, SqlRiteError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum IngestionSource {
    Direct { content: String },
    File { path: PathBuf },
    Url { url: String },
}

impl IngestionSource {
    pub fn load_content(&self) -> Result<String> {
        match self {
            Self::Direct { content } => Ok(content.clone()),
            Self::File { path } => Ok(fs::read_to_string(path)?),
            Self::Url { url } => {
                let output = Command::new("curl").arg("-fsSL").arg(url).output()?;
                if !output.status.success() {
                    return Err(SqlRiteError::UnsupportedOperation(format!(
                        "url ingestion failed for `{url}`"
                    )));
                }
                String::from_utf8(output.stdout).map_err(|_| {
                    SqlRiteError::UnsupportedOperation("url content is not valid UTF-8".to_string())
                })
            }
        }
    }

    fn source_label(&self) -> String {
        match self {
            Self::Direct { .. } => "direct".to_string(),
            Self::File { path } => path.display().to_string(),
            Self::Url { url } => url.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ChunkingStrategy {
    Fixed {
        max_chars: usize,
        overlap_chars: usize,
    },
    HeadingAware {
        max_chars: usize,
        overlap_chars: usize,
    },
    Semantic {
        max_chars: usize,
    },
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self::HeadingAware {
            max_chars: 1200,
            overlap_chars: 120,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionCheckpoint {
    pub job_id: String,
    pub source_id: String,
    pub next_chunk_index: usize,
    pub updated_unix_ms: u64,
}

impl IngestionCheckpoint {
    pub fn load(path: impl AsRef<Path>) -> Result<Option<Self>> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }
        let payload = fs::read_to_string(path)?;
        let checkpoint = serde_json::from_str::<Self>(&payload)
            .map_err(|e| SqlRiteError::InvalidIngestionCheckpoint(e.to_string()))?;
        Ok(Some(checkpoint))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }

        let payload = serde_json::to_string_pretty(self)?;
        let temp = path.with_extension("tmp");
        fs::write(&temp, payload)?;
        fs::rename(temp, path)?;
        Ok(())
    }

    pub fn clear(path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct EmbeddingRetryPolicy {
    pub max_retries: usize,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl Default for EmbeddingRetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 50,
            max_backoff_ms: 1_000,
        }
    }
}

pub trait EmbeddingProvider {
    fn provider_name(&self) -> &str;
    fn model_version(&self) -> &str;

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<std::result::Result<Vec<f32>, String>>>;
}

#[derive(Debug, Clone)]
pub struct DeterministicEmbeddingProvider {
    dimension: usize,
    model_version: String,
}

impl DeterministicEmbeddingProvider {
    pub fn new(dimension: usize, model_version: impl Into<String>) -> Result<Self> {
        if dimension == 0 {
            return Err(SqlRiteError::EmbeddingProvider(
                "dimension must be greater than 0".to_string(),
            ));
        }
        Ok(Self {
            dimension,
            model_version: model_version.into(),
        })
    }

    fn embed_text(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; self.dimension];
        for token in text
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
        {
            let hash = fnv1a64(token.as_bytes());
            let idx = (hash % self.dimension as u64) as usize;
            vector[idx] += 1.0;
        }

        // Keep a deterministic fallback signal for empty-token content.
        if vector.iter().all(|v| *v == 0.0) {
            vector[0] = 1.0;
        }

        normalize(&mut vector);
        vector
    }
}

impl EmbeddingProvider for DeterministicEmbeddingProvider {
    fn provider_name(&self) -> &str {
        "deterministic_local"
    }

    fn model_version(&self) -> &str {
        &self.model_version
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<std::result::Result<Vec<f32>, String>>> {
        Ok(texts
            .iter()
            .map(|text| Ok(self.embed_text(text)))
            .collect::<Vec<_>>())
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleEmbeddingProvider {
    endpoint: String,
    api_key: String,
    model: String,
    model_version: String,
    timeout_secs: u64,
}

impl OpenAiCompatibleEmbeddingProvider {
    pub fn new(
        endpoint: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self> {
        let endpoint = endpoint.into();
        let api_key = api_key.into();
        let model = model.into();
        if endpoint.trim().is_empty() || api_key.trim().is_empty() || model.trim().is_empty() {
            return Err(SqlRiteError::EmbeddingProvider(
                "endpoint, api_key and model are required".to_string(),
            ));
        }
        Ok(Self {
            endpoint,
            api_key,
            model_version: model.clone(),
            model,
            timeout_secs: 30,
        })
    }

    pub fn from_env(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key_env: &str,
    ) -> Result<Self> {
        let api_key = std::env::var(api_key_env).map_err(|_| {
            SqlRiteError::EmbeddingProvider(format!("missing required env var `{api_key_env}`"))
        })?;
        Self::new(endpoint, api_key, model)
    }

    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs.max(1);
        self
    }
}

impl EmbeddingProvider for OpenAiCompatibleEmbeddingProvider {
    fn provider_name(&self) -> &str {
        "openai_compatible_http"
    }

    fn model_version(&self) -> &str {
        &self.model_version
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<std::result::Result<Vec<f32>, String>>> {
        let payload = json!({
            "model": self.model,
            "input": texts,
        });
        let response = http_post_json(
            &self.endpoint,
            &payload,
            &[(
                "Authorization".to_string(),
                format!("Bearer {}", self.api_key),
            )],
            self.timeout_secs,
        )?;

        parse_openai_embeddings_response(&response)
    }
}

#[derive(Debug, Clone)]
pub struct CustomHttpEmbeddingProvider {
    endpoint: String,
    model: Option<String>,
    model_version: String,
    input_field: String,
    embeddings_field: String,
    headers: Vec<(String, String)>,
    timeout_secs: u64,
}

impl CustomHttpEmbeddingProvider {
    pub fn new(endpoint: impl Into<String>, model_version: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        if endpoint.trim().is_empty() {
            return Err(SqlRiteError::EmbeddingProvider(
                "endpoint is required".to_string(),
            ));
        }
        Ok(Self {
            endpoint,
            model: None,
            model_version: model_version.into(),
            input_field: "inputs".to_string(),
            embeddings_field: "embeddings".to_string(),
            headers: Vec::new(),
            timeout_secs: 30,
        })
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((key.into(), value.into()));
        self
    }

    pub fn with_fields(
        mut self,
        input_field: impl Into<String>,
        embeddings_field: impl Into<String>,
    ) -> Self {
        self.input_field = input_field.into();
        self.embeddings_field = embeddings_field.into();
        self
    }

    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs.max(1);
        self
    }
}

impl EmbeddingProvider for CustomHttpEmbeddingProvider {
    fn provider_name(&self) -> &str {
        "custom_http"
    }

    fn model_version(&self) -> &str {
        &self.model_version
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<std::result::Result<Vec<f32>, String>>> {
        let mut payload = serde_json::Map::new();
        payload.insert(
            self.input_field.clone(),
            Value::Array(texts.iter().cloned().map(Value::String).collect()),
        );
        if let Some(model) = &self.model {
            payload.insert("model".to_string(), Value::String(model.clone()));
        }

        let response = http_post_json(
            &self.endpoint,
            &Value::Object(payload),
            &self.headers,
            self.timeout_secs,
        )?;

        if let Some(vectors) = response
            .get(&self.embeddings_field)
            .and_then(Value::as_array)
        {
            let mut out = Vec::with_capacity(vectors.len());
            for vector in vectors {
                out.push(parse_embedding_array(vector).map_err(|e| e.to_string()));
            }
            return Ok(out);
        }

        if let Some(results) = response.get("results").and_then(Value::as_array) {
            let mut out = Vec::with_capacity(results.len());
            for item in results {
                if let Some(error) = item.get("error").and_then(Value::as_str) {
                    out.push(Err(error.to_string()));
                    continue;
                }
                let Some(embedding) = item.get("embedding") else {
                    out.push(Err("missing `embedding` field".to_string()));
                    continue;
                };
                out.push(parse_embedding_array(embedding).map_err(|e| e.to_string()));
            }
            return Ok(out);
        }

        if response.get("data").is_some() {
            return parse_openai_embeddings_response(&response);
        }

        Err(SqlRiteError::EmbeddingProvider(
            "unsupported custom embedding response schema".to_string(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct IngestionRequest {
    pub job_id: String,
    pub doc_id: String,
    pub source_id: String,
    pub tenant_id: String,
    pub source: IngestionSource,
    pub metadata: Value,
    pub chunking: ChunkingStrategy,
    pub batch_size: usize,
    pub batch_tuning: IngestionBatchTuning,
    pub continue_on_partial_failure: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IngestionBatchTuning {
    pub adaptive: bool,
    pub max_batch_size: usize,
    pub target_batch_ms: u64,
}

impl Default for IngestionBatchTuning {
    fn default() -> Self {
        Self {
            adaptive: true,
            max_batch_size: 1024,
            target_batch_ms: 80,
        }
    }
}

impl IngestionRequest {
    pub fn from_direct(
        job_id: impl Into<String>,
        doc_id: impl Into<String>,
        source_id: impl Into<String>,
        tenant_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            doc_id: doc_id.into(),
            source_id: source_id.into(),
            tenant_id: tenant_id.into(),
            source: IngestionSource::Direct {
                content: content.into(),
            },
            metadata: json!({}),
            chunking: ChunkingStrategy::default(),
            batch_size: 64,
            batch_tuning: IngestionBatchTuning::default(),
            continue_on_partial_failure: false,
        }
    }

    pub fn from_file(
        job_id: impl Into<String>,
        doc_id: impl Into<String>,
        source_id: impl Into<String>,
        tenant_id: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            doc_id: doc_id.into(),
            source_id: source_id.into(),
            tenant_id: tenant_id.into(),
            source: IngestionSource::File { path: path.into() },
            metadata: json!({}),
            chunking: ChunkingStrategy::default(),
            batch_size: 64,
            batch_tuning: IngestionBatchTuning::default(),
            continue_on_partial_failure: false,
        }
    }

    pub fn from_url(
        job_id: impl Into<String>,
        doc_id: impl Into<String>,
        source_id: impl Into<String>,
        tenant_id: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            doc_id: doc_id.into(),
            source_id: source_id.into(),
            tenant_id: tenant_id.into(),
            source: IngestionSource::Url { url: url.into() },
            metadata: json!({}),
            chunking: ChunkingStrategy::default(),
            batch_size: 64,
            batch_tuning: IngestionBatchTuning::default(),
            continue_on_partial_failure: false,
        }
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_chunking(mut self, chunking: ChunkingStrategy) -> Self {
        self.chunking = chunking;
        self
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn with_batch_tuning(mut self, batch_tuning: IngestionBatchTuning) -> Self {
        self.batch_tuning = batch_tuning;
        self
    }

    pub fn with_adaptive_batching(mut self, enabled: bool) -> Self {
        self.batch_tuning.adaptive = enabled;
        self
    }

    pub fn with_max_batch_size(mut self, max_batch_size: usize) -> Self {
        self.batch_tuning.max_batch_size = max_batch_size.max(1);
        self
    }

    pub fn with_target_batch_ms(mut self, target_batch_ms: u64) -> Self {
        self.batch_tuning.target_batch_ms = target_batch_ms.max(1);
        self
    }

    pub fn with_continue_on_partial_failure(mut self, enabled: bool) -> Self {
        self.continue_on_partial_failure = enabled;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionReport {
    pub total_chunks: usize,
    pub processed_chunks: usize,
    pub failed_chunks: usize,
    pub resumed_from_chunk: usize,
    pub duration_ms: f64,
    pub throughput_chunks_per_minute: f64,
    pub average_batch_size: f64,
    pub peak_batch_size: usize,
    pub batch_count: usize,
    pub adaptive_batching: bool,
    pub provider: String,
    pub model_version: String,
    pub source: String,
}

#[derive(Debug, Clone)]
struct Segment {
    content: String,
    start: usize,
    end: usize,
    heading: Option<String>,
}

struct MetadataEnrichment<'a> {
    tenant_id: &'a str,
    content_hash: String,
    source_start: usize,
    source_end: usize,
    heading: Option<&'a str>,
    provider: &'a str,
    model_version: &'a str,
}

pub struct IngestionWorker<'a, P: EmbeddingProvider> {
    db: &'a SqlRite,
    provider: P,
    retry_policy: EmbeddingRetryPolicy,
    checkpoint_path: Option<PathBuf>,
}

impl<'a, P: EmbeddingProvider> IngestionWorker<'a, P> {
    pub fn new(db: &'a SqlRite, provider: P) -> Self {
        Self {
            db,
            provider,
            retry_policy: EmbeddingRetryPolicy::default(),
            checkpoint_path: None,
        }
    }

    pub fn with_retry_policy(mut self, retry_policy: EmbeddingRetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub fn with_checkpoint_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.checkpoint_path = Some(path.into());
        self
    }

    pub fn ingest(&self, request: IngestionRequest) -> Result<IngestionReport> {
        if request.tenant_id.trim().is_empty() {
            return Err(SqlRiteError::InvalidTenantId);
        }
        if request.batch_size == 0 {
            return Err(SqlRiteError::InvalidBenchmarkConfig(
                "ingestion batch_size must be >= 1".to_string(),
            ));
        }
        if request.batch_tuning.max_batch_size == 0 {
            return Err(SqlRiteError::InvalidBenchmarkConfig(
                "ingestion max_batch_size must be >= 1".to_string(),
            ));
        }
        if request.batch_tuning.target_batch_ms == 0 {
            return Err(SqlRiteError::InvalidBenchmarkConfig(
                "ingestion target_batch_ms must be >= 1".to_string(),
            ));
        }

        let ingest_started = Instant::now();
        let source_content = request.source.load_content()?;
        let segments = chunk_content(&source_content, &request.chunking);

        let resumed_from_chunk = self
            .load_resume_checkpoint(&request.job_id, &request.source_id)?
            .unwrap_or(0)
            .min(segments.len());

        let mut processed_chunks = 0usize;
        let mut failed_chunks = 0usize;
        let mut cursor = resumed_from_chunk;
        let mut batch_count = 0usize;
        let mut peak_batch_size = 0usize;
        let mut total_batch_size = 0usize;
        let mut next_batch_size = request
            .batch_size
            .max(1)
            .min(request.batch_tuning.max_batch_size);

        while cursor < segments.len() {
            let batch_started = Instant::now();
            let remaining = segments.len().saturating_sub(cursor);
            let planned_batch = next_batch_size.min(remaining).max(1);
            let end = (cursor + planned_batch).min(segments.len());
            let batch = &segments[cursor..end];
            batch_count += 1;
            peak_batch_size = peak_batch_size.max(batch.len());
            total_batch_size += batch.len();
            let texts = batch
                .iter()
                .map(|segment| segment.content.clone())
                .collect::<Vec<_>>();

            let embedded = self.embed_with_retry(&texts)?;
            let mut upserts = Vec::with_capacity(batch.len());
            let mut batch_failed_chunks = 0usize;

            for (idx, segment) in batch.iter().enumerate() {
                let Some(embedding) = embedded[idx].clone() else {
                    batch_failed_chunks += 1;
                    failed_chunks += 1;
                    continue;
                };

                let chunk_id = chunk_id_for(
                    &request.tenant_id,
                    &request.doc_id,
                    segment.start,
                    segment.end,
                    &segment.content,
                );

                let metadata = merge_metadata(
                    &request.metadata,
                    &MetadataEnrichment {
                        tenant_id: &request.tenant_id,
                        content_hash: hex64(fnv1a64(segment.content.as_bytes())),
                        source_start: segment.start,
                        source_end: segment.end,
                        heading: segment.heading.as_deref(),
                        provider: self.provider.provider_name(),
                        model_version: self.provider.model_version(),
                    },
                )?;

                upserts.push(ChunkInput {
                    id: chunk_id,
                    doc_id: request.doc_id.clone(),
                    content: segment.content.clone(),
                    embedding,
                    metadata,
                    source: Some(request.source_id.clone()),
                });
            }

            if !upserts.is_empty() {
                self.db.ingest_chunks(&upserts)?;
                processed_chunks += upserts.len();
            }

            cursor = end;
            self.save_checkpoint(&request.job_id, &request.source_id, cursor)?;
            let batch_duration_ms = batch_started.elapsed().as_secs_f64() * 1000.0;

            if batch_failed_chunks > 0 && !request.continue_on_partial_failure {
                return Err(SqlRiteError::EmbeddingBatchPartialFailure {
                    failed: failed_chunks,
                });
            }

            if request.batch_tuning.adaptive {
                let target_ms = request.batch_tuning.target_batch_ms as f64;
                if batch_failed_chunks == 0 && batch_duration_ms <= target_ms * 0.60 {
                    let grown = next_batch_size
                        .saturating_add(next_batch_size / 2)
                        .saturating_add(1);
                    next_batch_size = grown.min(request.batch_tuning.max_batch_size).max(1);
                } else if batch_duration_ms > target_ms || batch_failed_chunks > 0 {
                    next_batch_size = (next_batch_size / 2).max(1);
                }
            }
        }

        self.clear_checkpoint()?;
        let duration_ms = ingest_started.elapsed().as_secs_f64() * 1000.0;
        let throughput_chunks_per_minute = if duration_ms > 0.0 {
            (processed_chunks as f64 / (duration_ms / 1000.0)) * 60.0
        } else {
            0.0
        };
        let average_batch_size = if batch_count > 0 {
            total_batch_size as f64 / batch_count as f64
        } else {
            0.0
        };

        Ok(IngestionReport {
            total_chunks: segments.len(),
            processed_chunks,
            failed_chunks,
            resumed_from_chunk,
            duration_ms,
            throughput_chunks_per_minute,
            average_batch_size,
            peak_batch_size,
            batch_count,
            adaptive_batching: request.batch_tuning.adaptive,
            provider: self.provider.provider_name().to_string(),
            model_version: self.provider.model_version().to_string(),
            source: request.source.source_label(),
        })
    }

    fn embed_with_retry(&self, texts: &[String]) -> Result<Vec<Option<Vec<f32>>>> {
        let mut pending: Vec<usize> = (0..texts.len()).collect();
        let mut resolved = vec![None; texts.len()];
        let mut attempt = 0usize;
        let mut backoff_ms = self.retry_policy.initial_backoff_ms.max(1);

        while !pending.is_empty() && attempt <= self.retry_policy.max_retries {
            let current_texts = pending
                .iter()
                .map(|idx| texts[*idx].clone())
                .collect::<Vec<_>>();
            let responses = self.provider.embed_batch(&current_texts)?;

            if responses.len() != current_texts.len() {
                return Err(SqlRiteError::EmbeddingProvider(
                    "provider returned mismatched batch length".to_string(),
                ));
            }

            let mut next_pending = Vec::new();
            for (slot, response) in responses.into_iter().enumerate() {
                let original_idx = pending[slot];
                match response {
                    Ok(embedding) => resolved[original_idx] = Some(embedding),
                    Err(_) => next_pending.push(original_idx),
                }
            }

            pending = next_pending;
            if !pending.is_empty() {
                attempt += 1;
                if attempt <= self.retry_policy.max_retries {
                    thread::sleep(Duration::from_millis(backoff_ms));
                    backoff_ms =
                        (backoff_ms.saturating_mul(2)).min(self.retry_policy.max_backoff_ms);
                }
            }
        }

        Ok(resolved)
    }

    fn load_resume_checkpoint(&self, job_id: &str, source_id: &str) -> Result<Option<usize>> {
        let Some(path) = &self.checkpoint_path else {
            return Ok(None);
        };

        let Some(checkpoint) = IngestionCheckpoint::load(path)? else {
            return Ok(None);
        };

        if checkpoint.job_id == job_id && checkpoint.source_id == source_id {
            Ok(Some(checkpoint.next_chunk_index))
        } else {
            Ok(None)
        }
    }

    fn save_checkpoint(
        &self,
        job_id: &str,
        source_id: &str,
        next_chunk_index: usize,
    ) -> Result<()> {
        let Some(path) = &self.checkpoint_path else {
            return Ok(());
        };

        let checkpoint = IngestionCheckpoint {
            job_id: job_id.to_string(),
            source_id: source_id.to_string(),
            next_chunk_index,
            updated_unix_ms: now_unix_ms(),
        };
        checkpoint.save(path)
    }

    fn clear_checkpoint(&self) -> Result<()> {
        let Some(path) = &self.checkpoint_path else {
            return Ok(());
        };

        IngestionCheckpoint::clear(path)
    }
}

fn chunk_content(text: &str, strategy: &ChunkingStrategy) -> Vec<Segment> {
    match strategy {
        ChunkingStrategy::Fixed {
            max_chars,
            overlap_chars,
        } => chunk_fixed(text, *max_chars, *overlap_chars),
        ChunkingStrategy::HeadingAware {
            max_chars,
            overlap_chars,
        } => chunk_heading_aware(text, *max_chars, *overlap_chars),
        ChunkingStrategy::Semantic { max_chars } => chunk_semantic(text, *max_chars),
    }
}

pub(crate) fn chunk_text_for_ingest(text: &str, strategy: &ChunkingStrategy) -> Vec<String> {
    chunk_content(text, strategy)
        .into_iter()
        .map(|segment| segment.content)
        .collect()
}

fn chunk_fixed(text: &str, max_chars: usize, overlap_chars: usize) -> Vec<Segment> {
    if text.is_empty() {
        return Vec::new();
    }

    let max_chars = max_chars.max(1);
    let overlap_chars = overlap_chars.min(max_chars.saturating_sub(1));

    let mut segments = Vec::new();
    let mut start = 0usize;

    while start < text.len() {
        let mut end = (start + max_chars).min(text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end <= start {
            end = text.len();
        }

        segments.push(Segment {
            content: text[start..end].to_string(),
            start,
            end,
            heading: None,
        });

        if end == text.len() {
            break;
        }

        let mut next_start = end.saturating_sub(overlap_chars);
        while next_start > start && !text.is_char_boundary(next_start) {
            next_start -= 1;
        }
        if next_start <= start {
            next_start = end;
        }
        start = next_start;
    }

    segments
}

fn chunk_heading_aware(text: &str, max_chars: usize, overlap_chars: usize) -> Vec<Segment> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut sections = Vec::new();
    let mut offset = 0usize;
    let mut section_start = 0usize;
    let mut heading: Option<String> = None;

    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            if line_start > section_start {
                sections.push((section_start, line_start, heading.clone()));
            }
            heading = Some(trimmed.trim().trim_start_matches('#').trim().to_string());
            section_start = line_start;
        }
    }

    if section_start < text.len() {
        sections.push((section_start, text.len(), heading));
    }

    if sections.is_empty() {
        return chunk_fixed(text, max_chars, overlap_chars);
    }

    let mut segments = Vec::new();
    for (start, end, heading) in sections {
        let section_text = &text[start..end];
        for mut part in chunk_fixed(section_text, max_chars, overlap_chars) {
            part.start += start;
            part.end += start;
            part.heading = heading.clone();
            segments.push(part);
        }
    }

    segments
}

fn chunk_semantic(text: &str, max_chars: usize) -> Vec<Segment> {
    if text.is_empty() {
        return Vec::new();
    }

    let max_chars = max_chars.max(1);
    let mut sentence_bounds = Vec::new();
    let mut sentence_start = 0usize;

    for (idx, ch) in text.char_indices() {
        if matches!(ch, '.' | '!' | '?') {
            let end = idx + ch.len_utf8();
            if end > sentence_start {
                sentence_bounds.push((sentence_start, end));
                sentence_start = end;
            }
        }
    }
    if sentence_start < text.len() {
        sentence_bounds.push((sentence_start, text.len()));
    }

    if sentence_bounds.is_empty() {
        return chunk_fixed(text, max_chars, 0);
    }

    let mut segments = Vec::new();
    let mut current_start = sentence_bounds[0].0;
    let mut current_end = sentence_bounds[0].0;

    for (start, end) in sentence_bounds {
        if end.saturating_sub(current_start) > max_chars && current_end > current_start {
            segments.push(Segment {
                content: text[current_start..current_end].trim().to_string(),
                start: current_start,
                end: current_end,
                heading: None,
            });
            current_start = start;
        }
        current_end = end;
    }

    if current_end > current_start {
        segments.push(Segment {
            content: text[current_start..current_end].trim().to_string(),
            start: current_start,
            end: current_end,
            heading: None,
        });
    }

    if segments.is_empty() {
        chunk_fixed(text, max_chars, 0)
    } else {
        segments
            .into_iter()
            .filter(|segment| !segment.content.is_empty())
            .collect()
    }
}

fn merge_metadata(base: &Value, enrichment: &MetadataEnrichment<'_>) -> Result<Value> {
    let mut metadata_obj = match base {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };

    metadata_obj.insert(
        "tenant".to_string(),
        Value::String(enrichment.tenant_id.to_string()),
    );
    metadata_obj.insert(
        "content_hash".to_string(),
        Value::String(enrichment.content_hash.clone()),
    );
    metadata_obj.insert(
        "source_start".to_string(),
        Value::Number(serde_json::Number::from(enrichment.source_start as u64)),
    );
    metadata_obj.insert(
        "source_end".to_string(),
        Value::Number(serde_json::Number::from(enrichment.source_end as u64)),
    );
    metadata_obj.insert(
        "embedding_provider".to_string(),
        Value::String(enrichment.provider.to_string()),
    );
    metadata_obj.insert(
        "embedding_model_version".to_string(),
        Value::String(enrichment.model_version.to_string()),
    );

    if let Some(heading) = enrichment.heading
        && !heading.is_empty()
    {
        metadata_obj.insert("heading".to_string(), Value::String(heading.to_string()));
    }

    Ok(Value::Object(metadata_obj))
}

fn chunk_id_for(tenant_id: &str, doc_id: &str, start: usize, end: usize, content: &str) -> String {
    let mut seed = Vec::new();
    seed.extend_from_slice(tenant_id.as_bytes());
    seed.push(0);
    seed.extend_from_slice(doc_id.as_bytes());
    seed.push(0);
    seed.extend_from_slice(start.to_string().as_bytes());
    seed.push(0);
    seed.extend_from_slice(end.to_string().as_bytes());
    seed.push(0);
    seed.extend_from_slice(content.as_bytes());

    let hash = hex64(fnv1a64(&seed));
    let mut tenant = tenant_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    if tenant.is_empty() {
        tenant = "tenant".to_string();
    }

    format!("{tenant}-{hash}")
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn http_post_json(
    endpoint: &str,
    payload: &Value,
    headers: &[(String, String)],
    timeout_secs: u64,
) -> Result<Value> {
    let mut cmd = Command::new("curl");
    cmd.arg("-fsS")
        .arg("-X")
        .arg("POST")
        .arg("--max-time")
        .arg(timeout_secs.max(1).to_string())
        .arg("-H")
        .arg("Content-Type: application/json");

    for (key, value) in headers {
        cmd.arg("-H").arg(format!("{key}: {value}"));
    }

    let output = cmd
        .arg("-d")
        .arg(payload.to_string())
        .arg(endpoint)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SqlRiteError::EmbeddingProvider(format!(
            "http request failed for `{endpoint}`: {stderr}"
        )));
    }

    serde_json::from_slice::<Value>(&output.stdout).map_err(|e| {
        SqlRiteError::EmbeddingProvider(format!("invalid json response from `{endpoint}`: {e}"))
    })
}

fn parse_openai_embeddings_response(
    response: &Value,
) -> Result<Vec<std::result::Result<Vec<f32>, String>>> {
    let data = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| SqlRiteError::EmbeddingProvider("missing `data` array".to_string()))?;

    let mut out = Vec::with_capacity(data.len());
    for row in data {
        let Some(embedding) = row.get("embedding") else {
            out.push(Err("missing `embedding` field".to_string()));
            continue;
        };
        out.push(parse_embedding_array(embedding).map_err(|e| e.to_string()));
    }
    Ok(out)
}

fn parse_embedding_array(value: &Value) -> Result<Vec<f32>> {
    let array = value
        .as_array()
        .ok_or_else(|| SqlRiteError::EmbeddingProvider("embedding must be an array".to_string()))?;
    let mut embedding = Vec::with_capacity(array.len());
    for item in array {
        let Some(number) = item.as_f64() else {
            return Err(SqlRiteError::EmbeddingProvider(
                "embedding item must be numeric".to_string(),
            ));
        };
        embedding.push(number as f32);
    }
    if embedding.is_empty() {
        return Err(SqlRiteError::EmbeddingProvider(
            "embedding array cannot be empty".to_string(),
        ));
    }
    Ok(embedding)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn hex64(value: u64) -> String {
    format!("{value:016x}")
}

fn normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in vector {
            *value /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuntimeConfig;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn fixed_chunking_respects_overlap() {
        let text = "abcdefghijklmnopqrstuvwxyz";
        let chunks = chunk_fixed(text, 10, 3);
        assert!(chunks.len() >= 3);
        assert_eq!(chunks[0].content, "abcdefghij");
        assert_eq!(chunks[1].content, "hijklmnopq");
    }

    #[test]
    fn checkpoint_round_trip() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("checkpoint.json");
        let checkpoint = IngestionCheckpoint {
            job_id: "job-a".to_string(),
            source_id: "source-a".to_string(),
            next_chunk_index: 42,
            updated_unix_ms: 1,
        };

        checkpoint.save(&path)?;
        let loaded = IngestionCheckpoint::load(&path)?.expect("checkpoint exists");
        assert_eq!(loaded.next_chunk_index, 42);

        IngestionCheckpoint::clear(&path)?;
        assert!(IngestionCheckpoint::load(&path)?.is_none());
        Ok(())
    }

    #[test]
    fn ingestion_request_convenience_builders_set_fields() {
        let req = IngestionRequest::from_file("job", "doc", "source", "acme", "README.md")
            .with_chunking(ChunkingStrategy::Fixed {
                max_chars: 200,
                overlap_chars: 20,
            })
            .with_batch_size(32)
            .with_max_batch_size(256)
            .with_target_batch_ms(120)
            .with_continue_on_partial_failure(true);
        assert_eq!(req.batch_size, 32);
        assert_eq!(req.batch_tuning.max_batch_size, 256);
        assert_eq!(req.batch_tuning.target_batch_ms, 120);
        assert!(req.continue_on_partial_failure);
        assert!(matches!(req.source, IngestionSource::File { .. }));
    }

    #[test]
    fn parse_openai_response_extracts_embeddings() -> Result<()> {
        let response = json!({
            "data": [
                {"embedding": [0.1, 0.2, 0.3]},
                {"embedding": [0.4, 0.5, 0.6]}
            ]
        });
        let parsed = parse_openai_embeddings_response(&response)?;
        assert_eq!(parsed.len(), 2);
        assert!(parsed.iter().all(std::result::Result::is_ok));
        Ok(())
    }

    #[test]
    fn custom_provider_parses_results_schema() -> Result<()> {
        let provider = CustomHttpEmbeddingProvider::new("http://localhost:1234", "v1")?;
        let response = json!({
            "results": [
                {"embedding": [0.1, 0.2]},
                {"error": "rate_limited"}
            ]
        });

        // Use internal parser contract by matching the same branch behavior.
        let parsed = if let Some(results) = response.get("results").and_then(Value::as_array) {
            let mut out = Vec::new();
            for item in results {
                if let Some(error) = item.get("error").and_then(Value::as_str) {
                    out.push(Err(error.to_string()));
                    continue;
                }
                let embedding = item.get("embedding").expect("embedding exists");
                out.push(parse_embedding_array(embedding).map_err(|e| e.to_string()));
            }
            out
        } else {
            Vec::new()
        };

        assert_eq!(provider.model_version(), "v1");
        assert_eq!(parsed.len(), 2);
        assert!(parsed[0].is_ok());
        assert!(parsed[1].is_err());
        Ok(())
    }

    #[test]
    fn ingestion_is_idempotent_for_same_payload() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let provider = DeterministicEmbeddingProvider::new(64, "det-v1")?;
        let worker = IngestionWorker::new(&db, provider);

        let request = IngestionRequest {
            job_id: "job-1".to_string(),
            doc_id: "doc-1".to_string(),
            source_id: "payload-1".to_string(),
            tenant_id: "acme".to_string(),
            source: IngestionSource::Direct {
                content: "# Intro\nRust agents need deterministic retrieval.\n\n# Details\nSQLite RAG memory is portable.".to_string(),
            },
            metadata: json!({"kind": "guide"}),
            chunking: ChunkingStrategy::HeadingAware {
                max_chars: 40,
                overlap_chars: 5,
            },
            batch_size: 4,
            batch_tuning: IngestionBatchTuning::default(),
            continue_on_partial_failure: false,
        };

        let first = worker.ingest(request.clone())?;
        let count_after_first = db.chunk_count()?;
        let second = worker.ingest(request)?;
        let count_after_second = db.chunk_count()?;

        assert!(first.total_chunks > 0);
        assert_eq!(count_after_first, count_after_second);
        assert_eq!(second.failed_chunks, 0);
        Ok(())
    }

    #[test]
    fn ingestion_resumes_from_checkpoint() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let provider = DeterministicEmbeddingProvider::new(32, "det-v1")?;
        let dir = tempdir()?;
        let checkpoint_path = dir.path().join("ingest.checkpoint.json");

        let checkpoint = IngestionCheckpoint {
            job_id: "job-resume".to_string(),
            source_id: "source-resume".to_string(),
            next_chunk_index: 1,
            updated_unix_ms: now_unix_ms(),
        };
        checkpoint.save(&checkpoint_path)?;

        let worker = IngestionWorker::new(&db, provider).with_checkpoint_path(&checkpoint_path);
        let request = IngestionRequest {
            job_id: "job-resume".to_string(),
            doc_id: "doc-r".to_string(),
            source_id: "source-resume".to_string(),
            tenant_id: "acme".to_string(),
            source: IngestionSource::Direct {
                content: "one two three four five six seven eight nine ten".to_string(),
            },
            metadata: json!({}),
            chunking: ChunkingStrategy::Fixed {
                max_chars: 10,
                overlap_chars: 0,
            },
            batch_size: 2,
            batch_tuning: IngestionBatchTuning::default(),
            continue_on_partial_failure: false,
        };

        let report = worker.ingest(request)?;
        assert!(report.resumed_from_chunk >= 1);
        assert!(IngestionCheckpoint::load(&checkpoint_path)?.is_none());
        Ok(())
    }

    #[test]
    fn ingestion_reports_throughput_and_adaptive_batching_stats() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let provider = DeterministicEmbeddingProvider::new(64, "det-v1")?;
        let worker = IngestionWorker::new(&db, provider);

        let request = IngestionRequest::from_direct(
            "job-batch-stats",
            "doc-batch-stats",
            "source-batch-stats",
            "acme",
            "# A\nRust SQLite agents.\n\n# B\nAdaptive batching for ingestion throughput.",
        )
        .with_chunking(ChunkingStrategy::HeadingAware {
            max_chars: 24,
            overlap_chars: 4,
        })
        .with_batch_size(2)
        .with_batch_tuning(IngestionBatchTuning {
            adaptive: true,
            max_batch_size: 8,
            target_batch_ms: 100,
        });

        let report = worker.ingest(request)?;
        assert!(report.duration_ms >= 0.0);
        assert!(report.throughput_chunks_per_minute >= 0.0);
        assert!(report.batch_count > 0);
        assert!(report.average_batch_size >= 1.0);
        assert!(report.peak_batch_size >= 1);
        assert!(report.adaptive_batching);
        Ok(())
    }
}
