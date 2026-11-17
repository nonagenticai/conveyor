#![forbid(unsafe_code)]
//! `conveyor-ops` — the cluster operations client (T7, SPEC_V2 §10.4).
//!
//! One transport for the `ops.*` read family: the in-cluster Kubernetes API, reached with the
//! mounted service-account credential. It is a deliberately small surface — a `GET` that returns
//! the API's own objects unmodified — so the action layer can name a closed operation
//! (`ops.pods`, `ops.logs`, …) without ever accepting a free-form URL.
//!
//! **Fail closed, visibly.** No token, no API address ⇒ [`OpsError::Unconfigured`] and no request
//! is made. A missing credential is a `503`, never a guessed default, exactly as the sandbox and
//! runner clients behave (`crates/runner/src/cluster.rs`).
//!
//! **No `kube` crate.** `cluster.rs` and `apps/server/src/sandbox.rs` chose hand-rolled REST over
//! the workspace client; this crate continues that choice so the estate has one HTTP stack and no
//! second authorization model.
//!
//! **Where the credential lives.** Only the `conveyor-worker` pod template automounts a
//! service-account token (`clusters/strato/services/conveyor/serviceaccount.yaml` states ⛔ that
//! the serve container must not). A process without the mounted token answers `Unconfigured` —
//! by design, not by accident.

use std::time::Duration;

use reqwest::Client as Http;
use serde_json::Value;

pub mod ci;

/// The in-cluster service-account directory — the same path `runner` and the sandbox read.
pub const SA_DIR: &str = "/var/run/secrets/kubernetes.io/serviceaccount";

/// The per-call deadline. Reads are small; a hung API must not hold an action open.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// How much of an error body is kept, so a 500's HTML does not become the action's detail.
const BODY_CAP: usize = 2_000;

/// Why an operations call did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    /// No credential or no API address: the client refuses before any request.
    #[error("the ops client is not configured")]
    Unconfigured,
    /// The API address or CA cannot be used.
    #[error("ops configuration: {0}")]
    Config(String),
    /// The request did not reach the API, or the response did not parse.
    #[error("kubernetes API: {0}")]
    Transport(String),
    /// The API answered a non-success status. Surfaced as-is (a `403` stays a `403`).
    #[error("kubernetes API returned {status}: {body}")]
    Api { status: u16, body: String },
}

impl OpsError {
    /// The upstream status, when the failure was the API's own answer.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(*status),
            _ => None,
        }
    }
}

/// Everything the client needs, built from the environment in a pod and from a lookup in tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpsConfig {
    /// The API server base URL, e.g. `https://kubernetes.default.svc:443`.
    pub api_url: String,
    /// The service-account bearer token.
    pub token: String,
    /// The cluster CA in PEM, when the API uses a private CA.
    pub ca_pem: Option<Vec<u8>>,
    /// The per-call HTTP timeout.
    pub timeout: Duration,
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

impl OpsConfig {
    /// Read the configuration from the process environment.
    pub fn from_env() -> Result<Self, OpsError> {
        Self::from_lookup(
            |key| std::env::var(key).ok(),
            |path| std::fs::read(path).ok(),
        )
    }

