use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

pub const DEFAULT_TOP_K: usize = 5;
pub const DEFAULT_ALPHA: f32 = 0.65;
pub const DEFAULT_CANDIDATE_LIMIT: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SqlRequest {
    pub statement: String,
}

impl SqlRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.statement.trim().is_empty() {
            return Err(ValidationError::EmptyStatement);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QueryRequest {
    pub query_text: Option<String>,
    pub query_embedding: Option<Vec<f32>>,
    pub top_k: Option<usize>,
    pub alpha: Option<f32>,
    pub candidate_limit: Option<usize>,
    pub metadata_filters: Option<HashMap<String, String>>,
    pub doc_id: Option<String>,
}

impl QueryRequest {
    pub fn top_k_or_default(&self) -> usize {
        self.top_k.unwrap_or(DEFAULT_TOP_K)
    }

    pub fn alpha_or_default(&self) -> f32 {
        self.alpha.unwrap_or(DEFAULT_ALPHA)
    }

    pub fn candidate_limit_or_default(&self) -> usize {
        self.candidate_limit.unwrap_or(DEFAULT_CANDIDATE_LIMIT)
    }

    pub fn normalized_query_text(&self) -> Option<String> {
        self.query_text
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }

    pub fn normalized_doc_id(&self) -> Option<String> {
        self.doc_id
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }

    pub fn normalized_query_embedding(&self) -> Option<Vec<f32>> {
        self.query_embedding
            .as_ref()
            .filter(|value| !value.is_empty())
            .cloned()
    }

    pub fn normalized_metadata_filters(&self) -> HashMap<String, String> {
        self.metadata_filters.clone().unwrap_or_default()
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        let has_query_text = self
            .query_text
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        let has_query_embedding = self
            .query_embedding
            .as_ref()
            .map(|value| !value.is_empty())
            .unwrap_or(false);

        if !has_query_text && !has_query_embedding {
            return Err(ValidationError::MissingQuery);
        }

        let top_k = self.top_k_or_default();
        if top_k == 0 {
            return Err(ValidationError::InvalidTopK);
        }

        let candidate_limit = self.candidate_limit_or_default();
        if candidate_limit == 0 {
            return Err(ValidationError::InvalidCandidateLimit);
        }
        if candidate_limit < top_k {
            return Err(ValidationError::CandidateLimitTooSmall {
                top_k,
                candidate_limit,
            });
        }

        let alpha = self.alpha_or_default();
        if !(0.0..=1.0).contains(&alpha) {
            return Err(ValidationError::InvalidAlpha(alpha));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryEnvelope<T> {
    pub kind: String,
    pub row_count: usize,
    pub rows: Vec<T>,
}

impl<T> QueryEnvelope<T> {
    pub fn from_rows(rows: Vec<T>) -> Self {
        Self {
            row_count: rows.len(),
            rows,
            kind: "query".to_string(),
        }
    }
}

#[derive(Debug, Clone, Error, PartialEq)]
pub enum ValidationError {
    #[error("statement cannot be empty")]
    EmptyStatement,
    #[error("query_text or query_embedding is required")]
    MissingQuery,
    #[error("top_k must be >= 1")]
    InvalidTopK,
    #[error("candidate_limit must be >= 1")]
    InvalidCandidateLimit,
    #[error("candidate_limit ({candidate_limit}) must be >= top_k ({top_k})")]
    CandidateLimitTooSmall {
        top_k: usize,
        candidate_limit: usize,
    },
    #[error("alpha must be between 0.0 and 1.0 (received {0})")]
    InvalidAlpha(f32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_request_requires_text_or_embedding() {
        let request = QueryRequest::default();
        assert_eq!(request.validate(), Err(ValidationError::MissingQuery));
    }

    #[test]
    fn query_request_rejects_candidate_limit_smaller_than_top_k() {
        let request = QueryRequest {
            query_text: Some("agent".to_string()),
            top_k: Some(10),
            candidate_limit: Some(2),
            ..QueryRequest::default()
        };
        assert_eq!(
            request.validate(),
            Err(ValidationError::CandidateLimitTooSmall {
                top_k: 10,
                candidate_limit: 2,
            })
        );
    }

    #[test]
    fn query_request_accepts_defaulted_values() {
        let request = QueryRequest {
            query_text: Some("agent".to_string()),
            ..QueryRequest::default()
        };
        assert_eq!(request.top_k_or_default(), DEFAULT_TOP_K);
        assert_eq!(
            request.candidate_limit_or_default(),
            DEFAULT_CANDIDATE_LIMIT
        );
        assert!((request.alpha_or_default() - DEFAULT_ALPHA).abs() < f32::EPSILON);
        assert!(request.validate().is_ok());
    }

    #[test]
    fn sql_request_rejects_blank_statement() {
        let request = SqlRequest {
            statement: "   ".to_string(),
        };
        assert_eq!(request.validate(), Err(ValidationError::EmptyStatement));
    }
}
