#![forbid(unsafe_code)]
//! `publish.design` — build the public pack and open a homepage PR.
//!
//! The job is the console button's worker half. It:
//!
//! 1. reads the target `repository` row and opens its sealed token (never logged);
//! 2. builds the data snapshot and the publishable output pages into a temp directory, using the
//!    same [`crate::snapshot`] and [`crate::publish_outputs`] code as the manual publisher;
//! 3. overlays those files on the pack the deployed image serves (`/srv/site/design`);
//! 4. commits the delta to one `publish/design-*` branch and opens a PR — both `design/` and
//!    `dist/design/` are written because the homepage copies the pack verbatim into `dist/`;
//! 5. records the PR URL on the job row (`payload.result`) and appends `publish.completed`.
//!
//! **It never merges.** The PR is the human gate. **It never retries a opened PR**: the job has
//! `max_attempts = 1`, so a failure after the PR exists is dead with the URL in `last_error`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use chrono::Utc;
use conveyor_actions::integrations::github::{
    GitHub, GitHubError, RepoRef, TreeChange, TreeEntry, git_blob_sha,
};
use conveyor_store as store;
use conveyor_store::TenantId;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::publish_outputs;
use crate::snapshot::{self, SnapshotArgs};
use crate::worker::JobFailure;

/// The job kind the action enqueues (one spelling, shared with the action).
pub const KIND: &str = conveyor_actions::publish::PUBLISH_KIND;

/// Remove the temp tree however the job ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let path =
            std::env::temp_dir().join(format!("conveyor-publish-{}", Uuid::now_v7().simple()));
        std::fs::create_dir_all(path.join("data"))?;
        std::fs::create_dir_all(path.join("docs"))?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run one publish job. `payload.dedupe_key` names the live job row to record the result on.
pub async fn run(pool: &PgPool, tenant: TenantId, payload: &Value) -> Result<(), JobFailure> {
    let repository_id = payload
        .get("repository_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| JobFailure::permanent("publish.design needs a repository_id"))?;
    let dedupe = payload
        .get("dedupe_key")
        .and_then(Value::as_str)
        .ok_or_else(|| JobFailure::permanent("publish.design needs a dedupe_key"))?;
    let tenant_slug = match payload.get("tenant_slug").and_then(Value::as_str) {
        Some(slug) => slug.to_owned(),
        None => store::tenants::slug_by_id(pool, tenant.as_uuid())
            .await
            .map_err(|error| JobFailure::retryable(format!("database: {error}")))?
            .ok_or_else(|| JobFailure::permanent("the tenant has no slug"))?,
    };

    // Read the target and open its token inside one scoped transaction. The plaintext exists only
    // in this process; it is never logged and never written to the job row.
    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    let job = store::jobs::by_dedupe(&mut tx, tenant, KIND, dedupe)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?
        .into_iter()
        .next()
        .ok_or_else(|| JobFailure::permanent("publish.design found no live job row"))?;
    let repository = store::repository::get_repository_tx(&mut tx, tenant, repository_id)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?
        .ok_or_else(|| JobFailure::permanent("the publish repository no longer exists"))?;
    let secret_id = repository
        .credential_secret_id
        .ok_or_else(|| JobFailure::permanent("the publish repository has no credential secret"))?;
    let token = store::secret::open_tx(&mut tx, tenant, secret_id)
        .await
        .map_err(|error| {
            JobFailure::permanent(format!("the publish credential cannot be opened: {error}"))
        })?;
    tx.rollback()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    let token = String::from_utf8(token)
        .map_err(|_| JobFailure::permanent("the publish credential is not a UTF-8 token"))?
        .trim()
        .to_owned();
    let repo = RepoRef::parse(&repository.url).ok_or_else(|| {
        JobFailure::permanent("the publish repository URL is not a GitHub repository")
    })?;
    let branch_base = repository.default_branch.clone();

    let temp = TempDir::new()
        .map_err(|error| JobFailure::permanent(format!("could not create a temp dir: {error}")))?;
    let web_root = std::env::var("CONVEYOR_WEB_ROOT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/srv/site"));
    let pack_root = web_root.join("design");
    if !pack_root.is_dir() {
        return Err(JobFailure::permanent(format!(
            "the pack root {} is not present in this image; set CONVEYOR_WEB_ROOT",
            pack_root.display()
        )));
    }
    let issues = pack_root.join("data/issues.json");
    if !issues.is_file() {
        return Err(JobFailure::permanent(format!(
            "the issue registry {} is not present; publish once with the manual path",
            issues.display()
        )));
    }
    let generated_at = Utc::now().to_rfc3339();
    let report = snapshot::run_with_pool(
        pool,
        SnapshotArgs {
            tenant: tenant_slug,
            out: temp.0.join("data"),
            outputs_dir: Some(temp.0.join("docs")),
            generated_at: Some(generated_at.clone()),
            issues,
            feedback_url: snapshot::DEFAULT_FEEDBACK_URL.to_owned(),
            database_url: None,
            git_sha: std::env::var("CONVEYOR_GIT_SHA").ok(),
        },
    )
    .await
    .map_err(|error| match error {
        snapshot::SnapshotError::Database(_)
        | snapshot::SnapshotError::PublishOutputs(
            publish_outputs::PublishOutputsError::Database(_),
        ) => JobFailure::retryable(format!("snapshot: {error}")),
        other => JobFailure::permanent(format!("snapshot: {other}")),
    })?;

    let files = collect_pack(&pack_root, &temp.0)
        .map_err(|error| JobFailure::permanent(format!("collecting the pack: {error}")))?;
    let github = GitHub::new(token).map_err(|error| {
        JobFailure::permanent(format!("the GitHub client is not configured: {error}"))
    })?;
    let outcome = publish(
        &github,
        &repo,
        &branch_base,
        &files,
        &generated_at,
        &report.manifest_sha256,
        report.outputs,
    )
    .await
    .map_err(|error| {
        if error.retryable() {
            JobFailure::retryable(format!("GitHub: {error}"))
        } else {
            JobFailure::permanent(format!("GitHub: {error}"))
        }
    })?;

    let result = json!({
        "pr_url": outcome.pr_url,
        "pr_number": outcome.pr_number,
        "branch": outcome.branch,
        "files": outcome.files,
        "generated_at": generated_at,
        "snapshot_sha256": report.manifest_sha256,
        "outputs": report.outputs,
    });
    store::jobs::record_result(pool, tenant, job.id, result.clone())
        .await
        .map_err(|error| {
            let url = outcome
                .pr_url
                .as_deref()
                .map(|url| format!(" (the PR is open at {url})"))
                .unwrap_or_default();
            JobFailure::permanent(format!(
                "the publish finished but its result could not be recorded{url}: {error}"
            ))
        })?;
    append_event(
        pool,
        tenant,
        job.id,
        if outcome.pr_url.is_some() {
            "publish.completed"
        } else {
            "publish.unchanged"
        },
        result,
    )
    .await;
    Ok(())
}

