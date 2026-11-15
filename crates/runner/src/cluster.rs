//! The Kubernetes API client: five REST calls over the workspace `reqwest`.
//!
//! This is the generalised form of `apps/server/src/sandbox.rs`'s client. The sandbox keeps its own
//! manifest builder and state interpretation; the transport, credential handling and fail-closed
//! configuration live here so a second dispatcher cannot drift from the first.

use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

use crate::{DEFAULT_PULL_SECRET, SA_DIR};

/// Why a cluster call did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    /// The namespace (or the in-cluster API address) is unset.
    #[error("the runner is not configured")]
    NotConfigured,
    /// The service-account credential could not be read.
    #[error("runner credential: {0}")]
    Credential(String),
    /// The API request did not reach the server.
    #[error("kubernetes API: {0}")]
    Transport(String),
    /// The API answered a non-success status.
    #[error("kubernetes API returned {status}: {body}")]
    Api { status: u16, body: String },
    /// The named object does not exist.
    #[error("{what} {name:?} does not exist")]
    NotFound { what: &'static str, name: String },
    /// The manifest cannot be built (size cap, unusable selector).
    #[error("{detail}")]
    Manifest { detail: String },
    /// The instruction is over the shared ConfigMap cap.
    #[error("instruction inputs are {size} bytes, over the {limit}-byte cap")]
    ConfigMapTooLarge { size: usize, limit: usize },
}

/// An optional node selector (`RUNNER_NODE_SELECTOR=key=value`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeSelector {
    pub key: String,
    pub value: String,
}

/// An optional toleration (`RUNNER_TOLERATION=key=value:Effect`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toleration {
    pub key: String,
    pub value: String,
    pub effect: String,
}

/// Everything the client needs. Built from env in a pod; built from a lookup in tests.
#[derive(Clone, Debug)]
pub struct ClusterConfig {
    /// The namespace Jobs, ConfigMaps and Secrets are created in.
    pub namespace: String,
    /// The API server base URL, e.g. `https://kubernetes.default.svc:443`.
    pub api_url: String,
    /// The service-account bearer token.
    pub token: String,
    /// The cluster CA in PEM, when the API uses a private CA.
    pub ca_pem: Option<Vec<u8>>,
    /// The image pull secret present in the runner namespace.
    pub pull_secret: String,
    /// The per-call HTTP timeout.
    pub timeout: Duration,
}

impl ClusterConfig {
    /// Read the runner configuration from the process environment.
    pub fn from_env() -> Result<Self, RunnerError> {
        Self::from_lookup(
            |key| std::env::var(key).ok(),
            |path| std::fs::read(path).ok(),
        )
    }

    /// Build from a lookup pair (env, file read) so the fail-closed path is testable.
    ///
    /// `RUNNER_NAMESPACE` is required. The API address defaults to the in-cluster
    /// `KUBERNETES_SERVICE_HOST`/`_PORT`; the token and CA default to the mounted service account.
    pub fn from_lookup(
        get: impl Fn(&str) -> Option<String>,
        read: impl Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<Self, RunnerError> {
        let namespace = get("RUNNER_NAMESPACE")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or(RunnerError::NotConfigured)?;

        let api_url = match get("RUNNER_API_URL")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            Some(url) => url,
            None => {
                let host = get("KUBERNETES_SERVICE_HOST").filter(|v| !v.trim().is_empty());
                let port = get("KUBERNETES_SERVICE_PORT")
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or_else(|| "443".to_owned());
                let host = host.ok_or_else(|| {
                    RunnerError::Credential(
                        "KUBERNETES_SERVICE_HOST is unset and RUNNER_API_URL is empty".into(),
                    )
                })?;
                format!("https://{}:{}", host.trim(), port.trim())
            }
        };

        let token = match get("RUNNER_TOKEN")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            Some(token) => token,
            None => read(&format!("{SA_DIR}/token"))
                .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    RunnerError::Credential(format!("{SA_DIR}/token is missing or empty"))
                })?,
        };

        let ca_pem = match get("RUNNER_CA_PATH").filter(|path| !path.trim().is_empty()) {
            Some(path) => read(path.trim()),
            None => read(&format!("{SA_DIR}/ca.crt")),
        };

        let pull_secret = get("RUNNER_PULL_SECRET")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_PULL_SECRET.to_owned());

        Ok(Self {
            namespace,
            api_url,
            token,
            ca_pem,
            pull_secret,
            timeout: Duration::from_secs(10),
        })
    }

    /// Build from parts, for a caller that already parsed its own environment (the sandbox).
    pub fn from_parts(
        namespace: impl Into<String>,
        api_url: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            api_url: api_url.into(),
            token: token.into(),
            ca_pem: None,
            pull_secret: DEFAULT_PULL_SECRET.to_owned(),
            timeout: Duration::from_secs(10),
        }
    }

    /// Attach a private CA, as the in-cluster mount supplies.
    pub fn with_ca(mut self, ca_pem: Option<Vec<u8>>) -> Self {
        self.ca_pem = ca_pem;
        self
    }
}

