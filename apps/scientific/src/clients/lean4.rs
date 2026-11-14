//! The lean4-prover client.
//!
//! Two endpoints, one call each: `/api/elaborate` type-checks a term, `/api/check` proves a
//! declaration. What matters is the failure contract, not the call: when the GPU is saturated the
//! prover answers `503 gpu_busy` with a `Retry-After`, and that is a **transient shed**, never a
//! `500` — the caller re-drives, and the header says when.
//!
//! The forwarder does not retry internally. The old Python client retried behind a deadline; here
//! the retry is the caller's, because the answer the caller gets (`503` + `Retry-After`) is more
//! useful than one this process hides. A bounded retry loop here would also need the GPU admission
//! accounting the prover already keeps, and a second copy of a queue is a second truth.

use std::time::Duration;

use serde_json::{Value, json};

/// The `Retry-After` bounds the fleet contract allows.
const RETRY_AFTER_MIN: u32 = 1;
const RETRY_AFTER_MAX: u32 = 120;

/// The value used when the prover sheds without a usable `Retry-After` header.
const RETRY_AFTER_DEFAULT: u32 = 10;

#[derive(Debug, thiserror::Error)]
pub enum Lean4Error {
    #[error("lean4-prover is not configured: SCIENTIFIC_PROCESSOR_LEAN4_PROVER_URL is empty")]
    Unconfigured,
    #[error(
        "lean4-prover call has no licence token: \
         SCIENTIFIC_PROCESSOR_DOWNSTREAM_LICENSE_TOKEN is empty"
    )]
    MissingLicence,
    #[error("lean4-prover is busy; retry after {retry_after}s")]
    Busy { retry_after: u32 },
    #[error("lean4-prover call failed: {0}")]
    Upstream(String),
}

/// A configured prover endpoint.
#[derive(Debug, Clone)]
pub struct Lean4Client {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
    timeout_seconds: u64,
}

impl Lean4Client {
    /// `None` when the endpoint is empty — a service with no prover is a service whose formal
    /// routes answer "not configured", never one that guesses a default host.
    pub fn new(
        base: Option<&str>,
        token: Option<&str>,
        http: reqwest::Client,
        timeout_seconds: u64,
    ) -> Option<Self> {
        let base = base?.trim_end_matches('/').to_owned();
        if base.is_empty() {
            return None;
        }
        Some(Self {
            base,
            token: token.map(str::to_owned).filter(|t| !t.is_empty()),
            http,
            timeout_seconds,
        })
    }

    /// Type-check a term without proving anything.
    pub async fn elaborate(&self, term: &str) -> Result<Value, Lean4Error> {
        self.post(
            "/api/elaborate",
            json!({"term": term, "timeout": self.timeout_seconds}),
        )
        .await
    }

    /// Check a declaration, which is what a proof is.
    pub async fn check(&self, source: &str) -> Result<Value, Lean4Error> {
        self.post(
            "/api/check",
            json!({"source": source, "timeout": self.timeout_seconds}),
        )
        .await
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, Lean4Error> {
        let token = self.token.as_deref().ok_or(Lean4Error::MissingLicence)?;
        let response = self
            .http
            .post(format!("{}{path}", self.base))
            // The prover's own `timeout` field bounds the work; this bounds the hop, with a
            // margin so the server's answer (rather than a client abort) is what the caller sees.
            .timeout(Duration::from_secs(self.timeout_seconds + 60))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|error| Lean4Error::Upstream(error.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::SERVICE_UNAVAILABLE
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            return Err(Lean4Error::Busy {
                retry_after: retry_after(response.headers()),
            });
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Lean4Error::Upstream(format!("{status}: {}", body.trim())));
        }
        response
            .json::<Value>()
            .await
            .map_err(|error| Lean4Error::Upstream(error.to_string()))
    }
}

/// The clamped `Retry-After`, in seconds.
///
/// The header may be fractional in a proxy's rewriting and may be absent entirely; both are
/// clamped into the range the fleet contract allows rather than passed through as-is.
fn retry_after(headers: &reqwest::header::HeaderMap) -> u32 {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite())
        .map(|seconds| {
            let rounded = seconds.ceil().max(0.0) as u64;
            rounded.clamp(RETRY_AFTER_MIN as u64, RETRY_AFTER_MAX as u64) as u32
        })
        .unwrap_or(RETRY_AFTER_DEFAULT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use axum::{Json, Router};

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("ephemeral port");
        let addr = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serves");
        });
        format!("http://{addr}")
    }

    fn client(base: &str, token: Option<&str>) -> Lean4Client {
        Lean4Client::new(
            Some(base),
            token,
            reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("client"),
            60,
        )
        .expect("configured")
    }

    #[tokio::test]
    async fn a_busy_prover_is_a_retry_after_not_an_error() {
        let app = Router::new().route(
            "/api/check",
            post(|| async {
                (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    [(axum::http::header::RETRY_AFTER, "7")],
                    Json(json!({"error": "gpu_busy"})),
                )
            }),
        );
        let base = serve(app).await;
        let error = client(&base, Some("licence"))
            .check("theorem t : True := by trivial")
            .await
            .expect_err("shed");
        assert!(matches!(error, Lean4Error::Busy { retry_after: 7 }));
    }

    #[tokio::test]
    async fn a_missing_retry_after_still_bounds_the_wait() {
        let app = Router::new().route(
            "/api/elaborate",
            post(|| async { axum::http::StatusCode::TOO_MANY_REQUESTS }),
        );
        let base = serve(app).await;
        let error = client(&base, Some("licence"))
            .elaborate("1 + 1 = 2")
            .await
            .expect_err("shed");
        assert!(matches!(
            error,
            Lean4Error::Busy {
                retry_after: RETRY_AFTER_DEFAULT
            }
        ));
    }

    #[tokio::test]
    async fn no_licence_is_a_named_refusal_before_any_request() {
        let error = client("http://127.0.0.1:9", None)
            .check("theorem t : True := by trivial")
            .await
            .expect_err("refused");
        assert!(matches!(error, Lean4Error::MissingLicence));
    }

    #[test]
    fn an_empty_endpoint_is_not_a_client() {
        let http = reqwest::Client::new();
        assert!(Lean4Client::new(Some(""), Some("t"), http.clone(), 60).is_none());
        assert!(Lean4Client::new(None, Some("t"), http, 60).is_none());
    }

    #[test]
    fn the_wait_is_clamped_to_the_contract() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "0.2".parse().unwrap());
        assert_eq!(retry_after(&headers), RETRY_AFTER_MIN);
        headers.insert(reqwest::header::RETRY_AFTER, "9999".parse().unwrap());
        assert_eq!(retry_after(&headers), RETRY_AFTER_MAX);
        headers.insert(
            reqwest::header::RETRY_AFTER,
            "not-a-number".parse().unwrap(),
        );
        assert_eq!(retry_after(&headers), RETRY_AFTER_DEFAULT);
    }
}
