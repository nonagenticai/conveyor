#![forbid(unsafe_code)]
//! The six `ops.*` reads. Each one names a closed operation; none accepts a URL.

use serde_json::Value;
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{
    CiIn, CiOut, FluxIn, ListIn, ListOut, LogsIn, LogsOut, client, items_of, limit_of, name_of,
    refusal, selector_of, tail_of,
};

/// The query parameters a Kubernetes list read carries.
fn list_query(selector: Option<String>, limit: u32) -> Vec<(&'static str, String)> {
    let mut query = vec![("limit", limit.to_string())];
    if let Some(selector) = selector {
        query.push(("labelSelector", selector));
    }
    query
}

/// Every pod in one namespace.
#[crate::action(
    name = "ops.pods",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant",
    agent_tier = "searchable"
)]
async fn ops_pods(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: ListIn,
) -> Result<HandlerOutput<ListOut>, ActionError> {
    let namespace = name_of("namespace", &input.namespace)?;
    let selector = selector_of(input.label_selector.as_deref())?;
    let client = client()?;
    let document: Value = client
        .get_json(
            &format!("/api/v1/namespaces/{namespace}/pods"),
            &list_query(selector, limit_of(input.limit)),
        )
        .await
        .map_err(refusal)?;
    let items = items_of(document);
    Ok(HandlerOutput::ok(ListOut {
        namespace,
        resource: "pods".to_owned(),
        count: items.len(),
        items,
    }))
}

/// Every deployment in one namespace.
#[crate::action(
    name = "ops.deployments",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant",
    agent_tier = "searchable"
)]
async fn ops_deployments(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: ListIn,
) -> Result<HandlerOutput<ListOut>, ActionError> {
    let namespace = name_of("namespace", &input.namespace)?;
    let selector = selector_of(input.label_selector.as_deref())?;
    let client = client()?;
    let document: Value = client
        .get_json(
            &format!("/apis/apps/v1/namespaces/{namespace}/deployments"),
            &list_query(selector, limit_of(input.limit)),
        )
        .await
        .map_err(refusal)?;
    let items = items_of(document);
    Ok(HandlerOutput::ok(ListOut {
        namespace,
        resource: "deployments".to_owned(),
        count: items.len(),
        items,
    }))
}

/// Every event in one namespace — the "what changed / why did it fail" read.
#[crate::action(
    name = "ops.events",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant",
    agent_tier = "searchable"
)]
async fn ops_events(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: ListIn,
) -> Result<HandlerOutput<ListOut>, ActionError> {
    let namespace = name_of("namespace", &input.namespace)?;
    let selector = selector_of(input.label_selector.as_deref())?;
    let client = client()?;
    let document: Value = client
        .get_json(
            &format!("/api/v1/namespaces/{namespace}/events"),
            &list_query(selector, limit_of(input.limit)),
        )
        .await
        .map_err(refusal)?;
    let items = items_of(document);
    Ok(HandlerOutput::ok(ListOut {
        namespace,
        resource: "events".to_owned(),
        count: items.len(),
        items,
    }))
}

/// One pod's bounded log tail.
#[crate::action(
    name = "ops.logs",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 60000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant",
    agent_tier = "searchable"
)]
async fn ops_logs(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: LogsIn,
) -> Result<HandlerOutput<LogsOut>, ActionError> {
    let namespace = name_of("namespace", &input.namespace)?;
    let pod = name_of("pod", &input.pod)?;
    let container = match input.container.as_deref() {
        Some(container) => Some(name_of("container", container)?),
        None => None,
    };
    let tail = tail_of(input.tail);

    let mut query: Vec<(&'static str, String)> = vec![("tailLines", tail.to_string())];
    if let Some(container) = &container {
        query.push(("container", container.clone()));
    }
    if input.previous.unwrap_or(false) {
        query.push(("previous", "true".to_owned()));
    }

    let client = client()?;
    let log = client
        .get_text(
            &format!("/api/v1/namespaces/{namespace}/pods/{pod}/log"),
            &query,
        )
        .await
        .map_err(refusal)?;
    Ok(HandlerOutput::ok(LogsOut {
        namespace,
        pod,
        container,
        tail,
        log,
    }))
}

/// The estate's reconciliation state: Flux `Kustomization` objects.
#[crate::action(
    name = "ops.flux_status",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant",
    agent_tier = "searchable"
)]
async fn ops_flux_status(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: FluxIn,
) -> Result<HandlerOutput<ListOut>, ActionError> {
    let namespace = name_of("namespace", &input.namespace)?;
    let named = match input.name.as_deref() {
        Some(name) => Some(name_of("name", name)?),
        None => None,
    };
    let client = client()?;
    let base =
        format!("/apis/kustomize.toolkit.fluxcd.io/v1/namespaces/{namespace}/kustomizations");
    let items = match named {
        Some(name) => {
            let document: Value = client
                .get_json(&format!("{base}/{name}"), &[])
                .await
                .map_err(refusal)?;
            vec![document]
        }
        None => {
            let document: Value = client
                .get_json(&base, &[("limit", limit_of(input.limit).to_string())])
                .await
                .map_err(refusal)?;
            items_of(document)
        }
    };
    Ok(HandlerOutput::ok(ListOut {
        namespace,
        resource: "kustomizations".to_owned(),
        count: items.len(),
        items,
    }))
}

/// Recent GitHub Actions runs.
#[crate::action(
    name = "ops.ci_status",
    version = 1,
    kind = "internal",
    scopes = ["integrations:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant",
    agent_tier = "searchable"
)]
async fn ops_ci_status(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: CiIn,
) -> Result<HandlerOutput<CiOut>, ActionError> {
    let config = conveyor_ops::ci::CiConfig::from_env().map_err(refusal)?;
    let limit = input
        .limit
        .unwrap_or(20)
        .clamp(1, conveyor_ops::ci::MAX_LIMIT);
    let runs = conveyor_ops::ci::runs(
        &config,
        input.repo.as_deref(),
        input.branch.as_deref(),
        limit,
    )
    .await
    .map_err(refusal)?;
    Ok(HandlerOutput::ok(CiOut {
        repo: runs.repo,
        branch: runs.branch,
        count: runs.runs.len(),
        runs: runs.runs,
    }))
}
