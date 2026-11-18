#![forbid(unsafe_code)]
//! The three `ops.*` writes — restart, scale, rollback — the gated half of T7.
//!
//! Every one is `effect = destructive` (SPEC_V2 §11.4): it names an approved approval whose
//! `target` is the exact object the call mutates, it takes a `dry_run` that returns the request
//! and changes nothing, and it reports the observed state back in `checks[]`. The gate runs in
//! the caller's transaction **before any cluster call** (`control::approvals::require_approval_tx`),
//! so a refused call leaves no backend trace.
//!
//! The client is the same one the read family uses: the service-account token of the pod the
//! action runs in (`conveyor-worker`), never the serve container.
//!
//! **Rollback is Kubernetes' own `kubectl rollout undo`**: read the Deployment's current
//! revision, pick the ReplicaSet carrying the target revision, and merge-patch its pod template
//! back onto the Deployment. The API has no "undo" endpoint; inventing one would be a second
//! semantics.

use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{
    CheckOut, RequestOut, RestartIn, RollbackIn, ScaleIn, WriteOut, client, items_of,
    name_for_target, name_of, refusal,
};

/// The annotation `kubectl rollout restart` sets; a restart is exactly this annotation change.
pub const RESTART_ANNOTATION: &str = "kubectl.kubernetes.io/restartedAt";

/// The annotation Kubernetes maintains with the Deployment's revision number.
pub const REVISION_ANNOTATION: &str = "deployment.kubernetes.io/revision";

const DEPLOYMENTS: &str = "/apis/apps/v1/namespaces";
const REPLICASETS: &str = "/apis/apps/v1/namespaces";

/// `PATCH …/deployments/{name}` for one workload, with the dry-run query the caller chose.
async fn patch(
    namespace: &str,
    deployment: &str,
    dry_run: bool,
    body: &Value,
) -> Result<Value, ActionError> {
    let path = format!("{DEPLOYMENTS}/{namespace}/deployments/{deployment}");
    let mut query: Vec<(&str, String)> = Vec::new();
    if dry_run {
        query.push(("dryRun", "All".to_owned()));
    }
    client()?
        .patch_json(&path, &query, body)
        .await
        .map_err(refusal)
}

/// The requested object's read-back: the Deployment exactly as the API holds it.
async fn read_deployment(namespace: &str, deployment: &str) -> Result<Value, ActionError> {
    client()?
        .get_json(
            &format!("{DEPLOYMENTS}/{namespace}/deployments/{deployment}"),
            &[],
        )
        .await
        .map_err(refusal)
}

/// `spec.replicas` of a Deployment document, when the document carries one.
fn replicas_of(document: &Value) -> Option<u64> {
    document
        .get("spec")
        .and_then(|spec| spec.get("replicas"))
        .and_then(Value::as_u64)
}

/// The revision annotation of a Deployment (or of a ReplicaSet), as a number.
fn revision_of(document: &Value) -> Option<u32> {
    document
        .get("metadata")
        .and_then(|metadata| metadata.get("annotations"))
        .and_then(|annotations| annotations.get(REVISION_ANNOTATION))
        .and_then(Value::as_str)
        .and_then(|text| text.parse().ok())
}

/// The request document a dry run returns and a real call sends.
fn request_out(dry_run: bool, path: &str, body: &Value) -> RequestOut {
    RequestOut {
        method: "PATCH".to_owned(),
        path: path.to_owned(),
        query: if dry_run {
            vec!["dryRun=All".to_owned()]
        } else {
            Vec::new()
        },
        body: body.clone(),
    }
}

/// Every write's target string: the exact object (and level) the approval authorises.
///
/// A restart has no level: the approval authorises "restart this Deployment", not one particular
/// `restartedAt` stamp — the stamp is generated per call and is not part of what was approved.
/// Scale and rollback DO carry a level, because replicas and revisions are the change itself.
fn target_of(operation: &str, namespace: &str, deployment: &str, level: Option<&str>) -> String {
    match level {
        Some(level) => format!("{namespace}/deployment/{deployment}/{operation}/{level}"),
        None => format!("{namespace}/deployment/{deployment}/{operation}"),
    }
}

// The three spec-declared targets `invoke()` computes before any handler runs (#282). Each is
// the same string its handler builds, from the same function — that is what makes "the engine
// gates what the handler does" true by construction rather than by two implementations agreeing.

/// `ops.restart`'s approval target: this Deployment, restarting.
fn target_restart(input: &RestartIn) -> Result<String, String> {
    let namespace = name_for_target("namespace", &input.namespace)?;
    let deployment = name_for_target("deployment", &input.deployment)?;
    Ok(target_of("restart", &namespace, &deployment, None))
}

