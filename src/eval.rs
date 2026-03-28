use crate::{
    ChunkInput, FusionStrategy, QueryProfile, Result, RuntimeConfig, SearchRequest, SqlRite,
    SqlRiteError,
};
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::collections::{HashMap, HashSet};

fn default_k_values() -> Vec<usize> {
    vec![1, 3, 5, 10]
}

fn default_alpha() -> f32 {
    0.65
}

fn default_candidate_limit() -> usize {
    1000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalDataset {
    pub corpus: Vec<ChunkInput>,
    pub queries: Vec<EvalQuery>,
    #[serde(default = "default_k_values")]
    pub k_values: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQuery {
    pub id: String,
    pub query_text: Option<String>,
    pub query_embedding: Option<Vec<f32>>,
    pub relevant_chunk_ids: Vec<String>,
    #[serde(default)]
    pub metadata_filters: HashMap<String, String>,
    pub doc_id: Option<String>,
    #[serde(default = "default_alpha")]
    pub alpha: f32,
    #[serde(default = "default_candidate_limit")]
    pub candidate_limit: usize,
    pub top_k: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalMetricsAtK {
    pub recall: f32,
    pub precision: f32,
    pub mrr: f32,
    pub ndcg: f32,
    pub hit_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryEvalResult {
    pub query_id: String,
    pub retrieved_chunk_ids: Vec<String>,
    pub relevant_chunk_ids: Vec<String>,
    pub metrics_at_k: HashMap<usize, EvalMetricsAtK>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSummary {
    pub corpus_size: usize,
    pub query_count: usize,
    pub k_values: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub summary: EvalSummary,
    pub aggregate_metrics_at_k: HashMap<usize, EvalMetricsAtK>,
    pub per_query: Vec<QueryEvalResult>,
}

pub fn evaluate_dataset(dataset: EvalDataset, runtime_config: RuntimeConfig) -> Result<EvalReport> {
    let k_values = normalized_k_values(&dataset.k_values)?;
    validate_dataset(&dataset, &k_values)?;

    let max_k = *k_values.last().expect("k_values cannot be empty");
    let db = SqlRite::open_in_memory_with_config(runtime_config)?;
    db.ingest_chunks(&dataset.corpus)?;

    let mut per_query = Vec::with_capacity(dataset.queries.len());
    let mut aggregate: HashMap<usize, EvalMetricAccumulator> = HashMap::new();
    for &k in &k_values {
        aggregate.insert(k, EvalMetricAccumulator::default());
    }

    for query in &dataset.queries {
        let top_k = query.top_k.unwrap_or(max_k).max(max_k);
        let request = SearchRequest {
            query_text: query.query_text.clone(),
            query_embedding: query.query_embedding.clone(),
            top_k,
            alpha: query.alpha,
            candidate_limit: query.candidate_limit.max(top_k),
            include_payloads: true,
            query_profile: QueryProfile::Balanced,
            metadata_filters: query.metadata_filters.clone(),
            doc_id: query.doc_id.clone(),
            fusion_strategy: FusionStrategy::Weighted,
        };
        let search_results = db.search(request)?;
        let ranked_ids: Vec<String> = search_results.into_iter().map(|r| r.chunk_id).collect();

        let relevant_set: HashSet<&str> = query
            .relevant_chunk_ids
            .iter()
            .map(String::as_str)
            .collect();
        let mut metrics_at_k = HashMap::new();

        for &k in &k_values {
            let metrics = compute_metrics_at_k(&ranked_ids, &relevant_set, k);
            metrics_at_k.insert(k, metrics.clone());

            if let Some(acc) = aggregate.get_mut(&k) {
                acc.add(&metrics);
            }
        }

        per_query.push(QueryEvalResult {
            query_id: query.id.clone(),
            retrieved_chunk_ids: ranked_ids,
            relevant_chunk_ids: query.relevant_chunk_ids.clone(),
            metrics_at_k,
        });
    }

    let mut aggregate_metrics_at_k = HashMap::new();
    for &k in &k_values {
        let metrics = aggregate
            .remove(&k)
            .expect("aggregate key exists")
            .mean(dataset.queries.len());
        aggregate_metrics_at_k.insert(k, metrics);
    }

    Ok(EvalReport {
        summary: EvalSummary {
            corpus_size: dataset.corpus.len(),
            query_count: dataset.queries.len(),
            k_values,
        },
        aggregate_metrics_at_k,
        per_query,
    })
}

#[derive(Debug, Default, Clone)]
struct EvalMetricAccumulator {
    recall: f32,
    precision: f32,
    mrr: f32,
    ndcg: f32,
    hit_rate: f32,
}

impl EvalMetricAccumulator {
    fn add(&mut self, metrics: &EvalMetricsAtK) {
        self.recall += metrics.recall;
        self.precision += metrics.precision;
        self.mrr += metrics.mrr;
        self.ndcg += metrics.ndcg;
        self.hit_rate += metrics.hit_rate;
    }

    fn mean(self, count: usize) -> EvalMetricsAtK {
        let denom = count as f32;
        EvalMetricsAtK {
            recall: self.recall / denom,
            precision: self.precision / denom,
            mrr: self.mrr / denom,
            ndcg: self.ndcg / denom,
            hit_rate: self.hit_rate / denom,
        }
    }
}

fn compute_metrics_at_k(
    ranked_ids: &[String],
    relevant_ids: &HashSet<&str>,
    k: usize,
) -> EvalMetricsAtK {
    let relevant_count = relevant_ids.len();
    if relevant_count == 0 || k == 0 {
        return EvalMetricsAtK {
            recall: 0.0,
            precision: 0.0,
            mrr: 0.0,
            ndcg: 0.0,
            hit_rate: 0.0,
        };
    }

    let cutoff = min(k, ranked_ids.len());
    let hits = ranked_ids
        .iter()
        .take(cutoff)
        .filter(|id| relevant_ids.contains(id.as_str()))
        .count();
    let recall = hits as f32 / relevant_count as f32;
    let precision = hits as f32 / k as f32;
    let hit_rate = if hits > 0 { 1.0 } else { 0.0 };

    let mut reciprocal_rank = 0.0;
    for (idx, id) in ranked_ids.iter().take(cutoff).enumerate() {
        if relevant_ids.contains(id.as_str()) {
            reciprocal_rank = 1.0 / (idx as f32 + 1.0);
            break;
        }
    }

    let mut dcg = 0.0;
    for (idx, id) in ranked_ids.iter().take(cutoff).enumerate() {
        if relevant_ids.contains(id.as_str()) {
            dcg += 1.0 / ((idx as f32 + 2.0).log2());
        }
    }

    let ideal_hits = min(relevant_count, k);
    let mut idcg = 0.0;
    for idx in 0..ideal_hits {
        idcg += 1.0 / ((idx as f32 + 2.0).log2());
    }
    let ndcg = if idcg > 0.0 { dcg / idcg } else { 0.0 };

    EvalMetricsAtK {
        recall,
        precision,
        mrr: reciprocal_rank,
        ndcg,
        hit_rate,
    }
}

fn normalized_k_values(values: &[usize]) -> Result<Vec<usize>> {
    let mut unique: Vec<usize> = values.iter().copied().filter(|v| *v > 0).collect();
    unique.sort_unstable();
    unique.dedup();
    if unique.is_empty() {
        return Err(SqlRiteError::InvalidEvaluationDataset(
            "k_values must contain at least one positive integer".to_string(),
        ));
    }
    Ok(unique)
}

fn validate_dataset(dataset: &EvalDataset, k_values: &[usize]) -> Result<()> {
    if dataset.corpus.is_empty() {
        return Err(SqlRiteError::InvalidEvaluationDataset(
            "corpus cannot be empty".to_string(),
        ));
    }
    if dataset.queries.is_empty() {
        return Err(SqlRiteError::InvalidEvaluationDataset(
            "queries cannot be empty".to_string(),
        ));
    }
    if k_values.is_empty() {
        return Err(SqlRiteError::InvalidEvaluationDataset(
            "k_values cannot be empty".to_string(),
        ));
    }

    for query in &dataset.queries {
        if query.query_text.is_none() && query.query_embedding.is_none() {
            return Err(SqlRiteError::InvalidEvaluationDataset(format!(
                "query `{}` must contain query_text, query_embedding, or both",
                query.id
            )));
        }
        if query.relevant_chunk_ids.is_empty() {
            return Err(SqlRiteError::InvalidEvaluationDataset(format!(
                "query `{}` has no relevant_chunk_ids",
                query.id
            )));
        }
        if query.candidate_limit == 0 {
            return Err(SqlRiteError::InvalidEvaluationDataset(format!(
                "query `{}` candidate_limit must be >= 1",
                query.id
            )));
        }
        if !(0.0..=1.0).contains(&query.alpha) {
            return Err(SqlRiteError::InvalidEvaluationDataset(format!(
                "query `{}` alpha must be between 0.0 and 1.0",
                query.id
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_dataset() -> EvalDataset {
        EvalDataset {
            corpus: vec![
                ChunkInput {
                    id: "c1".to_string(),
                    doc_id: "d1".to_string(),
                    content: "Rust for retrieval".to_string(),
                    embedding: vec![1.0, 0.0, 0.0],
                    metadata: json!({"tenant": "acme"}),
                    source: None,
                },
                ChunkInput {
                    id: "c2".to_string(),
                    doc_id: "d2".to_string(),
                    content: "Postgres transactions".to_string(),
                    embedding: vec![0.0, 1.0, 0.0],
                    metadata: json!({"tenant": "acme"}),
                    source: None,
                },
                ChunkInput {
                    id: "c3".to_string(),
                    doc_id: "d3".to_string(),
                    content: "SQLite local memory".to_string(),
                    embedding: vec![0.8, 0.2, 0.0],
                    metadata: json!({"tenant": "acme"}),
                    source: None,
                },
            ],
            queries: vec![
                EvalQuery {
                    id: "q1".to_string(),
                    query_text: Some("rust retrieval".to_string()),
                    query_embedding: Some(vec![0.95, 0.05, 0.0]),
                    relevant_chunk_ids: vec!["c1".to_string()],
                    metadata_filters: HashMap::new(),
                    doc_id: None,
                    alpha: 0.6,
                    candidate_limit: 10,
                    top_k: Some(3),
                },
                EvalQuery {
                    id: "q2".to_string(),
                    query_text: Some("sqlite memory".to_string()),
                    query_embedding: Some(vec![0.75, 0.25, 0.0]),
                    relevant_chunk_ids: vec!["c3".to_string()],
                    metadata_filters: HashMap::new(),
                    doc_id: None,
                    alpha: 0.5,
                    candidate_limit: 10,
                    top_k: Some(3),
                },
            ],
            k_values: vec![1, 3],
        }
    }

    #[test]
    fn compute_metrics_is_correct_for_simple_case() {
        let ranked = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let relevant: HashSet<&str> = HashSet::from(["b"]);
        let m = compute_metrics_at_k(&ranked, &relevant, 3);
        assert!((m.recall - 1.0).abs() < 1e-6);
        assert!((m.precision - (1.0 / 3.0)).abs() < 1e-6);
        assert!((m.mrr - 0.5).abs() < 1e-6);
        assert!(m.ndcg > 0.6 && m.ndcg < 0.7);
        assert!((m.hit_rate - 1.0).abs() < 1e-6);
    }

    #[test]
    fn evaluation_report_has_aggregate_metrics() -> Result<()> {
        let report = evaluate_dataset(sample_dataset(), RuntimeConfig::default())?;
        assert_eq!(report.summary.corpus_size, 3);
        assert_eq!(report.summary.query_count, 2);
        assert_eq!(report.per_query.len(), 2);
        assert!(report.aggregate_metrics_at_k.contains_key(&1));
        assert!(report.aggregate_metrics_at_k.contains_key(&3));
        Ok(())
    }
}
