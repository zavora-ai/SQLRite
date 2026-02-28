use crate::{
    ChunkInput, EmbeddingProvider, EmbeddingRetryPolicy, Result, SqlRite, SqlRiteError, StoredChunk,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReindexCheckpoint {
    pub offset: usize,
    pub updated_unix_ms: u64,
}

impl ReindexCheckpoint {
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
pub struct ReindexOptions {
    pub batch_size: usize,
    pub tenant_id: Option<String>,
    pub target_model_version: String,
    pub only_if_model_mismatch: bool,
    pub continue_on_partial_failure: bool,
    pub checkpoint_path: Option<PathBuf>,
    pub retry_policy: EmbeddingRetryPolicy,
}

impl Default for ReindexOptions {
    fn default() -> Self {
        Self {
            batch_size: 256,
            tenant_id: None,
            target_model_version: "det-v2".to_string(),
            only_if_model_mismatch: true,
            continue_on_partial_failure: false,
            checkpoint_path: None,
            retry_policy: EmbeddingRetryPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReindexReport {
    pub scanned_chunks: usize,
    pub updated_chunks: usize,
    pub skipped_chunks: usize,
    pub failed_chunks: usize,
    pub resumed_from_offset: usize,
    pub provider: String,
    pub model_version: String,
}

pub fn reindex_embeddings<P: EmbeddingProvider>(
    db: &SqlRite,
    provider: P,
    options: ReindexOptions,
) -> Result<ReindexReport> {
    if options.batch_size == 0 {
        return Err(SqlRiteError::InvalidBenchmarkConfig(
            "reindex batch_size must be >= 1".to_string(),
        ));
    }

    let resumed_from_offset = load_resume_offset(options.checkpoint_path.as_deref())?;
    let mut offset = resumed_from_offset;
    let mut scanned_chunks = 0usize;
    let mut updated_chunks = 0usize;
    let mut skipped_chunks = 0usize;
    let mut failed_chunks = 0usize;

    loop {
        let page = db.list_chunks_page(offset, options.batch_size, options.tenant_id.as_deref())?;
        if page.is_empty() {
            break;
        }

        scanned_chunks += page.len();
        let mut candidate_rows = Vec::new();
        let mut texts = Vec::new();

        for chunk in page {
            if should_skip_chunk(&chunk, &options) {
                skipped_chunks += 1;
                continue;
            }
            texts.push(chunk.content.clone());
            candidate_rows.push(chunk);
        }

        let embedded = embed_with_retry(&provider, &texts, &options.retry_policy)?;
        let mut upserts = Vec::new();

        for (idx, chunk) in candidate_rows.iter().enumerate() {
            let Some(embedding) = embedded[idx].clone() else {
                failed_chunks += 1;
                continue;
            };

            let metadata = enrich_reindex_metadata(
                &chunk.metadata,
                provider.provider_name(),
                &options.target_model_version,
            );

            upserts.push(ChunkInput {
                id: chunk.id.clone(),
                doc_id: chunk.doc_id.clone(),
                content: chunk.content.clone(),
                embedding,
                metadata,
                source: chunk.source.clone(),
            });
        }

        if !upserts.is_empty() {
            db.ingest_chunks(&upserts)?;
            updated_chunks += upserts.len();
        }

        if failed_chunks > 0 && !options.continue_on_partial_failure {
            return Err(SqlRiteError::EmbeddingBatchPartialFailure {
                failed: failed_chunks,
            });
        }

        offset += options.batch_size;
        save_resume_offset(options.checkpoint_path.as_deref(), offset)?;
    }

    clear_resume_offset(options.checkpoint_path.as_deref())?;

    Ok(ReindexReport {
        scanned_chunks,
        updated_chunks,
        skipped_chunks,
        failed_chunks,
        resumed_from_offset,
        provider: provider.provider_name().to_string(),
        model_version: options.target_model_version,
    })
}

fn embed_with_retry<P: EmbeddingProvider>(
    provider: &P,
    texts: &[String],
    retry_policy: &EmbeddingRetryPolicy,
) -> Result<Vec<Option<Vec<f32>>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let mut pending: Vec<usize> = (0..texts.len()).collect();
    let mut resolved = vec![None; texts.len()];
    let mut attempt = 0usize;
    let mut backoff_ms = retry_policy.initial_backoff_ms.max(1);

    while !pending.is_empty() && attempt <= retry_policy.max_retries {
        let current_texts = pending
            .iter()
            .map(|idx| texts[*idx].clone())
            .collect::<Vec<_>>();
        let responses = provider.embed_batch(&current_texts)?;

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
            if attempt <= retry_policy.max_retries {
                thread::sleep(Duration::from_millis(backoff_ms));
                backoff_ms = (backoff_ms.saturating_mul(2)).min(retry_policy.max_backoff_ms);
            }
        }
    }

    Ok(resolved)
}

fn should_skip_chunk(chunk: &StoredChunk, options: &ReindexOptions) -> bool {
    if !options.only_if_model_mismatch {
        return false;
    }

    chunk
        .metadata
        .get("embedding_model_version")
        .and_then(Value::as_str)
        == Some(options.target_model_version.as_str())
}

fn enrich_reindex_metadata(base: &Value, provider: &str, model_version: &str) -> Value {
    let mut metadata = match base {
        Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };

    metadata.insert(
        "embedding_provider".to_string(),
        Value::String(provider.to_string()),
    );
    metadata.insert(
        "embedding_model_version".to_string(),
        Value::String(model_version.to_string()),
    );
    metadata.insert(
        "reindexed_at_unix_ms".to_string(),
        Value::Number(serde_json::Number::from(now_unix_ms())),
    );

    Value::Object(metadata)
}

fn load_resume_offset(path: Option<&Path>) -> Result<usize> {
    let Some(path) = path else {
        return Ok(0);
    };
    Ok(ReindexCheckpoint::load(path)?
        .map(|checkpoint| checkpoint.offset)
        .unwrap_or(0))
}

fn save_resume_offset(path: Option<&Path>, offset: usize) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    ReindexCheckpoint {
        offset,
        updated_unix_ms: now_unix_ms(),
    }
    .save(path)
}

fn clear_resume_offset(path: Option<&Path>) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    ReindexCheckpoint::clear(path)
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeterministicEmbeddingProvider, RuntimeConfig};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn reindex_updates_embedding_model_version() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let provider_v1 = DeterministicEmbeddingProvider::new(32, "det-v1")?;
        let provider_v2 = DeterministicEmbeddingProvider::new(32, "det-v2")?;

        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "reindex me".to_string(),
            embedding: provider_v1
                .embed_batch(&["reindex me".to_string()])?
                .into_iter()
                .next()
                .and_then(std::result::Result::ok)
                .expect("deterministic embedding"),
            metadata: json!({"tenant": "acme", "embedding_model_version": "det-v1"}),
            source: None,
        })?;

        let report = reindex_embeddings(
            &db,
            provider_v2,
            ReindexOptions {
                target_model_version: "det-v2".to_string(),
                ..ReindexOptions::default()
            },
        )?;

        assert_eq!(report.updated_chunks, 1);
        let chunks = db.list_chunks_page(0, 10, None)?;
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            chunks[0]
                .metadata
                .get("embedding_model_version")
                .and_then(Value::as_str),
            Some("det-v2")
        );
        Ok(())
    }

    #[test]
    fn reindex_resumes_from_checkpoint() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let provider_v1 = DeterministicEmbeddingProvider::new(16, "det-v1")?;
        let provider_v2 = DeterministicEmbeddingProvider::new(16, "det-v2")?;

        for idx in 0..3 {
            let content = format!("reindex row {idx}");
            let embedding = provider_v1
                .embed_batch(std::slice::from_ref(&content))?
                .into_iter()
                .next()
                .and_then(std::result::Result::ok)
                .expect("deterministic embedding");
            db.ingest_chunk(&ChunkInput {
                id: format!("c{idx}"),
                doc_id: "d1".to_string(),
                content,
                embedding,
                metadata: json!({"tenant": "acme", "embedding_model_version": "det-v1"}),
                source: None,
            })?;
        }

        let dir = tempdir()?;
        let checkpoint_path = dir.path().join("reindex.checkpoint.json");
        ReindexCheckpoint {
            offset: 1,
            updated_unix_ms: now_unix_ms(),
        }
        .save(&checkpoint_path)?;

        let report = reindex_embeddings(
            &db,
            provider_v2,
            ReindexOptions {
                batch_size: 1,
                checkpoint_path: Some(checkpoint_path.clone()),
                target_model_version: "det-v2".to_string(),
                ..ReindexOptions::default()
            },
        )?;

        assert_eq!(report.resumed_from_offset, 1);
        assert!(ReindexCheckpoint::load(checkpoint_path)?.is_none());
        Ok(())
    }
}
