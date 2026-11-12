#![forbid(unsafe_code)]
//! The Kubernetes Job sandbox — the only place untrusted patches run (S17 §6 step 10,
//! CONTRACT §19 "Sandbox isolation").
//!
//! One `batch/v1` Job per `(run, instance)`: a ConfigMap carrying `patch.diff`, `eval.sh` and
//! `run.sh`; the instance image by digest; **no service-account token**; CPU/memory/ephemeral
//! caps; a pod deadline. The conveyor never executes test code in its own pod.
//!
//! **Client choice (recorded deviation from S17 §6's "kube client").** This talks to the
//! Kubernetes API over the workspace `reqwest` instead of the generated `kube` client: the
//! surface is five REST calls, the credential is already a service-account file, and `kube` +
//! `k8s-openapi` would add a new dependency tree to a `Cargo.lock` three lanes share this week.
//! The wire behaviour is identical, and the manifest builder is unit-tested without a cluster.
//!
//! **Fail-closed.** [`SandboxConfig::from_lookup`] returns [`SandboxError::NotConfigured`] when
//! `SANDBOX_NAMESPACE` is unset — the action layer maps that to `503
//! benchmark_sandbox_not_configured` before any case starts, never to a silently empty patch.

use std::time::Duration;

use conveyor_domain::benchmark::{APPLY_PATCH_FAIL, APPLY_PATCH_PASS, EVAL_MARKER, TIMEOUT_MARKER};
use reqwest::{Client, StatusCode};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// swebench's own default; a timeout is `eval_timeout`, never `unresolved`.
pub const EVAL_TIMEOUT_S: u64 = 1800;
/// Headroom for a cold multi-GB image pull (the old harness's `POD_DEADLINE_S`).
pub const POD_DEADLINE_S: u64 = EVAL_TIMEOUT_S + 1800;
/// The ConfigMap inputs share Kubernetes' ~1 MiB object cap; refuse before create.
pub const CONFIGMAP_LIMIT: usize = 1_000_000;
/// The estate's registry pull secret, in every consuming namespace.
pub const DEFAULT_PULL_SECRET: &str = "registry-pull-secret";
/// The in-cluster service-account directory.
pub const SA_DIR: &str = "/var/run/secrets/kubernetes.io/serviceaccount";

/// swebench 5.0.2's `GIT_APPLY_CMDS`, read from the pinned sdist (`run_evaluation.py:54`).
pub const GIT_APPLY_CMDS: [&str; 4] = [
    "git apply --verbose",
    "git apply --verbose --3way",
    "git apply --verbose --reject",
    "patch --batch --forward --fuzz=5 -p1 -i",
];

/// Why a sandbox call did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// `SANDBOX_NAMESPACE` (or the in-cluster API address) is unset.
    #[error("the sandbox is not configured")]
    NotConfigured,
    /// The service-account credential could not be read.
    #[error("sandbox credential: {0}")]
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
    /// The inputs are over the shared ConfigMap cap.
    #[error("configmap inputs are {size} bytes, over the {CONFIGMAP_LIMIT}-byte cap")]
    ConfigMapTooLarge { size: usize },
    /// The Job has no pod (yet, or ever).
    #[error("job {0:?} has no pod")]
    NoPod(String),
}

/// An optional node selector (`SANDBOX_NODE_SELECTOR=key=value`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeSelector {
    /// The label key.
    pub key: String,
    /// The label value.
    pub value: String,
}

/// An optional toleration (`SANDBOX_TOLERATION=key=value:Effect`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toleration {
    /// The taint key.
    pub key: String,
    /// The taint value.
    pub value: String,
    /// `NoSchedule | PreferNoSchedule | NoExecute`.
    pub effect: String,
}

/// The sandbox client's configuration. Built from env in a pod; built from a lookup in tests.
#[derive(Clone, Debug)]
pub struct SandboxConfig {
    /// The namespace the Job and ConfigMap are created in.
    pub namespace: String,
    /// The API server base URL, e.g. `https://kubernetes.default.svc:443`.
    pub api_url: String,
    /// The service-account bearer token.
    pub token: String,
    /// The cluster CA in PEM, when the API uses a private CA.
    pub ca_pem: Option<Vec<u8>>,
    /// The designated grading node, when deployment config names one.
    pub node_selector: Option<NodeSelector>,
    /// The toleration for that node's taint, when it carries one.
    pub toleration: Option<Toleration>,
    /// The image pull secret in the sandbox namespace.
    pub pull_secret: String,
    /// The per-call HTTP timeout.
    pub timeout: Duration,
}

