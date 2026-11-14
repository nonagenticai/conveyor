//! `/api/verify` — the RAGAS-style faithfulness scores.
//!
//! The one idea this module exists to preserve: **`null` is not `0.5`**. A metric whose backend was
//! unreachable, or whose judge answered in prose instead of a number, is `null` with
//! `degraded: true` — never a plausible-looking score. The old service learned that the hard way,
//! and the caller can only tell "moderately faithful" from "the judge never answered" if the two
//! are different answers.
//!
//! Three metrics, run concurrently:
//!
//! - `faithfulness` — an LLM judge reads the answer against the contexts.
//! - `answer_relevancy` — cosine similarity between the question and the answer embeddings.
//! - `context_precision` — a rank-weighted mean of the question's similarity to each context.
//!
//! `factual_accuracy` is their derived mean and is `null` whenever either input is.

use std::time::Instant;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use crate::clients::embeddings::{EmbeddingClient, cosine};
use crate::clients::inference::InferenceClient;
use crate::router::Failure;
use crate::state::Shared;

const MAX_QUESTION: usize = 2000;
const MAX_ANSWER: usize = 10_000;
/// How many contexts the judge reads and how many the precision metric weighs, as the old service
/// capped them.
const JUDGE_CONTEXTS: usize = 5;
const PRECISION_CONTEXTS: usize = 10;

const FAITHFULNESS_PROMPT: &str =
    "Evaluate the faithfulness of the following answer based on the given contexts.
Faithfulness measures whether every claim in the answer can be inferred from the contexts.

Contexts:
{contexts}

Answer: {answer}

Rate faithfulness from 0.0 (no claims supported) to 1.0 (all claims supported).
Respond with ONLY a number between 0.0 and 1.0.";

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct RagasRequest {
    pub question: String,
    pub answer: String,
    pub contexts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct RagasResponse {
    pub faithfulness: Option<f64>,
    pub answer_relevancy: Option<f64>,
    pub context_precision: Option<f64>,
    pub factual_accuracy: Option<f64>,
    pub computation_time_ms: f64,
    pub degraded: bool,
}

/// `POST /api/verify/ragas`
#[utoipa::path(
    post,
    path = "/api/verify/ragas",
    operation_id = "verification_ragas",
    request_body = RagasRequest,
    responses((status = 200, body = RagasResponse), (status = 422, body = Value)),
    tag = "verification"
)]
pub async fn ragas(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<RagasRequest>,
) -> Result<Json<RagasResponse>, Failure> {
    if body.question.chars().count() > MAX_QUESTION {
        return Err(unprocessable("question exceeds 2000 characters"));
    }
    if body.answer.chars().count() > MAX_ANSWER {
        return Err(unprocessable("answer exceeds 10000 characters"));
    }
    if body.contexts.is_empty() {
        return Err(unprocessable("contexts must contain at least one entry"));
    }

    let started = Instant::now();
    let tenant = optional_tenant(&headers);

    let (faithfulness, relevancy, precision) = tokio::join!(
        judge(&state, &body.answer, &body.contexts, tenant),
        relevancy(&state, &body.question, &body.answer, tenant),
        precision(&state, &body.question, &body.contexts, tenant),
    );

    let factual_accuracy = match (faithfulness, precision) {
        (Some(f), Some(p)) => Some((f + p) / 2.0),
        _ => None,
    };
    let degraded = faithfulness.is_none() || relevancy.is_none() || precision.is_none();

    Ok(Json(RagasResponse {
        faithfulness: round4(faithfulness),
        answer_relevancy: round4(relevancy),
        context_precision: round4(precision),
        factual_accuracy: round4(factual_accuracy),
        computation_time_ms: round2(started.elapsed().as_secs_f64() * 1000.0),
        degraded,
    }))
}

/// The LLM judge. `None` on any failure — unreachable, empty, or an answer that is not a number.
async fn judge(
    state: &Shared,
    answer: &str,
    contexts: &[String],
    tenant: Option<&str>,
) -> Option<f64> {
    let inference: &InferenceClient = state.inference.as_ref()?;
    let context_text = contexts
        .iter()
        .take(JUDGE_CONTEXTS)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n---\n");
    let prompt = FAITHFULNESS_PROMPT
        .replace("{contexts}", &context_text)
        .replace("{answer}", answer);
    let content = inference
        .chat(&prompt, 512, "sp-ragas-faithfulness", tenant)
        .await
        .ok()?;
    parse_score(&content)
}

