#![forbid(unsafe_code)]
//! `ops.ci_status` — GitHub Actions runs for one repository (`#170` P2, owner ruling 2026-11-16).
//!
//! The credential is the platform's `CONVEYOR_CI_TOKEN`, not a tenant secret: CI status is estate
//! metadata, and the action refuses visibly (`Unconfigured` ⇒ `503`) when the token is absent
//! rather than answering an empty list that would read as "no failures".

use serde_json::Value;

use crate::OpsError;

/// GitHub's public API.
pub const DEFAULT_API_URL: &str = "https://api.github.com";

/// The repository this estate's CI runs for, when the caller names none.
pub const DEFAULT_REPO: &str = "nonagenticai/conveyor";

/// The largest page the action will ask for.
pub const MAX_LIMIT: u32 = 100;

/// How much of an error body is kept.
const BODY_CAP: usize = 2_000;

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// Everything a CI read needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CiConfig {
    /// The API base URL (`CONVEYOR_CI_API_URL`, default GitHub).
    pub api_url: String,
    /// The bearer (`CONVEYOR_CI_TOKEN`). Required.
    pub token: String,
    /// The repository used when the caller names none (`CONVEYOR_CI_REPO`).
    pub default_repo: String,
}

impl CiConfig {
    /// Read from the process environment.
    pub fn from_env() -> Result<Self, OpsError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Build from a lookup so the unconfigured path is testable.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, OpsError> {
        let token = trimmed(get("CONVEYOR_CI_TOKEN")).ok_or(OpsError::Unconfigured)?;
        Ok(Self {
            api_url: trimmed(get("CONVEYOR_CI_API_URL"))
                .unwrap_or_else(|| DEFAULT_API_URL.to_owned()),
            token,
            default_repo: trimmed(get("CONVEYOR_CI_REPO"))
                .unwrap_or_else(|| DEFAULT_REPO.to_owned()),
        })
    }
}

/// One repository's recent workflow runs.
#[derive(Debug)]
pub struct CiRuns {
    /// The repository asked for, as `owner/name`.
    pub repo: String,
    /// The branch filter, when one was given.
    pub branch: Option<String>,
    /// The run objects, exactly as the API returned them.
    pub runs: Vec<Value>,
}

/// List recent workflow runs, newest first.
pub async fn runs(
    config: &CiConfig,
    repo: Option<&str>,
    branch: Option<&str>,
    limit: u32,
) -> Result<CiRuns, OpsError> {
    let repo = trimmed(repo.map(str::to_owned)).unwrap_or_else(|| config.default_repo.clone());
    let limit = limit.clamp(1, MAX_LIMIT);
    let mut url = reqwest::Url::parse(&format!(
        "{}/repos/{repo}/actions/runs",
        config.api_url.trim_end_matches('/')
    ))
    .map_err(|error| OpsError::Config(format!("{repo:?}: {error}")))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("per_page", &limit.to_string());
        if let Some(branch) = branch.filter(|branch| !branch.trim().is_empty()) {
            pairs.append_pair("branch", branch);
        }
    }

    let http = reqwest::Client::builder()
        .timeout(crate::DEFAULT_TIMEOUT)
        .user_agent("conveyor-ops/1")
        .build()
        .map_err(|error| OpsError::Config(error.to_string()))?;
    let response = http
        .get(url)
        .bearer_auth(&config.token)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|error| OpsError::Transport(error.to_string()))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| OpsError::Transport(error.to_string()))?;
    if !status.is_success() {
        return Err(OpsError::Api {
            status: status.as_u16(),
            body: if body.len() <= BODY_CAP {
                body
            } else {
                body[..BODY_CAP].to_owned()
            },
        });
    }
    let document: Value = serde_json::from_str(&body)
        .map_err(|error| OpsError::Transport(format!("GitHub answered non-JSON: {error}")))?;
    let runs = document
        .get("workflow_runs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(CiRuns {
        repo,
        branch: branch.map(str::to_owned),
        runs,
    })
}