impl SandboxConfig {
    /// Read the sandbox configuration from the process environment.
    pub fn from_env() -> Result<Self, SandboxError> {
        Self::from_lookup(
            |key| std::env::var(key).ok(),
            |path| std::fs::read(path).ok(),
        )
    }

    /// Build from a lookup pair (env, file read) so the fail-closed path is testable.
    ///
    /// `SANDBOX_NAMESPACE` is required. The API address defaults to the in-cluster
    /// `KUBERNETES_SERVICE_HOST`/`_PORT`; the token and CA default to the mounted service
    /// account. `SANDBOX_NODE_SELECTOR` and `SANDBOX_TOLERATION` are optional deployment config —
    /// the node is **never** hardcoded (SPEC R12; the old harness's `worker1` is gone).
    pub fn from_lookup(
        get: impl Fn(&str) -> Option<String>,
        read: impl Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<Self, SandboxError> {
        let namespace = get("SANDBOX_NAMESPACE")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or(SandboxError::NotConfigured)?;

        let api_url = match get("SANDBOX_API_URL")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            Some(url) => url,
            None => {
                let host = get("KUBERNETES_SERVICE_HOST").filter(|value| !value.trim().is_empty());
                let port = get("KUBERNETES_SERVICE_PORT")
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "443".to_owned());
                let host = host.ok_or_else(|| {
                    SandboxError::Credential(
                        "KUBERNETES_SERVICE_HOST is unset and SANDBOX_API_URL is empty".into(),
                    )
                })?;
                format!("https://{}:{}", host.trim(), port.trim())
            }
        };

        let token = match get("SANDBOX_TOKEN")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            Some(token) => token,
            None => read(&format!("{SA_DIR}/token"))
                .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    SandboxError::Credential(format!("{SA_DIR}/token is missing or empty"))
                })?,
        };

        let ca_pem = match get("SANDBOX_CA_PATH").filter(|path| !path.trim().is_empty()) {
            Some(path) => read(path.trim()),
            None => read(&format!("{SA_DIR}/ca.crt")),
        };

        let node_selector = match get("SANDBOX_NODE_SELECTOR").map(|v| v.trim().to_owned()) {
            Some(raw) if !raw.is_empty() => {
                let (key, value) = raw.split_once('=').ok_or_else(|| SandboxError::Manifest {
                    detail: format!("SANDBOX_NODE_SELECTOR {raw:?} is not key=value"),
                })?;
                Some(NodeSelector {
                    key: key.trim().to_owned(),
                    value: value.trim().to_owned(),
                })
            }
            _ => None,
        };

        let toleration = match get("SANDBOX_TOLERATION").map(|v| v.trim().to_owned()) {
            Some(raw) if !raw.is_empty() => {
                let (pair, effect) = raw.split_once(':').ok_or_else(|| SandboxError::Manifest {
                    detail: format!("SANDBOX_TOLERATION {raw:?} is not key=value:Effect"),
                })?;
                let (key, value) = pair.split_once('=').ok_or_else(|| SandboxError::Manifest {
                    detail: format!("SANDBOX_TOLERATION {raw:?} is not key=value:Effect"),
                })?;
                Some(Toleration {
                    key: key.trim().to_owned(),
                    value: value.trim().to_owned(),
                    effect: effect.trim().to_owned(),
                })
            }
            _ => None,
        };

        let pull_secret = get("SANDBOX_PULL_SECRET")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_PULL_SECRET.to_owned());

        Ok(Self {
            namespace,
            api_url,
            token,
            ca_pem,
            node_selector,
            toleration,
            pull_secret,
            timeout: Duration::from_secs(10),
        })
    }
}

/// The observed state of a sandbox Job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobState {
    /// The Job is running (or not yet observed).
    Running,
    /// The Job's pod exited 0 — the log is ready to grade.
    Succeeded,
    /// The Job failed (pod deadline, image pull, exit code).
    Failed {
        /// A human-readable reason from the Job's conditions.
        reason: String,
    },
}

/// A configured Kubernetes API client for the benchmark sandbox.
#[derive(Clone, Debug)]
pub struct Sandbox {
    config: SandboxConfig,
    client: Client,
}