/// The observed state of a dispatched Job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobState {
    /// The Job is running (or not yet observed).
    Running,
    /// The Job's pod exited 0 — a result should be available.
    Succeeded,
    /// The Job failed (pod deadline, image pull, exit code).
    Failed {
        /// A human-readable reason from the Job's conditions.
        reason: String,
    },
}

/// A configured Kubernetes API client.
#[derive(Clone, Debug)]
pub struct Cluster {
    config: ClusterConfig,
    client: Client,
}

impl Cluster {
    /// Build a client from an explicit configuration.
    pub fn new(config: ClusterConfig) -> Result<Self, RunnerError> {
        let mut builder = Client::builder().timeout(config.timeout);
        if let Some(pem) = config.ca_pem.as_deref() {
            let certificate = reqwest::Certificate::from_pem(pem)
                .map_err(|err| RunnerError::Credential(format!("CA PEM: {err}")))?;
            builder = builder.add_root_certificate(certificate);
        }
        let client = builder
            .build()
            .map_err(|err| RunnerError::Transport(err.to_string()))?;
        Ok(Self { config, client })
    }

    /// Build from the process environment; [`RunnerError::NotConfigured`] is the fail-closed path.
    pub fn from_env() -> Result<Self, RunnerError> {
        Self::new(ClusterConfig::from_env()?)
    }

    /// The namespace objects are created in.
    pub fn namespace(&self) -> &str {
        &self.config.namespace
    }

    /// The pull secret a Job spec should reference.
    pub fn pull_secret(&self) -> &str {
        &self.config.pull_secret
    }

    /// Create an object. A `409 AlreadyExists` is a **reattach**, not a failure: every object this
    /// crate creates has a deterministic name.
    pub async fn create(&self, path: &str, body: &Value) -> Result<(), RunnerError> {
        let response = self
            .client
            .post(format!("{}{path}", self.config.api_url))
            .bearer_auth(&self.config.token)
            .json(body)
            .send()
            .await
            .map_err(|err| RunnerError::Transport(err.to_string()))?;
        match response.status() {
            status if status.is_success() => Ok(()),
            StatusCode::CONFLICT => Ok(()),
            StatusCode::NOT_FOUND => Err(RunnerError::NotFound {
                what: "object",
                name: path.to_owned(),
            }),
            status => Err(RunnerError::Api {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            }),
        }
    }

    /// Read one object's JSON body.
    pub async fn get(
        &self,
        path: &str,
        what: &'static str,
        name: &str,
    ) -> Result<Value, RunnerError> {
        let response = self
            .client
            .get(format!("{}{path}", self.config.api_url))
            .bearer_auth(&self.config.token)
            .send()
            .await
            .map_err(|err| RunnerError::Transport(err.to_string()))?;
        match response.status() {
            status if status.is_success() => response
                .json::<Value>()
                .await
                .map_err(|err| RunnerError::Transport(err.to_string())),
            StatusCode::NOT_FOUND => Err(RunnerError::NotFound {
                what,
                name: name.to_owned(),
            }),
            status => Err(RunnerError::Api {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            }),
        }
    }

