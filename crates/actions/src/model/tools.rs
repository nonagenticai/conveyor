#![forbid(unsafe_code)]
//! The four read-only architecture tools — `models.traceability`, `.gaps`, `.impact`,
//! `.completeness` (S08 Part B §5.9).
//!
//! All four are computations over `trace_link` and the model graph: no writes, no tables of
//! their own, and no new event types. They read the same [`ModelSnapshot`] every other model
//! path reads, so a tool can never disagree with a publish about what the model contains.
//!
//! Determinism is part of the contract: every list below is sorted (by id, or by distance then
//! id), so two calls against an unchanged graph return the same document in the same order.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use conveyor_domain::model::ModelSnapshot;
use conveyor_store as store;
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::models::read_model;
use super::{
    CompletenessIn, CompletenessOut, ElementTraceOut, GapOut, GapsIn, GapsOut, ImpactIn, ImpactOut,
    ImpactedOut, TraceabilityIn, TraceabilityOut, model_error,
};

/// Element kinds that are expected to name the governed asset they represent.
const GOVERNED_KINDS: [&str; 3] = ["requirement", "principle", "stakeholder"];

async fn snapshot_of(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    model_id: uuid::Uuid,
) -> Result<ModelSnapshot, ActionError> {
    read_model(&mut *tx, ctx, model_id).await?;
    store::model::load_snapshot_tx(&mut *tx, ctx.tenant, model_id)
        .await
        .map_err(|error| model_error(error, ctx.action))
}