impl Sandbox {
    /// Build a client from an explicit configuration.
    pub fn new(config: SandboxConfig) -> Result<Self, SandboxError> {
        let mut builder = Client::builder().timeout(config.timeout);
        if let Some(pem) = config.ca_pem.as_deref() {
            let certificate = reqwest::Certificate::from_pem(pem)
                .map_err(|err| SandboxError::Credential(format!("CA PEM: {err}")))?;
            builder = builder.add_root_certificate(certificate);
        }
        let client = builder
            .build()
            .map_err(|err| SandboxError::Transport(err.to_string()))?;
        Ok(Self { config, client })
    }

    /// Build from the process environment; [`SandboxError::NotConfigured`] is the fail-closed
    /// path the action layer turns into `503 benchmark_sandbox_not_configured`.
    pub fn from_env() -> Result<Self, SandboxError> {
        Self::new(SandboxConfig::from_env()?)
    }

    /// The namespace Jobs are created in.
    pub fn namespace(&self) -> &str {
        &self.config.namespace
    }

    /// The deterministic Job name for `(run, instance)`; a repeated create is a reattach.
    ///
    /// Ported from `grader.py::job_name` with one correction: the run marker is the first 12 hex
    /// digits of SHA-256 over the **full** run id, not the first 8 characters of the id itself.
    /// A UUIDv7 begins with a millisecond timestamp, so two runs created in the same millisecond
    /// shared the first 8 characters — a collision the identity must not have. Long instance ids
    /// are truncated to 63 with an 8-hex-digit hash suffix, exactly as the harness did.
    pub fn job_name(run_id: Uuid, instance_id: &str) -> String {
        let slug: String = instance_id
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let slug = slug.trim_matches('-');
        let run_digest = Sha256::digest(run_id.as_bytes());
        let run12: String = run_digest
            .iter()
            .take(6)
            .map(|b| format!("{b:02x}"))
            .collect();
        let name = format!("swb-{run12}-{slug}");
        if name.len() <= 63 {
            return name;
        }
        let digest = Sha256::digest(name.as_bytes());
        let suffix: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
        let head = name[..54].trim_end_matches('-');
        format!("{head}-{suffix}")
    }

