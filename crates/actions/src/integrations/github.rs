#![forbid(unsafe_code)]
//! The GitHub transport behind `publish.design` — Git Data API and one Pull Request.
//!
//! The conveyor does not clone, check out or push with git. A publish is a small, explicit
//! sequence of API calls:
//!
//! ```text
//! GET  /repos/{o}/{r}/git/ref/heads/{branch}      the base commit
//! GET  /repos/{o}/{r}/git/commits/{commit}        its tree
//! GET  /repos/{o}/{r}/git/trees/{tree}?recursive  every current blob (path → sha)
//! POST /repos/{o}/{r}/git/blobs                   only the changed files
//! POST /repos/{o}/{r}/git/trees                   base_tree + changes (sha:null deletes)
//! POST /repos/{o}/{r}/git/commits                 one commit
//! POST /repos/{o}/{r}/git/refs                    the publish branch
//! POST /repos/{o}/{r}/pulls                       the review PR
//! ```
//!
//! **Never merges.** The PR is the human gate; the transport has no merge call.
//!
//! **The token is write-only.** It arrives from a sealed `secret`, is held in this struct, and is
//! never returned, logged or put in an error. [`Debug`] is hand-written to redact it.

use std::time::Duration;

use base64::Engine as _;
use reqwest::{Client, Method, Response, StatusCode};
use serde_json::{Value, json};
use sha1::{Digest as _, Sha1};

/// The public API root. Tests point this at a local stub.
pub const DEFAULT_API: &str = "https://api.github.com";

/// The per-call timeout. A blob upload is small; a hung GitHub must not hold a job for minutes.
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// A repository the caller named by URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoRef {
    /// The owner (user or organisation).
    pub owner: String,
    /// The repository name.
    pub name: String,
}

impl RepoRef {
    /// Parse `https://github.com/<owner>/<repo>[.git]`, `http://…` or `git@github.com:<owner>/<repo>.git`.
    pub fn parse(url: &str) -> Option<Self> {
        let trimmed = url.trim().trim_end_matches('/');
        let path = trimmed
            .strip_prefix("https://github.com/")
            .or_else(|| trimmed.strip_prefix("http://github.com/"))
            .or_else(|| trimmed.strip_prefix("git@github.com:"))?;
        let path = path.strip_suffix(".git").unwrap_or(path);
        let mut parts = path.split('/');
        let owner = parts.next()?;
        let name = parts.next()?;
        if !valid_segment(owner) || !valid_segment(name) || parts.next().is_some() {
            return None;
        }
        Some(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    /// `owner/name`.
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// A GitHub owner or repository segment: non-empty, not a dot segment, and limited to the
/// characters GitHub allows. The value becomes part of the API path, so a `..` or a slash must
/// never reach it.
fn valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

/// One blob in the base tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeEntry {
    /// Path inside the repository.
    pub path: String,
    /// The git blob SHA-1.
    pub sha: String,
}

/// One change in the new tree. `sha: None` deletes the path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeChange {
    /// Path inside the repository.
    pub path: String,
    /// The new blob SHA, or `None` to delete.
    pub sha: Option<String>,
}

/// The PR the publish opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pull {
    /// The PR number.
    pub number: u64,
    /// Its browser URL.
    pub html_url: String,
}

/// Why a GitHub call did not produce what the caller needed.
#[derive(Debug, thiserror::Error)]
pub enum GitHubError {
    /// No token, or an API base that is not http(s).
    #[error("the GitHub client is not configured")]
    Unavailable,
    /// 401 — the token is wrong or expired.
    #[error("GitHub rejected the token")]
    Unauthorized,
    /// 403 — the token lacks the permission, or the rate limit is exhausted.
    #[error("GitHub refused the request: {status} {body}")]
    Forbidden {
        /// The HTTP status.
        status: u16,
        /// A truncated response body.
        body: String,
    },
    /// 404 — the repository, branch or commit does not exist.
    #[error("GitHub has no such {what}")]
    NotFound {
        /// What was missing (`repository`, `branch`, `commit`, …).
        what: &'static str,
    },
    /// 422 — GitHub rejected the document (a branch collision, a bad tree, …).
    #[error("GitHub rejected the request: {status} {body}")]
    Validation {
        /// The HTTP status.
        status: u16,
        /// A truncated response body.
        body: String,
    },
    /// The connection failed or timed out.
    #[error("GitHub could not be reached: {0}")]
    Transport(String),
    /// A 2xx body this client cannot read.
    #[error("GitHub answered a body this client cannot read: {0}")]
    Protocol(String),
}

impl GitHubError {
    /// A retryable failure is a transport or server fault; everything else is the caller's.
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Transport(_))
            || matches!(self, Self::Forbidden { status, .. } if *status >= 500)
    }
}

/// The GitHub client.
#[derive(Clone)]
pub struct GitHub {
    client: Client,
    api: String,
    token: String,
}