/// The publish outcome.
#[derive(Clone, Debug, Default)]
struct Outcome {
    pr_url: Option<String>,
    pr_number: Option<u64>,
    branch: String,
    files: usize,
}

/// Commit the delta and open the PR (or report that the pack is unchanged).
async fn publish(
    github: &GitHub,
    repo: &RepoRef,
    branch_base: &str,
    files: &BTreeMap<String, Vec<u8>>,
    generated_at: &str,
    manifest_sha: &str,
    outputs: usize,
) -> Result<Outcome, GitHubError> {
    let head = github.branch_head(repo, branch_base).await?;
    let base_tree = github.commit_tree(repo, &head).await?;
    let remote = github.tree(repo, &base_tree).await?;
    let targets = targets(files);

    let remote_by_path: HashMap<&str, &str> = remote
        .iter()
        .map(|entry| (entry.path.as_str(), entry.sha.as_str()))
        .collect();
    let mut blobs: HashMap<String, String> = remote
        .iter()
        .map(|entry| (entry.sha.clone(), entry.sha.clone()))
        .collect();
    let mut changes: Vec<TreeChange> = Vec::new();
    for (path, bytes) in &targets {
        let git_sha = git_blob_sha(bytes);
        if remote_by_path.get(path.as_str()) == Some(&git_sha.as_str()) {
            continue; // already byte-identical on the branch
        }
        let blob_sha = match blobs.get(&git_sha) {
            Some(sha) => sha.clone(),
            None => {
                let sha = github.create_blob(repo, bytes).await?;
                blobs.insert(git_sha, sha.clone());
                sha
            }
        };
        changes.push(TreeChange {
            path: path.clone(),
            sha: Some(blob_sha),
        });
    }
    changes.extend(deletions(&remote, &targets));
    if changes.is_empty() {
        return Ok(Outcome {
            branch: String::new(),
            files: 0,
            ..Outcome::default()
        });
    }
    let tree = github.create_tree(repo, &base_tree, &changes).await?;
    let message = format!(
        "design: publish the programme snapshot\n\nsnapshot: {manifest_sha}\ngenerated: {generated_at}\noutputs: {outputs}\nfiles: {}",
        changes.len()
    );
    let commit = github
        .create_commit(repo, &message, &tree, std::slice::from_ref(&head))
        .await?;
    let branch = format!(
        "publish/design-{}-{}",
        Utc::now().format("%Y%m%d-%H%M%S"),
        &Uuid::now_v7().simple().to_string()[..8]
    );
    github.create_ref(repo, &branch, &commit).await?;
    let body = format!(
        "Automated publish from the conveyor console.\n\n\
         - snapshot payload: `{manifest_sha}`\n\
         - generated: {generated_at}\n\
         - platform outputs: {outputs}\n\
         - files changed: {}\n\n\
         Merge to deploy the public design site. The pack is generated from the platform's rows; \
         no hand edits.\n",
        changes.len()
    );
    let pull = github
        .open_pull(
            repo,
            "design: publish the programme snapshot",
            &body,
            &branch,
            branch_base,
        )
        .await?;
    Ok(Outcome {
        pr_url: Some(pull.html_url),
        pr_number: Some(pull.number),
        branch,
        files: changes.len(),
    })
}