/// `ops.scale`'s approval target: this Deployment, at this replica count.
fn target_scale(input: &ScaleIn) -> Result<String, String> {
    let namespace = name_for_target("namespace", &input.namespace)?;
    let deployment = name_for_target("deployment", &input.deployment)?;
    let replicas = input.replicas.to_string();
    Ok(target_of("scale", &namespace, &deployment, Some(&replicas)))
}

/// `ops.rollback`'s approval target: this Deployment, back to this revision (or `previous`).
///
/// The level is the revision asked for, or `previous` — the gate must never need a cluster read to
/// know what it authorises.
fn target_rollback(input: &RollbackIn) -> Result<String, String> {
    let namespace = name_for_target("namespace", &input.namespace)?;
    let deployment = name_for_target("deployment", &input.deployment)?;
    let level = input
        .revision
        .map(|revision| revision.to_string())
        .unwrap_or_else(|| "previous".to_owned());
    Ok(target_of("rollback", &namespace, &deployment, Some(&level)))
}

/// `ops.restart` — a pod-template annotation change, the same one `kubectl rollout restart` makes.
#[crate::action(
    name = "ops.restart",
    version = 1,
    kind = "internal",
    scopes = ["ops:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_restart"
)]
async fn ops_restart(
    _tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RestartIn,
) -> Result<HandlerOutput<WriteOut>, ActionError> {
    let namespace = name_of("namespace", &input.namespace)?;
    let deployment = name_of("deployment", &input.deployment)?;
    // #282: the gate is the engine's now — `target_restart` below is the function it calls, and
    // the string here is the same one by construction. The target is DETERMINISTIC: the stamp is
    // generated after it, because what the approval authorises is "restart this Deployment", not
    // one annotation value.
    let target = target_of("restart", &namespace, &deployment, None);
    let restarted_at = Utc::now().to_rfc3339();

    let path = format!("{DEPLOYMENTS}/{namespace}/deployments/{deployment}");
    let mut annotations = serde_json::Map::new();
    annotations.insert(
        RESTART_ANNOTATION.to_owned(),
        Value::String(restarted_at.clone()),
    );
    let body = json!({
        "spec": {"template": {"metadata": {"annotations": Value::Object(annotations)}}}
    });
    let request = request_out(input.dry_run, &path, &body);
    let mut checks = Vec::new();
    if input.dry_run {
        checks.push(CheckOut {
            name: "dry_run".to_owned(),
            ok: true,
            detail: "no write was made; the body above is what would be sent".to_owned(),
        });
    } else {
        patch(&namespace, &deployment, false, &body).await?;
        let observed = read_deployment(&namespace, &deployment).await?;
        let applied = observed
            .get("spec")
            .and_then(|spec| spec.get("template"))
            .and_then(|template| template.get("metadata"))
            .and_then(|metadata| metadata.get("annotations"))
            .and_then(|annotations| annotations.get(RESTART_ANNOTATION))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        checks.push(CheckOut {
            name: "restarted_at_annotation".to_owned(),
            ok: applied == restarted_at,
            detail: format!(
                "spec.template.metadata.annotations[{RESTART_ANNOTATION}] = {applied:?}"
            ),
        });
    }
    Ok(HandlerOutput::ok(WriteOut {
        namespace,
        deployment,
        operation: "restart".to_owned(),
        target,
        approval_id: ctx.approval_id,
        dry_run: input.dry_run,
        request,
        checks,
    }))
}

/// `ops.scale` — set `spec.replicas`.
#[crate::action(
    name = "ops.scale",
    version = 1,
    kind = "internal",
    scopes = ["ops:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_scale"
)]
async fn ops_scale(
    _tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ScaleIn,
) -> Result<HandlerOutput<WriteOut>, ActionError> {
    let namespace = name_of("namespace", &input.namespace)?;
    let deployment = name_of("deployment", &input.deployment)?;
    let replicas = input.replicas.to_string();
    // #282: `target_scale` is the function the engine gated this call with.
    let target = target_of("scale", &namespace, &deployment, Some(&replicas));

    let path = format!("{DEPLOYMENTS}/{namespace}/deployments/{deployment}");
    let body = json!({"spec": {"replicas": input.replicas}});
    let request = request_out(input.dry_run, &path, &body);
    let mut checks = Vec::new();
    if input.dry_run {
        checks.push(CheckOut {
            name: "dry_run".to_owned(),
            ok: true,
            detail: "no write was made; the body above is what would be sent".to_owned(),
        });
    } else {
        patch(&namespace, &deployment, false, &body).await?;
        let observed = read_deployment(&namespace, &deployment).await?;
        let replicas = replicas_of(&observed);
        checks.push(CheckOut {
            name: "spec_replicas".to_owned(),
            ok: replicas == Some(u64::from(input.replicas)),
            detail: format!(
                "spec.replicas = {:?} (asked for {})",
                replicas, input.replicas
            ),
        });
    }
    Ok(HandlerOutput::ok(WriteOut {
        namespace,
        deployment,
        operation: "scale".to_owned(),
        target,
        approval_id: ctx.approval_id,
        dry_run: input.dry_run,
        request,
        checks,
    }))
}