/// Cosine similarity between the question and the answer.
async fn relevancy(
    state: &Shared,
    question: &str,
    answer: &str,
    tenant: Option<&str>,
) -> Option<f64> {
    let client: &EmbeddingClient = state.embeddings.as_ref()?;
    let vectors = client
        .embed(&[question.to_owned(), answer.to_owned()], tenant)
        .await
        .ok()?;
    if vectors.len() < 2 {
        return None;
    }
    Some(cosine(&vectors[0], &vectors[1]))
}

/// Rank-weighted relevance of the contexts to the question.
async fn precision(
    state: &Shared,
    question: &str,
    contexts: &[String],
    tenant: Option<&str>,
) -> Option<f64> {
    let client: &EmbeddingClient = state.embeddings.as_ref()?;
    let mut texts = Vec::with_capacity(1 + contexts.len().min(PRECISION_CONTEXTS));
    texts.push(question.to_owned());
    texts.extend(contexts.iter().take(PRECISION_CONTEXTS).cloned());
    let vectors = client.embed(&texts, tenant).await.ok()?;
    if vectors.len() < 2 {
        return None;
    }
    let question_vector = &vectors[0];
    let scores: Vec<f64> = vectors[1..]
        .iter()
        .map(|context| cosine(question_vector, context))
        .collect();
    if scores.is_empty() {
        return Some(0.0);
    }
    // Earlier contexts weigh more: 1, 1/2, 1/3, …
    let weights: Vec<f64> = (0..scores.len()).map(|i| 1.0 / (i as f64 + 1.0)).collect();
    let weighted: f64 = scores.iter().zip(&weights).map(|(s, w)| s * w).sum();
    let total: f64 = weights.iter().sum();
    Some((weighted / total).min(1.0))
}

/// The judge's first token as a number, or `None` — a reachable judge that answers in prose has
/// not scored anything.
fn parse_score(content: &str) -> Option<f64> {
    let first = content.split_whitespace().next()?;
    let value: f64 = first.trim_end_matches([',', '.', ';']).parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    Some(value.clamp(0.0, 1.0))
}

fn round4(value: Option<f64>) -> Option<f64> {
    value.map(|value| (value * 10_000.0).round() / 10_000.0)
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn unprocessable(message: &str) -> Failure {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({"detail": message})),
    )
}

fn optional_tenant(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::AppState;
    use std::sync::Arc;

    fn state() -> Shared {
        Arc::new(AppState::new(
            Config::from_lookup(&[]).expect("defaults parse"),
        ))
    }

    #[tokio::test]
    async fn no_backends_is_degraded_nulls_not_a_score() {
        let Json(body) = ragas(
            State(state()),
            HeaderMap::new(),
            Json(RagasRequest {
                question: "q".to_owned(),
                answer: "a".to_owned(),
                contexts: vec!["c".to_owned()],
            }),
        )
        .await
        .expect("answers");
        assert!(body.degraded);
        assert!(body.faithfulness.is_none());
        assert!(body.answer_relevancy.is_none());
        assert!(body.context_precision.is_none());
        assert!(body.factual_accuracy.is_none());
    }

    #[tokio::test]
    async fn an_empty_context_list_is_refused() {
        let result = ragas(
            State(state()),
            HeaderMap::new(),
            Json(RagasRequest {
                question: "q".to_owned(),
                answer: "a".to_owned(),
                contexts: Vec::new(),
            }),
        )
        .await;
        let (status, _) = result.expect_err("refused");
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn a_judge_that_answers_in_prose_has_not_scored() {
        assert_eq!(parse_score("0.75"), Some(0.75));
        assert_eq!(
            parse_score("0.8, because the answer is grounded"),
            Some(0.8)
        );
        assert_eq!(parse_score("1.4"), Some(1.0));
        assert_eq!(parse_score("I cannot rate this"), None);
        assert_eq!(parse_score(""), None);
        assert_eq!(parse_score("nan"), None);
    }
}