impl std::fmt::Debug for GitHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHub")
            .field("api", &self.api)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl GitHub {
    /// Build from `CONVEYOR_GITHUB_API_BASE` (tests) or the public API, with an explicit token.
    pub fn new(token: impl Into<String>) -> Result<Self, GitHubError> {
        let api = std::env::var("CONVEYOR_GITHUB_API_BASE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_API.to_owned());
        Self::with_api(token, &api)
    }

    /// Build against a named API base. The test stub uses this.
    pub fn with_api(token: impl Into<String>, api: &str) -> Result<Self, GitHubError> {
        let token = token.into();
        if token.trim().is_empty() {
            return Err(GitHubError::Unavailable);
        }
        let api = api.trim().trim_end_matches('/').to_owned();
        if !(api.starts_with("http://") || api.starts_with("https://")) {
            return Err(GitHubError::Unavailable);
        }
        let client = Client::builder()
            .timeout(Duration::from_millis(DEFAULT_TIMEOUT_MS))
            .build()
            .map_err(|error| GitHubError::Transport(error.to_string()))?;
        Ok(Self { client, api, token })
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, format!("{}{}", self.api, path))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "conveyor-publish")
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
        what: &'static str,
    ) -> Result<Value, GitHubError> {
        let response = request
            .send()
            .await
            .map_err(|error| GitHubError::Transport(error.to_string()))?;
        self.check(response, what).await
    }

    async fn check(&self, response: Response, what: &'static str) -> Result<Value, GitHubError> {
        let status = response.status();
        if status.is_success() {
            return response
                .json::<Value>()
                .await
                .map_err(|error| GitHubError::Protocol(error.to_string()));
        }
        let body = response.text().await.unwrap_or_default();
        let body = body.chars().take(500).collect::<String>();
        if status == StatusCode::UNAUTHORIZED {
            return Err(GitHubError::Unauthorized);
        }
        if status == StatusCode::NOT_FOUND {
            return Err(GitHubError::NotFound { what });
        }
        if status == StatusCode::UNPROCESSABLE_ENTITY {
            return Err(GitHubError::Validation {
                status: status.as_u16(),
                body,
            });
        }
        if status.is_server_error() || status == StatusCode::FORBIDDEN {
            return Err(GitHubError::Forbidden {
                status: status.as_u16(),
                body,
            });
        }
        Err(GitHubError::Validation {
            status: status.as_u16(),
            body,
        })
    }

    /// The commit SHA a branch points at.
    pub async fn branch_head(&self, repo: &RepoRef, branch: &str) -> Result<String, GitHubError> {
        let value = self
            .send(
                self.request(
                    Method::GET,
                    &format!(
                        "/repos/{}/{}/git/ref/heads/{}",
                        repo.owner, repo.name, branch
                    ),
                ),
                "branch",
            )
            .await?;
        value
            .pointer("/object/sha")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| GitHubError::Protocol("the ref has no object.sha".to_owned()))
    }

    /// The tree SHA of a commit.
    pub async fn commit_tree(&self, repo: &RepoRef, commit: &str) -> Result<String, GitHubError> {
        let value = self
            .send(
                self.request(
                    Method::GET,
                    &format!("/repos/{}/{}/git/commits/{}", repo.owner, repo.name, commit),
                ),
                "commit",
            )
            .await?;
        value
            .pointer("/tree/sha")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| GitHubError::Protocol("the commit has no tree.sha".to_owned()))
    }

    /// Every blob in a tree, recursively.
    pub async fn tree(&self, repo: &RepoRef, tree: &str) -> Result<Vec<TreeEntry>, GitHubError> {
        let value = self
            .send(
                self.request(
                    Method::GET,
                    &format!(
                        "/repos/{}/{}/git/trees/{}?recursive=1",
                        repo.owner, repo.name, tree
                    ),
                ),
                "tree",
            )
            .await?;
        if value.get("truncated").and_then(Value::as_bool) == Some(true) {
            return Err(GitHubError::Protocol(
                "GitHub truncated the recursive tree; the publish delta would be incomplete"
                    .to_owned(),
            ));
        }
        let entries = value
            .get("tree")
            .and_then(Value::as_array)
            .ok_or_else(|| GitHubError::Protocol("the tree has no `tree` array".to_owned()))?;
        Ok(entries
            .iter()
            .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("blob"))
            .filter_map(|entry| {
                Some(TreeEntry {
                    path: entry.get("path")?.as_str()?.to_owned(),
                    sha: entry.get("sha")?.as_str()?.to_owned(),
                })
            })
            .collect())
    }

    /// Upload one blob and return its SHA.
    pub async fn create_blob(&self, repo: &RepoRef, bytes: &[u8]) -> Result<String, GitHubError> {
        let body = json!({
            "content": base64::engine::general_purpose::STANDARD.encode(bytes),
            "encoding": "base64",
        });
        let value = self
            .send(
                self.request(
                    Method::POST,
                    &format!("/repos/{}/{}/git/blobs", repo.owner, repo.name),
                )
                .json(&body),
                "blob",
            )
            .await?;
        value
            .get("sha")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| GitHubError::Protocol("the blob has no sha".to_owned()))
    }

    /// Create a tree over `base_tree` with the given changes.
    pub async fn create_tree(
        &self,
        repo: &RepoRef,
        base_tree: &str,
        changes: &[TreeChange],
    ) -> Result<String, GitHubError> {
        let tree: Vec<Value> = changes
            .iter()
            .map(|change| {
                json!({
                    "path": &change.path,
                    "mode": "100644",
                    "type": "blob",
                    "sha": change.sha.as_deref(),
                })
            })
            .collect();
        let body = json!({ "base_tree": base_tree, "tree": tree });
        let value = self
            .send(
                self.request(
                    Method::POST,
                    &format!("/repos/{}/{}/git/trees", repo.owner, repo.name),
                )
                .json(&body),
                "tree",
            )
            .await?;
        value
            .get("sha")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| GitHubError::Protocol("the new tree has no sha".to_owned()))
    }

    /// Create one commit.
    pub async fn create_commit(
        &self,
        repo: &RepoRef,
        message: &str,
        tree: &str,
        parents: &[String],
    ) -> Result<String, GitHubError> {
        let body = json!({ "message": message, "tree": tree, "parents": parents });
        let value = self
            .send(
                self.request(
                    Method::POST,
                    &format!("/repos/{}/{}/git/commits", repo.owner, repo.name),
                )
                .json(&body),
                "commit",
            )
            .await?;
        value
            .get("sha")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| GitHubError::Protocol("the new commit has no sha".to_owned()))
    }

    /// Create a branch ref at `sha`.
    pub async fn create_ref(
        &self,
        repo: &RepoRef,
        branch: &str,
        sha: &str,
    ) -> Result<(), GitHubError> {
        let body = json!({ "ref": format!("refs/heads/{branch}"), "sha": sha });
        self.send(
            self.request(
                Method::POST,
                &format!("/repos/{}/{}/git/refs", repo.owner, repo.name),
            )
            .json(&body),
            "ref",
        )
        .await
        .map(|_| ())
    }

    /// Open a pull request. This client has no merge call by design.
    pub async fn open_pull(
        &self,
        repo: &RepoRef,
        title: &str,
        body: &str,
        head: &str,
        base: &str,
    ) -> Result<Pull, GitHubError> {
        let payload = json!({ "title": title, "body": body, "head": head, "base": base });
        let value = self
            .send(
                self.request(
                    Method::POST,
                    &format!("/repos/{}/{}/pulls", repo.owner, repo.name),
                )
                .json(&payload),
                "pull request",
            )
            .await?;
        let number = value
            .get("number")
            .and_then(Value::as_u64)
            .ok_or_else(|| GitHubError::Protocol("the pull has no number".to_owned()))?;
        let html_url = value
            .get("html_url")
            .and_then(Value::as_str)
            .ok_or_else(|| GitHubError::Protocol("the pull has no html_url".to_owned()))?
            .to_owned();
        Ok(Pull { number, html_url })
    }
}