    /// Read one object's text body (a pod log).
    pub async fn get_text(
        &self,
        path: &str,
        what: &'static str,
        name: &str,
    ) -> Result<String, RunnerError> {
        let response = self
            .client
            .get(format!("{}{path}", self.config.api_url))
            .bearer_auth(&self.config.token)
            .send()
            .await
            .map_err(|err| RunnerError::Transport(err.to_string()))?;
        match response.status() {
            status if status.is_success() => response
                .text()
                .await
                .map_err(|err| RunnerError::Transport(err.to_string())),
            StatusCode::NOT_FOUND => Err(RunnerError::NotFound {
                what,
                name: name.to_owned(),
            }),
            status => Err(RunnerError::Api {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            }),
        }
    }

    /// Delete an object with background propagation. A `404` is fine: the reaper may run after the
    /// Job's TTL cleanup.
    pub async fn delete(
        &self,
        path: &str,
        what: &'static str,
        name: &str,
    ) -> Result<(), RunnerError> {
        let response = self
            .client
            .delete(format!("{}{path}", self.config.api_url))
            .bearer_auth(&self.config.token)
            .send()
            .await
            .map_err(|err| RunnerError::Transport(err.to_string()))?;
        match response.status() {
            status if status.is_success() => Ok(()),
            StatusCode::NOT_FOUND => Err(RunnerError::NotFound {
                what,
                name: name.to_owned(),
            }),
            status => Err(RunnerError::Api {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            }),
        }
    }

    /// Create a ConfigMap in the configured namespace.
    pub async fn create_config_map(&self, manifest: &Value, name: &str) -> Result<(), RunnerError> {
        let path = format!("/api/v1/namespaces/{}/configmaps", self.config.namespace);
        self.create(&path, manifest)
            .await
            .map_err(|err| prefix(err, name))
    }

    /// Create a Secret in the configured namespace.
    pub async fn create_secret(&self, manifest: &Value, name: &str) -> Result<(), RunnerError> {
        let path = format!("/api/v1/namespaces/{}/secrets", self.config.namespace);
        self.create(&path, manifest)
            .await
            .map_err(|err| prefix(err, name))
    }

    /// Create a Job in the configured namespace.
    pub async fn create_job(&self, manifest: &Value, name: &str) -> Result<(), RunnerError> {
        let path = format!("/apis/batch/v1/namespaces/{}/jobs", self.config.namespace);
        self.create(&path, manifest)
            .await
            .map_err(|err| prefix(err, name))
    }

    /// Read one Job's state.
    pub async fn job_state(&self, name: &str) -> Result<JobState, RunnerError> {
        let path = format!(
            "/apis/batch/v1/namespaces/{}/jobs/{name}",
            self.config.namespace
        );
        let body = self.get(&path, "job", name).await?;
        Ok(interpret_state(&body))
    }

    /// Delete a Job (background propagation).
    pub async fn delete_job(&self, name: &str) -> Result<(), RunnerError> {
        let path = format!(
            "/apis/batch/v1/namespaces/{}/jobs/{name}?propagationPolicy=Background",
            self.config.namespace
        );
        self.delete(&path, "job", name).await
    }

    /// Delete a ConfigMap (best effort; a missing one is not an error at reap time).
    pub async fn delete_config_map(&self, name: &str) -> Result<(), RunnerError> {
        let path = format!(
            "/api/v1/namespaces/{}/configmaps/{name}",
            self.config.namespace
        );
        self.delete(&path, "configmap", name).await
    }

    /// Delete a Secret (best effort).
    pub async fn delete_secret(&self, name: &str) -> Result<(), RunnerError> {
        let path = format!(
            "/api/v1/namespaces/{}/secrets/{name}",
            self.config.namespace
        );
        self.delete(&path, "secret", name).await
    }

    /// The pod logs of a Job, when the name is unambiguous (one pod per Job, `backoffLimit: 0`).
    pub async fn pod_logs(&self, job: &str) -> Result<String, RunnerError> {
        let selector = format!("job-name={job}");
        let path = format!(
            "/api/v1/namespaces/{}/pods?labelSelector={selector}",
            self.config.namespace
        );
        let body = self.get(&path, "pod list", job).await?;
        let pod = body["items"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item["metadata"]["name"].as_str())
            .ok_or_else(|| RunnerError::NotFound {
                what: "pod",
                name: job.to_owned(),
            })?;
        let path = format!(
            "/api/v1/namespaces/{}/pods/{pod}/log",
            self.config.namespace
        );
        self.get_text(&path, "pod", pod).await
    }
}