    /// Build from a lookup pair so the fail-closed path is testable without a cluster.
    ///
    /// `OPS_API_URL` overrides the in-cluster address; it defaults to
    /// `https://$KUBERNETES_SERVICE_HOST:$KUBERNETES_SERVICE_PORT`. The token comes from
    /// `OPS_TOKEN` or the mounted service account. **Either being absent is `Unconfigured`.**
    pub fn from_lookup(
        get: impl Fn(&str) -> Option<String>,
        read: impl Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<Self, OpsError> {
        let api_url = match trimmed(get("OPS_API_URL")) {
            Some(url) => url,
            None => {
                let host = trimmed(get("KUBERNETES_SERVICE_HOST")).ok_or(OpsError::Unconfigured)?;
                let port =
                    trimmed(get("KUBERNETES_SERVICE_PORT")).unwrap_or_else(|| "443".to_owned());
                format!("https://{host}:{port}")
            }
        };

        let token = match trimmed(get("OPS_TOKEN")) {
            Some(token) => token,
            None => {
                let path = format!("{SA_DIR}/token");
                let bytes = read(&path).ok_or(OpsError::Unconfigured)?;
                trimmed(Some(String::from_utf8_lossy(&bytes).into_owned()))
                    .ok_or(OpsError::Unconfigured)?
            }
        };

        let ca_pem = match trimmed(get("OPS_CA_PATH")) {
            Some(path) => read(&path),
            None => read(&format!("{SA_DIR}/ca.crt")),
        };

        Ok(Self {
            api_url,
            token,
            ca_pem,
            timeout: DEFAULT_TIMEOUT,
        })
    }

    /// Build from explicit parts, for a caller that already parsed its environment or a test.
    pub fn from_parts(api_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            api_url: api_url.into(),
            token: token.into(),
            ca_pem: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

/// The read-only Kubernetes client.
#[derive(Debug, Clone)]
pub struct Client {
    http: Http,
    api_url: String,
    token: String,
}

impl Client {
    /// Build the HTTP client, installing the cluster CA when one was read.
    pub fn new(config: OpsConfig) -> Result<Self, OpsError> {
        let mut builder = Http::builder()
            .timeout(config.timeout)
            .user_agent("conveyor-ops/1");
        if let Some(pem) = &config.ca_pem {
            let certificate = reqwest::Certificate::from_pem(pem)
                .map_err(|error| OpsError::Config(format!("ca: {error}")))?;
            builder = builder.add_root_certificate(certificate);
        }
        let http = builder
            .build()
            .map_err(|error| OpsError::Config(error.to_string()))?;
        Ok(Self {
            http,
            api_url: config.api_url.trim_end_matches('/').to_owned(),
            token: config.token,
        })
    }

    /// Build from the process environment (the in-pod path).
    pub fn from_env() -> Result<Self, OpsError> {
        Self::new(OpsConfig::from_env()?)
    }

    /// The API base URL this client was built with.
    pub fn base_url(&self) -> &str {
        &self.api_url
    }

    /// `GET path` and parse the body as JSON. The objects are returned exactly as sent.
    pub async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Value, OpsError> {
        let (status, body) = self.get(path, query).await?;
        if !status.is_success() {
            return Err(OpsError::Api {
                status: status.as_u16(),
                body: capped(&body),
            });
        }
        serde_json::from_str(&body)
            .map_err(|error| OpsError::Transport(format!("the API answered non-JSON: {error}")))
    }

    /// `GET path` and return the body as text (the pod-log route answers `text/plain`).
    pub async fn get_text(&self, path: &str, query: &[(&str, String)]) -> Result<String, OpsError> {
        let (status, body) = self.get(path, query).await?;
        if !status.is_success() {
            return Err(OpsError::Api {
                status: status.as_u16(),
                body: capped(&body),
            });
        }
        Ok(body)
    }

    /// `PATCH path` with an RFC 7386 merge-patch body, parsed as JSON.
    ///
    /// The write half of the family. `dryRun=All` rides in `query`, so a dry run and a real
    /// write take the SAME code path and differ only in the query string — the property the
    /// `ops.*` write tests rely on.
    pub async fn patch_json(
        &self,
        path: &str,
        query: &[(&str, String)],
        body: &Value,
    ) -> Result<Value, OpsError> {
        let (status, text) = self.patch(path, query, body).await?;
        if !status.is_success() {
            return Err(OpsError::Api {
                status: status.as_u16(),
                body: capped(&text),
            });
        }
        serde_json::from_str(&text)
            .map_err(|error| OpsError::Transport(format!("the API answered non-JSON: {error}")))
    }

    /// The request URL for one absolute API path and its query pairs.
    fn url(&self, path: &str, query: &[(&str, String)]) -> Result<reqwest::Url, OpsError> {
        if !path.starts_with('/') {
            return Err(OpsError::Config(format!(
                "{path:?} is not an absolute API path"
            )));
        }
        let mut url = reqwest::Url::parse(&format!("{}{path}", self.api_url))
            .map_err(|error| OpsError::Config(format!("{path:?}: {error}")))?;
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
        }
        Ok(url)
    }

    async fn patch(
        &self,
        path: &str,
        query: &[(&str, String)],
        body: &Value,
    ) -> Result<(reqwest::StatusCode, String), OpsError> {
        let url = self.url(path, query)?;
        let payload = serde_json::to_vec(body)
            .map_err(|error| OpsError::Config(format!("request body: {error}")))?;
        let response = self
            .http
            .patch(url)
            .bearer_auth(&self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/merge-patch+json",
            )
            .body(payload)
            .send()
            .await
            .map_err(|error| OpsError::Transport(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| OpsError::Transport(error.to_string()))?;
        Ok((status, body))
    }

    async fn get(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<(reqwest::StatusCode, String), OpsError> {
        let url = self.url(path, query)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| OpsError::Transport(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| OpsError::Transport(error.to_string()))?;
        Ok((status, body))
    }
}

fn capped(body: &str) -> String {
    if body.len() <= BODY_CAP {
        body.to_owned()
    } else {
        format!("{}…", &body[..BODY_CAP])
    }
}