/// How many trace links each element has — the coverage report.
#[crate::action(
    name = "models.traceability",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_traceability(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TraceabilityIn,
) -> Result<HandlerOutput<TraceabilityOut>, ActionError> {
    let snapshot = snapshot_of(&mut *tx, &ctx, input.model_id).await?;
    let ids: Vec<uuid::Uuid> = snapshot.elements.iter().map(|element| element.id).collect();
    let counts = store::model::trace_counts_tx(&mut *tx, ctx.tenant, &ids)
        .await
        .map_err(|error| model_error(error, ctx.action))?
        .into_iter()
        .map(|(id, outgoing, incoming)| (id, (outgoing, incoming)))
        .collect::<BTreeMap<_, _>>();

    let mut untraced = 0i64;
    let elements = snapshot
        .elements
        .iter()
        .map(|element| {
            let (outgoing, incoming) = counts.get(&element.id).copied().unwrap_or((0, 0));
            if outgoing == 0 && incoming == 0 {
                untraced += 1;
            }
            ElementTraceOut {
                element_id: element.id.to_string(),
                kind: element.kind.clone(),
                name: element.name.clone(),
                governed_asset_id: element
                    .meta
                    .get("governed_asset_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                outgoing,
                incoming,
            }
        })
        .collect();

    Ok(HandlerOutput::ok(TraceabilityOut {
        model_id: input.model_id.to_string(),
        elements,
        untraced,
    }))
}

/// What the model is missing — isolation, undrawn elements, empty views, unlinked assets.
#[crate::action(
    name = "models.gaps",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_gaps(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: GapsIn,
) -> Result<HandlerOutput<GapsOut>, ActionError> {
    let snapshot = snapshot_of(&mut *tx, &ctx, input.model_id).await?;
    let mut gaps = Vec::new();

    let mut related: BTreeSet<uuid::Uuid> = BTreeSet::new();
    for relationship in &snapshot.relationships {
        related.insert(relationship.source_id);
        related.insert(relationship.target_id);
    }
    let mut drawn: BTreeSet<uuid::Uuid> = BTreeSet::new();
    for view in &snapshot.views {
        for object in &view.objects {
            if let Some(element_id) = object.element_id {
                drawn.insert(element_id);
            }
        }
    }

    for element in &snapshot.elements {
        if !related.contains(&element.id) {
            gaps.push(GapOut {
                code: "isolated_element".to_owned(),
                element_id: Some(element.id.to_string()),
                message: format!("{} is not the end of any relationship", element.name),
            });
        }
        if !drawn.contains(&element.id) {
            gaps.push(GapOut {
                code: "undrawn_element".to_owned(),
                element_id: Some(element.id.to_string()),
                message: format!("{} is not drawn on any view", element.name),
            });
        }
        if GOVERNED_KINDS.contains(&element.kind.as_str())
            && element
                .meta
                .get("governed_asset_id")
                .and_then(serde_json::Value::as_str)
                .is_none()
        {
            gaps.push(GapOut {
                code: "unlinked_governed_kind".to_owned(),
                element_id: Some(element.id.to_string()),
                message: format!(
                    "{} is a {} and names no governed asset",
                    element.name, element.kind
                ),
            });
        }
    }
    for view in &snapshot.views {
        if view.objects.is_empty() {
            gaps.push(GapOut {
                code: "empty_view".to_owned(),
                element_id: None,
                message: format!("view {} has no objects", view.name),
            });
        }
    }
    gaps.sort_by(|a, b| {
        a.code
            .cmp(&b.code)
            .then_with(|| a.element_id.cmp(&b.element_id))
    });

    Ok(HandlerOutput::ok(GapsOut {
        model_id: input.model_id.to_string(),
        gaps,
    }))
}

/// Everything a change to one element would reach, nearest first.
#[crate::action(
    name = "models.impact",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_impact(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ImpactIn,
) -> Result<HandlerOutput<ImpactOut>, ActionError> {
    let snapshot = snapshot_of(&mut *tx, &ctx, input.model_id).await?;
    if snapshot.element(input.element_id).is_none() {
        return Err(ActionError::resource_not_found(
            "such element in this model",
        ));
    }

    // Undirected adjacency: a change reaches anything related to it, whichever way the
    // relationship points.
    let mut neighbours: BTreeMap<uuid::Uuid, Vec<uuid::Uuid>> = BTreeMap::new();
    for relationship in &snapshot.relationships {
        neighbours
            .entry(relationship.source_id)
            .or_default()
            .push(relationship.target_id);
        neighbours
            .entry(relationship.target_id)
            .or_default()
            .push(relationship.source_id);
    }

    let mut distance: BTreeMap<uuid::Uuid, i64> = BTreeMap::new();
    distance.insert(input.element_id, 0);
    let mut queue = VecDeque::from([input.element_id]);
    while let Some(current) = queue.pop_front() {
        let next = distance[&current] + 1;
        for neighbour in neighbours.get(&current).into_iter().flatten() {
            if !distance.contains_key(neighbour) {
                distance.insert(*neighbour, next);
                queue.push_back(*neighbour);
            }
        }
    }
    distance.remove(&input.element_id);

    let mut impacted: Vec<ImpactedOut> = distance
        .into_iter()
        .filter_map(|(id, distance)| {
            snapshot.element(id).map(|element| ImpactedOut {
                element_id: id.to_string(),
                kind: element.kind.clone(),
                name: element.name.clone(),
                distance,
            })
        })
        .collect();
    impacted.sort_by(|a, b| {
        a.distance
            .cmp(&b.distance)
            .then_with(|| a.element_id.cmp(&b.element_id))
    });

    Ok(HandlerOutput::ok(ImpactOut {
        model_id: input.model_id.to_string(),
        element_id: input.element_id.to_string(),
        impacted,
    }))
}

/// How complete the model is — counts plus the share of elements that are related and drawn.
#[crate::action(
    name = "models.completeness",
    version = 1,
    kind = "internal",
    scopes = ["model:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn models_completeness(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CompletenessIn,
) -> Result<HandlerOutput<CompletenessOut>, ActionError> {
    let snapshot = snapshot_of(&mut *tx, &ctx, input.model_id).await?;

    let mut related: BTreeSet<uuid::Uuid> = BTreeSet::new();
    for relationship in &snapshot.relationships {
        related.insert(relationship.source_id);
        related.insert(relationship.target_id);
    }
    let mut drawn: BTreeSet<uuid::Uuid> = BTreeSet::new();
    for view in &snapshot.views {
        for object in &view.objects {
            if let Some(element_id) = object.element_id {
                drawn.insert(element_id);
            }
        }
    }

    let elements = snapshot.elements.len() as i64;
    let isolated = snapshot
        .elements
        .iter()
        .filter(|element| !related.contains(&element.id))
        .count() as i64;
    let undrawn = snapshot
        .elements
        .iter()
        .filter(|element| !drawn.contains(&element.id))
        .count() as i64;
    let covered = snapshot
        .elements
        .iter()
        .filter(|element| related.contains(&element.id) && drawn.contains(&element.id))
        .count() as i64;
    let score = if elements == 0 {
        0.0
    } else {
        covered as f64 / elements as f64
    };

    Ok(HandlerOutput::ok(CompletenessOut {
        model_id: input.model_id.to_string(),
        elements,
        relationships: snapshot.relationships.len() as i64,
        isolated,
        undrawn,
        empty_views: snapshot
            .views
            .iter()
            .filter(|view| view.objects.is_empty())
            .count() as i64,
        score,
    }))
}