/// Keep the object name on an error raised before the body is parsed.
fn prefix(err: RunnerError, name: &str) -> RunnerError {
    match err {
        RunnerError::Api { status, mut body } => {
            body = format!("{name}: {body}");
            RunnerError::Api { status, body }
        }
        other => other,
    }
}

/// The Job status → [`JobState`] projection, split out so it is unit-tested without a cluster.
pub fn interpret_state(job: &Value) -> JobState {
    let status = &job["status"];
    if status["succeeded"].as_i64().unwrap_or(0) > 0 {
        return JobState::Succeeded;
    }
    if status["failed"].as_i64().unwrap_or(0) > 0 {
        let reason = status["conditions"]
            .as_array()
            .and_then(|conditions| {
                conditions.iter().find_map(|condition| {
                    (condition["type"] == "Failed")
                        .then(|| condition["reason"].as_str().unwrap_or("failed").to_owned())
                })
            })
            .unwrap_or_else(|| "failed".to_owned());
        return JobState::Failed { reason };
    }
    JobState::Running
}

/// A minimal in-process object used by the unit tests to prove the fail-closed configuration.
pub(crate) fn default_labels(job_id: uuid::Uuid, attempt: i32, kind: &str) -> Value {
    json!({
        "app.kubernetes.io/name": "conveyor-runner",
        "conveyor/job": job_id.to_string(),
        "conveyor/attempt": attempt.to_string(),
        "conveyor/kind": kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key| owned.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn a_missing_namespace_fails_closed() {
        let config = ClusterConfig::from_lookup(lookup(&[]), |_| None);
        assert!(matches!(config, Err(RunnerError::NotConfigured)));
    }

    #[test]
    fn an_explicit_api_url_and_token_are_used() {
        let config = ClusterConfig::from_lookup(
            lookup(&[
                ("RUNNER_NAMESPACE", "conveyor-runners"),
                ("RUNNER_API_URL", "https://kube.example:443"),
                ("RUNNER_TOKEN", "token-1"),
            ]),
            |_| None,
        )
        .expect("configured");
        assert_eq!(config.namespace, "conveyor-runners");
        assert_eq!(config.api_url, "https://kube.example:443");
        assert_eq!(config.token, "token-1");
        assert_eq!(config.pull_secret, DEFAULT_PULL_SECRET);
    }

    #[test]
    fn the_in_cluster_address_and_token_file_are_the_defaults() {
        let config = ClusterConfig::from_lookup(
            lookup(&[
                ("RUNNER_NAMESPACE", "conveyor-runners"),
                ("KUBERNETES_SERVICE_HOST", "10.43.0.1"),
                ("KUBERNETES_SERVICE_PORT", "443"),
            ]),
            |path| (path == format!("{SA_DIR}/token")).then(|| b"file-token".to_vec()),
        )
        .expect("configured");
        assert_eq!(config.api_url, "https://10.43.0.1:443");
        assert_eq!(config.token, "file-token");
    }

    #[test]
    fn a_succeeded_job_is_read_from_status() {
        let state = interpret_state(&json!({"status": {"succeeded": 1}}));
        assert_eq!(state, JobState::Succeeded);
    }

    #[test]
    fn a_failed_job_carries_its_condition_reason() {
        let state = interpret_state(&json!({
            "status": {
                "failed": 1,
                "conditions": [{"type": "Failed", "reason": "DeadlineExceeded"}]
            }
        }));
        assert_eq!(
            state,
            JobState::Failed {
                reason: "DeadlineExceeded".to_owned()
            }
        );
    }

    #[test]
    fn a_job_with_no_terminal_condition_is_running() {
        let state = interpret_state(&json!({"status": {"active": 1}}));
        assert_eq!(state, JobState::Running);
    }
}
