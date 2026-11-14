//! The embeddings client — the `pytorch-backend` route the two vector metrics need.
//!
//! It is one POST. What matters is the reading of the answer: the old service accepted either a
//! bare array or an object with an `embeddings` key, and treated anything else as "no vectors",
//! which is what turns an outage into `degraded` instead of a fabricated score.

use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("pytorch-backend is not configured: SCIENTIFIC_PROCESSOR_PYTORCH_BACKEND_URL is empty")]
    Unconfigured,
    #[error("pytorch-backend call failed: {0}")]
    Upstream(String),
}

#[derive(Debug, Clone)]
pub struct EmbeddingClient {
    base: String,
    http: reqwest::Client,
}

impl EmbeddingClient {
    pub fn new(base: Option<&str>, http: reqwest::Client) -> Option<Self> {
        let base = base?.trim_end_matches('/').to_owned();
        if base.is_empty() {
            return None;
        }
        Some(Self { base, http })
    }

    /// One embedding per input text, in the order they were sent.
    pub async fn embed(
        &self,
        texts: &[String],
        tenant: Option<&str>,
    ) -> Result<Vec<Vec<f64>>, EmbeddingError> {
        let mut request = self
            .http
            .post(format!("{}/api/ml/embeddings", self.base))
            .json(&json!({"texts": texts, "model": "default", "normalize": true}));
        if let Some(tenant) = tenant.filter(|value| !value.is_empty()) {
            request = request.header("X-Tenant-Id", tenant);
        }
        let response = request
            .send()
            .await
            .map_err(|error| EmbeddingError::Upstream(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::Upstream(format!(
                "{status}: {}",
                body.trim()
            )));
        }
        let document: Value = response
            .json()
            .await
            .map_err(|error| EmbeddingError::Upstream(error.to_string()))?;
        vectors(&document)
            .ok_or_else(|| EmbeddingError::Upstream("no embeddings in the answer".to_owned()))
    }
}

/// The old service's own read: `{"embeddings": [[…]]}`, or a bare `[[…]]`.
pub fn vectors(document: &Value) -> Option<Vec<Vec<f64>>> {
    let array = match document {
        Value::Array(items) => items,
        Value::Object(map) => map.get("embeddings")?.as_array()?,
        _ => return None,
    };
    let mut out = Vec::with_capacity(array.len());
    for item in array {
        let row = item.as_array()?;
        let mut vector = Vec::with_capacity(row.len());
        for value in row {
            vector.push(value.as_f64()?);
        }
        out.push(vector);
    }
    if out.is_empty() { None } else { Some(out) }
}

/// Cosine similarity, with the dimension guard the old service carried: two vectors of different
/// lengths are not comparable, and `0.0` says that without lying about a similarity.
pub fn cosine(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|y| y * y).sum::<f64>().sqrt();
    let norm = norm_a * norm_b;
    if norm > 0.0 { dot / norm } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_shapes_of_answer_are_read() {
        let wrapped = json!({"embeddings": [[1.0, 0.0], [0.0, 1.0]]});
        let bare = json!([[1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(vectors(&wrapped).expect("wrapped").len(), 2);
        assert_eq!(vectors(&bare).expect("bare").len(), 2);
        assert!(vectors(&json!({"other": []})).is_none());
        assert!(vectors(&json!([])).is_none());
    }

    #[test]
    fn cosine_is_the_angle_and_zero_on_a_mismatch() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-12);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-12);
        assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }
}