    /// The ConfigMap and Job for one instance.
    ///
    /// The ConfigMap carries the three inputs (`patch.diff`, `eval.sh`, `run.sh`) and is
    /// refused over [`CONFIGMAP_LIMIT`]. The Job runs the runner script as a non-root,
    /// token-less pod with CPU/memory/ephemeral caps and the instance image by digest.
    pub fn manifests(
        &self,
        run_id: Uuid,
        instance_id: &str,
        image: &str,
        eval_script: &str,
        model_patch: &str,
    ) -> Result<(Value, Value), SandboxError> {
        let name = Self::job_name(run_id, instance_id);
        let labels = json!({
            "app.kubernetes.io/name": "conveyor-bench-grader",
            "conveyor/run": run_id.to_string(),
        });
        let run_script = runner_script();
        let data = json!({
            "patch.diff": model_patch,
            "eval.sh": eval_script,
            "run.sh": run_script,
        });
        let size = model_patch.len() + eval_script.len() + run_script.len();
        if size > CONFIGMAP_LIMIT {
            return Err(SandboxError::ConfigMapTooLarge { size });
        }

        let mut pod_spec = json!({
            "restartPolicy": "Never",
            "automountServiceAccountToken": false,
            "enableServiceLinks": false,
            "imagePullSecrets": [{ "name": self.config.pull_secret }],
            "containers": [{
                "name": "grader",
                "image": image,
                "command": ["/bin/bash", "/bench/run.sh"],
                "workingDir": "/testbed",
                "resources": {
                    "requests": { "cpu": "1", "memory": "2Gi", "ephemeral-storage": "2Gi" },
                    "limits": { "cpu": "2", "memory": "6Gi", "ephemeral-storage": "10Gi" }
                },
                "securityContext": { "allowPrivilegeEscalation": false },
                "volumeMounts": [{ "name": "bench", "mountPath": "/bench", "readOnly": true }]
            }],
            "volumes": [{
                "name": "bench",
                "configMap": { "name": name, "defaultMode": 0o555 }
            }]
        });
        if let Some(selector) = &self.config.node_selector {
            let mut node_selector = Map::new();
            node_selector.insert(selector.key.clone(), Value::String(selector.value.clone()));
            pod_spec["nodeSelector"] = Value::Object(node_selector);
        }
        if let Some(toleration) = &self.config.toleration {
            pod_spec["tolerations"] = json!([{
                "key": toleration.key,
                "operator": "Equal",
                "value": toleration.value,
                "effect": toleration.effect,
            }]);
        }

        let configmap = json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": { "name": name, "namespace": self.config.namespace, "labels": labels.clone() },
            "data": data,
        });
        let job = json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {
                "name": name,
                "namespace": self.config.namespace,
                "labels": labels,
                "annotations": { "conveyor/instance-id": instance_id },
            },
            "spec": {
                "backoffLimit": 0,
                "activeDeadlineSeconds": POD_DEADLINE_S,
                "ttlSecondsAfterFinished": 3600,
                "template": {
                    "metadata": { "labels": labels },
                    "spec": pod_spec,
                },
            },
        });
        Ok((configmap, job))
    }

    /// Create the ConfigMap and Job, returning the Job name.
    ///
    /// A `409 AlreadyExists` on either object is a **reattach** — the deterministic name means a
    /// repeated tick finds the Job it started, not a second one.
    pub async fn start(
        &self,
        run_id: Uuid,
        instance_id: &str,
        image: &str,
        eval_script: &str,
        model_patch: &str,
    ) -> Result<String, SandboxError> {
        let (configmap, job) =
            self.manifests(run_id, instance_id, image, eval_script, model_patch)?;
        let name = Self::job_name(run_id, instance_id);
        self.create(
            &format!("/api/v1/namespaces/{}/configmaps", self.config.namespace),
            &configmap,
        )
        .await?;
        self.create(
            &format!("/apis/batch/v1/namespaces/{}/jobs", self.config.namespace),
            &job,
        )
        .await?;
        Ok(name)
    }

    /// Read one Job's state.
    pub async fn state(&self, name: &str) -> Result<JobState, SandboxError> {
        let path = format!(
            "/apis/batch/v1/namespaces/{}/jobs/{name}",
            self.config.namespace
        );
        let body = self.get(&path, "job", name).await?;
        let status = &body["status"];
        if status["succeeded"].as_i64().unwrap_or(0) > 0 {
            return Ok(JobState::Succeeded);
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
            return Ok(JobState::Failed { reason });
        }
        Ok(JobState::Running)
    }

    /// The pod log of a finished Job.
    pub async fn logs(&self, name: &str) -> Result<String, SandboxError> {
        let selector = format!("job-name={name}");
        let pod = self
            .pod_name(name, &selector)
            .await
            .map_err(|err| match err {
                SandboxError::NotFound { .. } | SandboxError::NoPod(_) => {
                    SandboxError::NoPod(name.to_owned())
                }
                other => other,
            })?;
        let path = format!(
            "/api/v1/namespaces/{}/pods/{pod}/log",
            self.config.namespace
        );
        self.get_text(&path, "pod", &pod).await
    }

    /// Delete the Job (and its pod, background propagation) and the ConfigMap. A `404` is fine:
    /// the reaper may run after the Job's TTL cleanup.
    pub async fn delete(&self, name: &str) -> Result<(), SandboxError> {
        let job = format!(
            "/apis/batch/v1/namespaces/{}/jobs/{name}?propagationPolicy=Background",
            self.config.namespace
        );
        self.delete_path(&job).await?;
        let configmap = format!(
            "/api/v1/namespaces/{}/configmaps/{name}",
            self.config.namespace
        );
        self.delete_path(&configmap).await
    }

    async fn pod_name(&self, job: &str, selector: &str) -> Result<String, SandboxError> {
        // The Job's name is `[a-z0-9-]`, so `labelSelector=job-name=<name>` needs no escaping.
        let path = format!(
            "/api/v1/namespaces/{}/pods?labelSelector={selector}",
            self.config.namespace
        );
        let body = self.get(&path, "pod for job", job).await?;
        body["items"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(|pod| pod["metadata"]["name"].as_str())
            .map(|name| name.to_owned())
            .ok_or_else(|| SandboxError::NoPod(job.to_owned()))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.api_url.trim_end_matches('/'), path)
    }

    async fn create(&self, path: &str, body: &Value) -> Result<(), SandboxError> {
        let response = self
            .client
            .post(self.url(path))
            .bearer_auth(&self.config.token)
            .json(body)
            .send()
            .await
            .map_err(|err| SandboxError::Transport(err.to_string()))?;
        let status = response.status();
        if status.is_success() || status == StatusCode::CONFLICT {
            return Ok(());
        }
        Err(SandboxError::Api {
            status: status.as_u16(),
            body: capped(response.text().await.unwrap_or_default()),
        })
    }

    async fn get(&self, path: &str, what: &'static str, name: &str) -> Result<Value, SandboxError> {
        let response = self
            .client
            .get(self.url(path))
            .bearer_auth(&self.config.token)
            .send()
            .await
            .map_err(|err| SandboxError::Transport(err.to_string()))?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(SandboxError::NotFound {
                what,
                name: name.to_owned(),
            });
        }
        if !status.is_success() {
            return Err(SandboxError::Api {
                status: status.as_u16(),
                body: capped(response.text().await.unwrap_or_default()),
            });
        }
        response
            .json()
            .await
            .map_err(|err| SandboxError::Transport(err.to_string()))
    }

    async fn get_text(
        &self,
        path: &str,
        what: &'static str,
        name: &str,
    ) -> Result<String, SandboxError> {
        let response = self
            .client
            .get(self.url(path))
            .bearer_auth(&self.config.token)
            .send()
            .await
            .map_err(|err| SandboxError::Transport(err.to_string()))?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(SandboxError::NotFound {
                what,
                name: name.to_owned(),
            });
        }
        if !status.is_success() {
            return Err(SandboxError::Api {
                status: status.as_u16(),
                body: capped(response.text().await.unwrap_or_default()),
            });
        }
        response
            .text()
            .await
            .map_err(|err| SandboxError::Transport(err.to_string()))
    }

    async fn delete_path(&self, path: &str) -> Result<(), SandboxError> {
        let response = self
            .client
            .delete(self.url(path))
            .bearer_auth(&self.config.token)
            .send()
            .await
            .map_err(|err| SandboxError::Transport(err.to_string()))?;
        let status = response.status();
        if status.is_success() || status == StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(SandboxError::Api {
            status: status.as_u16(),
            body: capped(response.text().await.unwrap_or_default()),
        })
    }
}

