#![forbid(unsafe_code)]
//! `publish.design` — build the public pack and open a pack PR.
//!
//! The job is the console button's worker half. It:
//!
//! 1. reads the target `repository` row and opens its sealed token (never logged);
//! 2. builds the data snapshot and the publishable output pages into a temp directory, using the
//!    same [`crate::snapshot`] and [`crate::publish_outputs`] code as the manual publisher;
//! 3. overlays those files on the pack the deployed image serves (`/srv/site/design`);
//! 4. commits the delta to one `publish/design-*` branch and opens a PR — the pack lands under
//!    `docs/design/` in this repository, the tree the app image serves;
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
    // The rework register is optional: a pack that carries one re-publishes it unchanged, a pack
    // that does not publishes none. Its absence is honest, never a job failure.
    let conformance = pack_root.join("data/rework.json");
    let conformance = conformance.is_file().then_some(conformance);
    let generated_at = Utc::now().to_rfc3339();
    let report = snapshot::run_with_pool(
        pool,
        SnapshotArgs {
            tenant: tenant_slug,
            out: temp.0.join("data"),
            outputs_dir: Some(temp.0.join("docs")),
            generated_at: Some(generated_at.clone()),
            issues,
            conformance,
            feedback_url: snapshot::DEFAULT_FEEDBACK_URL.to_owned(),
            database_url: None,
            git_sha: std::env::var("CONVEYOR_GIT_SHA").ok(),
            manifest_only: false,
            build_time: std::env::var("CONVEYOR_BUILD_TIME").ok(),
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

    let files = collect_pack(&temp.0)
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

/// The DCO trailer the public repository's `dco` job requires on every non-merge commit. The
/// GitHub commit API attributes the commit to the sealed token's user, so the trailer names the
/// same identity; the noreply address is the one GitHub exposes without a public email.
fn sign_off(login: &str, id: u64) -> String {
    format!("Signed-off-by: {login} <{id}+{login}@users.noreply.github.com>")
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
    // #400: the generated PR is born red without this. The dco job greps every non-merge commit
    // for `^Signed-off-by: .+ <[^<>@ ]+@[^<> ]+>$`; the API-made commit carried none.
    let (login, id) = github.user_identity().await?;
    let message = format!(
        "design: publish the programme snapshot\n\nsnapshot: {manifest_sha}\ngenerated: {generated_at}\noutputs: {outputs}\nfiles: {}\n\n{}",
        changes.len(),
        sign_off(&login, id)
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
/// The pack lives in this repository under `docs/design/` — the same tree the app image copies
/// and serves at `/design`. `studio.html` needs no `index.html`: the API serves it as the
/// directory index (`crates/api/src/web.rs`).
fn targets(files: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    files
        .iter()
        .map(|(rel, bytes)| (format!("docs/design/{rel}"), bytes.clone()))
        .collect()
}

/// The generated files the publisher owns: the snapshot's data files and the published output
/// pages. Everything else under `docs/design/` is hand-authored (documents, decisions, scripts,
/// the issue registry) and a publish never deletes it.
///
/// `conformance.json` and `conformance.md` are deliberately absent: only
/// `scripts/publish-data.sh` writes them, so the app publisher must never delete them.
const GENERATED_DATA: [&str; 17] = [
    "programme.json",
    "organisation.json",
    "config.json",
    "work.json",
    "msp.json",
    "architecture.json",
    "artefacts.json",
    "ops.json",
    "measure.json",
    "secrets.json",
    "evidence.json",
    "feedback.json",
    "rework.json",
    "register.json",
    "register.js",
    "snapshot.js",
    "manifest.json",
];

/// True when a repository path is a file the publisher generates and may therefore delete when
/// a later pack no longer contains it.
fn is_generated_path(path: &str) -> bool {
    if let Some(rest) = path.strip_prefix("docs/design/data/") {
        return GENERATED_DATA.contains(&rest);
    }
    if let Some(rest) = path.strip_prefix("docs/design/docs/") {
        return rest == "outputs.js" || rest.starts_with("output-");
    }
    false
}

/// Stale generated files under `docs/design/` that the new pack no longer contains.
fn deletions(remote: &[TreeEntry], targets: &BTreeMap<String, Vec<u8>>) -> Vec<TreeChange> {
    remote
        .iter()
        .filter(|entry| is_generated_path(&entry.path) && !targets.contains_key(&entry.path))
        .map(|entry| TreeChange {
            path: entry.path.clone(),
            sha: None,
        })
        .collect()
}

/// The full pack the image serves plus the generated data and output pages.
///
/// The generated files the publisher owns: the snapshot's data files plus the published output
/// pages and index. Everything else under `docs/design/` is hand-authored and comes from the
/// target branch, never from the deployed image: the image can be older than main, and committing
/// its copy silently reverts main (#477 — measured as a 9-line drift in `actions_invoked[*].line`
/// on a publish PR, because the image's `v2.js` was older than main's).
fn collect_pack(temp: &Path) -> std::io::Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    collect_flat(&temp.join("data"), "data", &mut files)?;
    collect_flat(&temp.join("docs"), "docs", &mut files)?;
    Ok(files)
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
    fn pack_files_land_under_docs_design() {
        let mut files = BTreeMap::new();
        files.insert("studio.html".to_owned(), b"x".to_vec());
        let targets = targets(&files);
        assert!(targets.contains_key("docs/design/studio.html"));
        assert!(
            !targets
                .keys()
                .any(|path| path.starts_with("design/") || path.starts_with("dist/")),
            "the retired repo layout must not be written"
        );
    }

    #[test]
    fn generated_data_includes_the_pack_outputs_but_not_the_script_written_conformance() {
        for name in ["architecture.json", "ops.json", "rework.json"] {
            assert!(
                GENERATED_DATA.contains(&name),
                "{name} is written by the pack and must be deletable when a later pack omits it"
            );
        }
        for name in ["conformance.json", "conformance.md"] {
            assert!(
                !GENERATED_DATA.contains(&name),
                "{name} is written by publish-data.sh alone; the app publisher must never delete it"
            );
        }
    }

    #[test]
    fn stale_generated_pages_are_deleted_and_hand_authored_files_are_not() {
        let mut files = BTreeMap::new();
        files.insert("docs/output-new.html".to_owned(), b"x".to_vec());
        let targets = targets(&files);
        let remote = vec![
            entry("docs/design/docs/output-new.html"),
            entry("docs/design/docs/output-old.html"),
            entry("docs/design/docs/outputs.js"),
            entry("docs/design/data/snapshot.js"),
            entry("docs/design/data/architecture.json"),
            entry("docs/design/data/ops.json"),
            entry("docs/design/data/rework.json"),
            entry("docs/design/data/conformance.json"),
            entry("docs/design/data/conformance.md"),
            entry("docs/design/data/issues.json"),
            entry("docs/design/data/coverage.json"),
            entry("docs/design/docs/documents.js"),
            entry("docs/design/README.md"),
            entry("docs/design/scripts/build-docs.py"),
            entry("docs/design/studio.html"),
            entry("docs/evidence/keep.md"),
            entry("README.md"),
        ];
        let deletes: Vec<String> = deletions(&remote, &targets)
            .into_iter()
            .map(|change| change.path)
            .collect();
        assert!(!deletes.contains(&"docs/design/docs/output-new.html".to_owned()));
        assert!(deletes.contains(&"docs/design/docs/output-old.html".to_owned()));
        assert!(deletes.contains(&"docs/design/docs/outputs.js".to_owned()));
        assert!(deletes.contains(&"docs/design/data/snapshot.js".to_owned()));
        assert!(deletes.contains(&"docs/design/data/architecture.json".to_owned()));
        assert!(
            deletes.contains(&"docs/design/data/ops.json".to_owned()),
            "ops.json is pack-written and deletable when a later pack omits it"
        );
        assert!(
            deletes.contains(&"docs/design/data/rework.json".to_owned()),
            "a pack without --conformance publishes no rework register, so a stale one is deleted"
        );
        for keep in [
            "docs/design/data/conformance.json",
            "docs/design/data/conformance.md",
            "docs/design/data/issues.json",
            "docs/design/data/coverage.json",
            "docs/design/docs/documents.js",
            "docs/design/README.md",
            "docs/design/scripts/build-docs.py",
            "docs/design/studio.html",
            "docs/evidence/keep.md",
            "README.md",
        ] {
            assert!(
                !deletes.contains(&keep.to_owned()),
                "{keep} is hand-authored and must not be deleted"
            );
        }
    }

    #[test]
    fn a_publish_commit_is_signed_off_for_the_token_user() {
        let line = sign_off("georgiedekker", 11147883);
        assert_eq!(
            line,
            "Signed-off-by: georgiedekker <11147883+georgiedekker@users.noreply.github.com>"
        );
        // The dco job's shape: `^Signed-off-by: .+ <[^<>@ ]+@[^<> ]+>$`. No spaces in the
        // address, angle brackets exactly around it.
        let address = line
            .strip_prefix("Signed-off-by: ")
            .and_then(|rest| rest.split_once('<'))
            .map(|(_, address)| address)
            .expect("a name and an address");
        assert!(address.ends_with('>'), "{line}");
        assert!(!address.contains(' '), "{line}");
        assert!(address.contains('@'), "{line}");
    }

    #[test]
    fn a_publish_commits_only_the_generated_files() {
        let temp = TempDir::new().expect("temp dir");
        std::fs::create_dir_all(temp.0.join("data")).expect("data dir");
        std::fs::write(temp.0.join("data/snapshot.js"), b"snapshot").expect("write data");
        std::fs::create_dir_all(temp.0.join("docs")).expect("docs dir");
        std::fs::write(temp.0.join("docs/output-1.html"), b"page").expect("write page");

        let files = collect_pack(&temp.0).expect("collect");
        let paths: Vec<&str> = files.keys().map(String::as_str).collect();
        assert_eq!(paths, ["data/snapshot.js", "docs/output-1.html"]);
    }
}