/// The git blob SHA-1 of a file: `sha1("blob <len>\0" + bytes)`, lowercase hex.
///
/// GitHub's tree entries are keyed by this value, so an unchanged file needs no blob upload.
pub fn git_blob_sha(bytes: &[u8]) -> String {
    let mut digest = Sha1::new();
    digest.update(format!("blob {}\0", bytes.len()).as_bytes());
    digest.update(bytes);
    let out = digest.finalize();
    let mut hex = String::with_capacity(40);
    for byte in out {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_urls_parse() {
        let repo =
            RepoRef::parse("https://github.com/nonagenticai/nonagentic_homepage").expect("https");
        assert_eq!(repo.slug(), "nonagenticai/nonagentic_homepage");
        let repo = RepoRef::parse("git@github.com:nonagenticai/conveyor.git").expect("ssh");
        assert_eq!(repo.slug(), "nonagenticai/conveyor");
        assert!(RepoRef::parse("https://gitlab.com/a/b").is_none());
        assert!(RepoRef::parse("https://github.com/a").is_none());
        assert!(RepoRef::parse("https://github.com/../evil").is_none());
    }

    #[test]
    fn a_known_blob_sha_is_the_git_one() {
        // `printf 'hello\n' | git hash-object --stdin` = ce013625030ba8dba906f756967f9e9ca394464a
        assert_eq!(
            git_blob_sha(b"hello\n"),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    #[test]
    fn the_debug_impl_redacts_the_token() {
        let client = GitHub::with_api("ghp_secret_token_value", DEFAULT_API).expect("client");
        let debug = format!("{client:?}");
        assert!(!debug.contains("ghp_secret_token_value"), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");
    }
}