/// Every repository path a pack file must occupy.
///
/// `studio.html` is also the directory index: the homepage build copies it to
/// `dist/design/index.html`. Every other file exists in both the source mirror and `dist`.
fn targets(files: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for (rel, bytes) in files {
        out.insert(format!("design/{rel}"), bytes.clone());
        out.insert(format!("dist/design/{rel}"), bytes.clone());
        if rel == "studio.html" {
            out.insert("dist/design/index.html".to_owned(), bytes.clone());
        }
    }
    out
}

/// Stale files under `design/` or `dist/design/` that the new pack no longer contains.
fn deletions(remote: &[TreeEntry], targets: &BTreeMap<String, Vec<u8>>) -> Vec<TreeChange> {
    remote
        .iter()
        .filter(|entry| {
            (entry.path.starts_with("design/") || entry.path.starts_with("dist/design/"))
                && !targets.contains_key(&entry.path)
        })
        .map(|entry| TreeChange {
            path: entry.path.clone(),
            sha: None,
        })
        .collect()
}

/// The full pack the image serves plus the generated data and output pages.
///
/// The pack's own `data/` (placeholder snapshot + issue registry), root `index.html` (derived),
/// `scripts/`, `legacy/`, Markdown sources and the placeholder `docs/outputs.js` are not
/// published. The generated `data/` and `docs/` come from the temp snapshot.
fn collect_pack(pack: &Path, temp: &Path) -> std::io::Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    collect_dir(pack, "", &mut files)?;
    collect_flat(&temp.join("data"), "data", &mut files)?;
    collect_flat(&temp.join("docs"), "docs", &mut files)?;
    Ok(files)
}

fn collect_dir(root: &Path, rel: &str, out: &mut BTreeMap<String, Vec<u8>>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let child = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        let path = entry.path();
        if path.is_dir() {
            if rel.is_empty() && matches!(name.as_str(), "scripts" | "legacy" | "data") {
                continue;
            }
            collect_dir(&path, &child, out)?;
            continue;
        }
        if name.ends_with(".md")
            || (rel.is_empty() && (name == "index.html" || name == "issues.json"))
            || child == "docs/outputs.js"
            || (child.starts_with("docs/output-") && child.ends_with(".html"))
        {
            continue;
        }
        out.insert(child, std::fs::read(&path)?);
    }
    Ok(())
}

fn collect_flat(
    root: &Path,
    prefix: &str,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_file() {
            out.insert(format!("{prefix}/{name}"), std::fs::read(&path)?);
        }
    }
    Ok(())
}

/// Append one publish event; a failure is logged, never allowed to undo an opened PR.
async fn append_event(
    pool: &PgPool,
    tenant: TenantId,
    job_id: Uuid,
    event_type: &'static str,
    payload: Value,
) {
    let result: Result<(), String> = async {
        let mut tx = store::begin_scoped(pool, tenant)
            .await
            .map_err(|error| error.to_string())?;
        let event = store::NewEvent {
            topic: format!("publish:{job_id}"),
            event_type: event_type.to_owned(),
            aggregate_type: "publish".to_owned(),
            aggregate_id: job_id,
            payload,
            correlation_id: None,
            causation_id: None,
        };
        store::outbox::append(&mut tx, tenant, &event)
            .await
            .map_err(|error| error.to_string())?;
        tx.commit().await.map_err(|error| error.to_string())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, job = %job_id, event = event_type, "append publish event");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> TreeEntry {
        TreeEntry {
            path: path.to_owned(),
            sha: "0123456789012345678901234567890123456789".to_owned(),
        }
    }

    #[test]
    fn studio_html_also_writes_the_dist_index() {
        let mut files = BTreeMap::new();
        files.insert("studio.html".to_owned(), b"x".to_vec());
        let targets = targets(&files);
        assert!(targets.contains_key("design/studio.html"));
        assert!(targets.contains_key("dist/design/studio.html"));
        assert!(targets.contains_key("dist/design/index.html"));
    }

    #[test]
    fn stale_output_pages_are_deleted_and_current_ones_are_not() {
        let mut files = BTreeMap::new();
        files.insert("docs/output-new.html".to_owned(), b"x".to_vec());
        let targets = targets(&files);
        let remote = vec![
            entry("design/docs/output-new.html"),
            entry("design/docs/output-old.html"),
            entry("dist/design/docs/output-old.html"),
            entry("design/index.html"),
            entry("index.html"),
            entry("assets/keep.png"),
        ];
        let deletes: Vec<String> = deletions(&remote, &targets)
            .into_iter()
            .map(|change| change.path)
            .collect();
        assert!(!deletes.contains(&"design/docs/output-new.html".to_owned()));
        assert!(deletes.contains(&"design/docs/output-old.html".to_owned()));
        assert!(deletes.contains(&"dist/design/docs/output-old.html".to_owned()));
        assert!(deletes.contains(&"design/index.html".to_owned()));
        assert!(
            !deletes.contains(&"index.html".to_owned()),
            "outside the design pack"
        );
        assert!(
            !deletes.contains(&"assets/keep.png".to_owned()),
            "outside the design pack"
        );
    }
}