/// The runner script, ported verbatim from `grader.py::runner_script`.
///
/// It applies the model patch with swebench 5.0.2's commands (restoring a pristine tree before
/// each retry), prints [`APPLY_PATCH_FAIL`] and exits when none applies, otherwise prints
/// [`APPLY_PATCH_PASS`] and [`EVAL_MARKER`], runs the eval under `timeout`, and prints
/// [`TIMEOUT_MARKER`] on exit 124. Every marker is outside the graded region.
pub fn runner_script() -> String {
    let cmds = GIT_APPLY_CMDS
        .iter()
        .map(|cmd| format!("\"{cmd}\""))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"#!/bin/bash
set -uo pipefail
cd /testbed
cp /bench/patch.diff /tmp/patch.diff
applied=0
attempt=0
for cmd in {cmds}; do
  if [ "$attempt" -gt 0 ]; then git checkout -- . ; git clean -fd; fi
  attempt=$((attempt + 1))
  if $cmd /tmp/patch.diff; then applied=1; break; fi
  echo "Failed to apply patch to container: $cmd"
done
if [ "$applied" -eq 0 ] && git apply --check --reverse /tmp/patch.diff; then applied=1; fi
if [ "$applied" -eq 0 ]; then
  echo "{APPLY_PATCH_FAIL}"
  exit 0
fi
echo "{APPLY_PATCH_PASS}"
echo "{EVAL_MARKER}"
timeout {eval_timeout_s} /bin/bash /bench/eval.sh 2>&1
rc=$?
if [ "$rc" -eq 124 ]; then echo "{TIMEOUT_MARKER}"; fi
exit 0
"#,
        cmds = cmds,
        APPLY_PATCH_FAIL = APPLY_PATCH_FAIL,
        APPLY_PATCH_PASS = APPLY_PATCH_PASS,
        EVAL_MARKER = EVAL_MARKER,
        eval_timeout_s = EVAL_TIMEOUT_S,
        TIMEOUT_MARKER = TIMEOUT_MARKER,
    )
}

fn capped(body: String) -> String {
    const CAP: usize = 500;
    if body.len() <= CAP {
        return body;
    }
    let mut end = CAP;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &body[..end])
}