/// The ReplicaSet of one Deployment at a named revision, exactly as the API holds it.
async fn replicaset_at(
    namespace: &str,
    deployment: &str,
    revision: u32,
) -> Result<Value, ActionError> {
    let document = client()?
        .get_json(
            &format!("{REPLICASETS}/{namespace}/replicasets"),
            &[("limit", "500".to_owned())],
        )
        .await
        .map_err(refusal)?;
    items_of(document)
        .into_iter()
        .find(|item| {
            let name_matches = item
                .get("metadata")
                .and_then(|metadata| metadata.get("ownerReferences"))
                .and_then(Value::as_array)
                .map(|owners| {
                    owners.iter().any(|owner| {
                        owner.get("kind").and_then(Value::as_str) == Some("Deployment")
                            && owner.get("name").and_then(Value::as_str) == Some(deployment)
                    })
                })
                .unwrap_or(false);
            name_matches && revision_of(item) == Some(revision)
        })
        .ok_or_else(|| {
            ActionError::new(
                422,
                "revision_not_found",
                format!("no ReplicaSet of deployment {deployment:?} carries revision {revision}"),
            )
        })
}

/// `ops.rollback` — restore the pod template of an earlier revision (`kubectl rollout undo`).
#[crate::action(
    name = "ops.rollback",
    version = 1,
    kind = "internal",
    scopes = ["ops:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_rollback"
)]
async fn ops_rollback(
    _tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RollbackIn,
) -> Result<HandlerOutput<WriteOut>, ActionError> {
    let namespace = name_of("namespace", &input.namespace)?;
    let deployment = name_of("deployment", &input.deployment)?;
    // The level is the revision asked for, or `previous`. The gate must not need a cluster read
    // to know what it authorises, so `previous` is its own documented level.
    let level = input
        .revision
        .map(|revision| revision.to_string())
        .unwrap_or_else(|| "previous".to_owned());
    // #282: `target_rollback` is the function the engine gated this call with.
    let target = target_of("rollback", &namespace, &deployment, Some(&level));

    let current = read_deployment(&namespace, &deployment).await?;
    let current_revision = revision_of(&current).unwrap_or(0);
    let wanted = input
        .revision
        .or_else(|| current_revision.checked_sub(1))
        .ok_or_else(|| {
            ActionError::new(
                422,
                "no_previous_revision",
                format!("deployment {deployment:?} is at revision {current_revision}; nothing to roll back to"),
            )
        })?;
    let replica_set = replicaset_at(&namespace, &deployment, wanted).await?;
    let template = replica_set
        .get("spec")
        .and_then(|spec| spec.get("template"))
        .cloned()
        .ok_or_else(|| {
            ActionError::new(
                502,
                "replicaset_without_template",
                format!("ReplicaSet at revision {wanted} carries no spec.template"),
            )
        })?;
    let body = json!({"spec": {"template": template}});
    let path = format!("{DEPLOYMENTS}/{namespace}/deployments/{deployment}");
    let request = request_out(input.dry_run, &path, &body);
    let mut checks = Vec::new();
    if input.dry_run {
        checks.push(CheckOut {
            name: "dry_run".to_owned(),
            ok: true,
            detail: format!(
                "no write was made; the pod template of revision {wanted} would be restored"
            ),
        });
    } else {
        patch(&namespace, &deployment, false, &body).await?;
        let observed = read_deployment(&namespace, &deployment).await?;
        let restored = observed
            .get("spec")
            .and_then(|spec| spec.get("template"))
            .cloned()
            .unwrap_or(Value::Null);
        checks.push(CheckOut {
            name: "pod_template_restored".to_owned(),
            ok: restored == template,
            detail: format!("spec.template equals the template of revision {wanted}"),
        });
    }
    Ok(HandlerOutput::ok(WriteOut {
        namespace,
        deployment,
        operation: "rollback".to_owned(),
        target,
        approval_id: ctx.approval_id,
        dry_run: input.dry_run,
        request,
        checks,
    }))
}
