//! `trace_link` — the single writer of the polymorphic trace edge (S07, data-arch §2.7).
//!
//! Both ends are a `(type, id)` pair with no FK, so the domain function resolves each end
//! inside the caller's tenant in the same transaction (CONTRACT §2). The whitelist of types is
//! this module's, not a client's: [`END_TYPES`] is the only source of an end table name, and
//! no caller-supplied string ever reaches the SQL as an identifier.
//!
//! The chain does not stop at the card: SPEC_V2 §4.5 continues card → run → step →
//! artefact/evidence, so [`END_TYPES`] admits those four ends too. `evidence` resolves to the
//! `tool_call` ledger row, not to a table of evidence items: an evidence item is `jsonb`
//! inside an approval or step payload, while the governed call that produced it is the row a
//! screen can address.
//!
//! [`list_trace_links_tx`] takes a [`TraceFilter`] whose predicates may address **either**
//! end, so one function serves both read directions of S07 §5.7: the requirement screen
//! filters the end it anchored the link at, and the card screen filters the other end of the
//! same row. A row is returned by both filters; there is no second read path.
//!
//! [`view_trace_tx`] is the bounded read the governance screen uses for that chain: one
//! requirement or one run in, its cards, runs, steps, artefacts, evidence and incident edges
//! out, with a cap at every hop so one screen load can never read an unbounded set
//! (v1-w2c-5, SPEC_V2 §8.5).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use sqlx::PgConnection;
use uuid::Uuid;

/// The end types a trace link may name, and therefore the only tables this module resolves.
///
/// The name **is** the table: `governed_asset` covers requirements, principles and ADRs (the
/// asset's `kind` distinguishes them); `card` is a work item; `project` is the programme
/// project a gate's evidence traces to (T3); `document` is a published `document` row, so the
/// artefact factory's output can be trace-linked to its project (SPEC_V2 §8.5). `run`, `step`,
/// `artefact` and `evidence` continue that chain past the card (SPEC_V2 §4.5): a `run` is the
/// card's execution, a `step` is one `run_step`, an `artefact` is one `runner_artefact` row,
/// and `evidence` is one governed `tool_call` ledger row. A new end type is a code change
/// here, never an input.
pub const END_TYPES: [&str; 8] = [
    "governed_asset",
    "card",
    "project",
    "document",
    "run",
    "step",
    "artefact",
    "evidence",
];

/// The five edge kinds `trace_link_kind_check` admits (0002_domain.sql).
pub const LINK_KINDS: [&str; 5] = ["traces", "realises", "derives", "satisfies", "conflicts"];

/// A trace edge to create.
#[derive(Clone, Debug)]
pub struct NewTraceLink {
    /// The end the edge starts at; one of [`END_TYPES`].
    pub from_type: String,
    /// The id in that end's table.
    pub from_id: Uuid,
    /// The end the edge points at; one of [`END_TYPES`].
    pub to_type: String,
    /// The id in that end's table.
    pub to_id: Uuid,
    /// One of [`LINK_KINDS`]; `traces` by default.
    pub kind: String,
}

impl NewTraceLink {
    /// An edge of kind `traces`, the S07 §5.7 demonstration.
    pub fn new(
        from_type: impl Into<String>,
        from_id: Uuid,
        to_type: impl Into<String>,
        to_id: Uuid,
    ) -> Self {
        Self {
            from_type: from_type.into(),
            from_id,
            to_type: to_type.into(),
            to_id,
            kind: "traces".to_owned(),
        }
    }
}

/// One `trace_link` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TraceLinkRecord {
    /// The row id.
    pub id: Uuid,
    /// The start end's type.
    pub from_type: String,
    /// The start end's id.
    pub from_id: Uuid,
    /// The pointed-at end's type.
    pub to_type: String,
    /// The pointed-at end's id.
    pub to_id: Uuid,
    /// The edge's meaning.
    pub kind: String,
    /// When the edge was written; `trace_link` is an immutable fact, so there is no
    /// `updated_at`.
    pub created_at: DateTime<Utc>,
}

/// A predicate set for [`list_trace_links_tx`].
///
/// Every field is optional and the set is `AND`ed. Give `from_*` to read the edges a node
/// emits, or `to_*` to read the edges that point at it — the same rows answer both. An id
/// without its type is still a valid filter, though supplying the type uses the index.
#[derive(Clone, Debug, Default)]
pub struct TraceFilter {
    /// The start end's type; one of [`END_TYPES`] when present.
    pub from_type: Option<String>,
    /// The start end's id.
    pub from_id: Option<Uuid>,
    /// The pointed-at end's type; one of [`END_TYPES`] when present.
    pub to_type: Option<String>,
    /// The pointed-at end's id.
    pub to_id: Option<Uuid>,
    /// The edge kind; one of [`LINK_KINDS`] when present.
    pub kind: Option<String>,
}

/// Why a trace-link operation failed.
#[derive(Debug, thiserror::Error)]
pub enum TraceLinkStoreError {
    /// An end type outside [`END_TYPES`].
    #[error(
        "unknown trace end type {0:?}: expected one of governed_asset, card, project, document, \
         run, step, artefact, evidence"
    )]
    UnknownType(String),
    /// An edge kind outside [`LINK_KINDS`].
    #[error(
        "unknown trace kind {0:?}: expected one of traces, realises, derives, satisfies, \
         conflicts"
    )]
    UnknownKind(String),
    /// An end id that does not resolve **inside the caller's tenant**.
    #[error("the {side} end {id} does not exist in this tenant")]
    EndNotFound {
        /// `"from"` or `"to"`.
        side: &'static str,
        /// The id that did not resolve.
        id: Uuid,
    },
    /// The same `(tenant, from, to, kind)` edge already exists.
    #[error("this trace link already exists")]
    Conflict,
    /// A [`TraceRoot`] that does not resolve inside the caller's tenant.
    #[error("the {root} {id} does not exist in this tenant")]
    RootNotFound {
        /// `"requirement"` or `"run"`.
        root: &'static str,
        /// The id that did not resolve.
        id: Uuid,
    },
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The static table name for an end type — the only source of the identifier.
fn end_table(end_type: &str) -> Option<&'static str> {
    match end_type {
        "governed_asset" => Some("governed_asset"),
        "card" => Some("card"),
        "project" => Some("project"),
        "document" => Some("document"),
        "run" => Some("run"),
        "step" => Some("run_step"),
        "artefact" => Some("runner_artefact"),
        "evidence" => Some("tool_call"),
        _ => None,
    }
}

/// Resolve one end inside the caller's tenant, in the caller's transaction.
async fn ensure_end_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    side: &'static str,
    end_type: &str,
    id: Uuid,
) -> Result<(), TraceLinkStoreError> {
    let table =
        end_table(end_type).ok_or_else(|| TraceLinkStoreError::UnknownType(end_type.to_owned()))?;
    let sql = format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE tenant_id = $1 AND id = $2)");
    let present: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    if present {
        Ok(())
    } else {
        Err(TraceLinkStoreError::EndNotFound { side, id })
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

const COLUMNS: &str = "id, from_type, from_id, to_type, to_id, kind, created_at";

/// Insert a trace edge inside the caller's scoped transaction.
///
/// Both ends are resolved with a tenant-scoped `SELECT` **before** the insert: the table has
/// no FK, and even if it had one a single-column FK does not enforce tenancy (CONTRACT §2).
/// A duplicate edge is [`TraceLinkStoreError::Conflict`] (SQLSTATE 23505), not a 500.
pub async fn insert_trace_link_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewTraceLink,
) -> Result<TraceLinkRecord, TraceLinkStoreError> {
    ensure_end_tx(conn, tenant, "from", &input.from_type, input.from_id).await?;
    ensure_end_tx(conn, tenant, "to", &input.to_type, input.to_id).await?;
    if !LINK_KINDS.contains(&input.kind.as_str()) {
        return Err(TraceLinkStoreError::UnknownKind(input.kind.clone()));
    }
    let sql = format!(
        "INSERT INTO trace_link (tenant_id, from_type, from_id, to_type, to_id, kind)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {COLUMNS}"
    );
    let row: TraceLinkRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.from_type)
        .bind(input.from_id)
        .bind(&input.to_type)
        .bind(input.to_id)
        .bind(&input.kind)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| {
            if is_unique_violation(&error) {
                TraceLinkStoreError::Conflict
            } else {
                TraceLinkStoreError::Database(error)
            }
        })?;
    Ok(row)
}

/// Insert a trace edge inside the caller's scoped transaction, tolerating the same edge.
///
/// [`insert_trace_link_tx`] reports a duplicate as [`TraceLinkStoreError::Conflict`], but a
/// unique violation has already aborted the caller's transaction by the time the error reaches
/// it, so a caller that ignores the conflict cannot continue. This writer states the tolerance
/// in the statement instead: `ON CONFLICT ON CONSTRAINT trace_link_tenant_edge_key DO NOTHING`
/// leaves the transaction usable and returns `None` when the edge was already there.
///
/// Both ends are still resolved inside the caller's tenant first (CONTRACT §2), and every other
/// database error is returned unchanged, so a genuine failure is never read as an existing edge.
pub async fn insert_trace_link_if_absent_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewTraceLink,
) -> Result<Option<TraceLinkRecord>, TraceLinkStoreError> {
    ensure_end_tx(conn, tenant, "from", &input.from_type, input.from_id).await?;
    ensure_end_tx(conn, tenant, "to", &input.to_type, input.to_id).await?;
    if !LINK_KINDS.contains(&input.kind.as_str()) {
        return Err(TraceLinkStoreError::UnknownKind(input.kind.clone()));
    }
    let sql = format!(
        "INSERT INTO trace_link (tenant_id, from_type, from_id, to_type, to_id, kind)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT ON CONSTRAINT trace_link_tenant_edge_key DO NOTHING
         RETURNING {COLUMNS}"
    );
    let row: Option<TraceLinkRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(&input.from_type)
        .bind(input.from_id)
        .bind(&input.to_type)
        .bind(input.to_id)
        .bind(&input.kind)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// Delete one trace edge. `false` means it does not exist in this tenant.
pub async fn delete_trace_link_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<bool, TraceLinkStoreError> {
    let rows = sqlx::query("DELETE FROM trace_link WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(rows == 1)
}

/// One page of trace edges matching [`TraceFilter`], newest first.
///
/// Order is `(created_at DESC, id DESC)`: the id is a uuid v7, so it is both the tiebreaker
/// for rows written in one transaction and the stable key a cursor would carry.
pub async fn list_trace_links_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    filter: &TraceFilter,
    limit: i64,
) -> Result<Vec<TraceLinkRecord>, TraceLinkStoreError> {
    if let Some(end_type) = &filter.from_type {
        end_table(end_type).ok_or_else(|| TraceLinkStoreError::UnknownType(end_type.clone()))?;
    }
    if let Some(end_type) = &filter.to_type {
        end_table(end_type).ok_or_else(|| TraceLinkStoreError::UnknownType(end_type.clone()))?;
    }
    if let Some(kind) = &filter.kind
        && !LINK_KINDS.contains(&kind.as_str())
    {
        return Err(TraceLinkStoreError::UnknownKind(kind.clone()));
    }
    let sql = format!(
        "SELECT {COLUMNS}
           FROM trace_link
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR from_type = $2)
            AND ($3::uuid IS NULL OR from_id = $3)
            AND ($4::text IS NULL OR to_type = $4)
            AND ($5::uuid IS NULL OR to_id = $5)
            AND ($6::text IS NULL OR kind = $6)
          ORDER BY created_at DESC, id DESC
          LIMIT $7"
    );
    let rows: Vec<TraceLinkRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(filter.from_type.as_deref())
        .bind(filter.from_id)
        .bind(filter.to_type.as_deref())
        .bind(filter.to_id)
        .bind(filter.kind.as_deref())
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

// ── the bounded governance view (v1-w2c-5, SPEC_V2 §8.5) ───────────────────────────────

/// The root a [`view_trace_tx`] read starts at.
#[derive(Clone, Copy, Debug)]
pub enum TraceRoot {
    /// A requirement: a `governed_asset` row (the view reports its `kind` with its title).
    Requirement(Uuid),
    /// A run: the card's execution; the view walks down from its card.
    Run(Uuid),
}

/// One card in a trace view.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TraceViewCard {
    /// The card id.
    pub id: Uuid,
    /// The card's name.
    pub name: String,
}

/// One run in a trace view.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TraceViewRun {
    /// The run id.
    pub id: Uuid,
    /// The card the run executes; absent for a project-scoped run.
    pub card_id: Option<Uuid>,
    /// The run status.
    pub status: String,
    /// When the run was created.
    pub created_at: DateTime<Utc>,
}

/// One run step in a trace view.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TraceViewStep {
    /// The step id.
    pub id: Uuid,
    /// The run the step belongs to.
    pub run_id: Uuid,
    /// The step's position in the run.
    pub position: i32,
    /// The action the step executes.
    pub action: String,
    /// The step status.
    pub status: String,
}

/// One runner artefact in a trace view.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TraceViewArtefact {
    /// The artefact id.
    pub id: Uuid,
    /// The run the artefact is evidence for — the domain run, not the runner attempt row.
    pub run_id: Uuid,
    /// The artefact's name within the run.
    pub name: String,
    /// SHA-256 of the artefact's bytes.
    pub sha256: String,
    /// The artefact's size in bytes.
    pub bytes: i64,
}

/// One governed tool call — the addressable evidence of a run.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TraceViewEvidence {
    /// The ledger row id.
    pub id: Uuid,
    /// The run the call was made for.
    pub run_id: Option<Uuid>,
    /// The step that made the call.
    pub run_step_id: Option<Uuid>,
    /// The card the call was made for.
    pub card_id: Option<Uuid>,
    /// The canonical tool name (`<toolkit>.<operation>`).
    pub tool_key: String,
    /// The call's outcome (`ok`, `backend_error`, …).
    pub status: String,
    /// The destination's HTTP status, when it answered.
    pub http_status: Option<i32>,
}

/// The root row a trace view was read from.
#[derive(Clone, Debug)]
pub struct TraceViewRoot {
    /// `requirement` or `run`.
    pub root_type: &'static str,
    /// The root row's id.
    pub id: Uuid,
    /// The requirement's title; absent for a run root.
    pub title: Option<String>,
    /// The requirement asset's kind; absent for a run root.
    pub kind: Option<String>,
    /// The run's status; absent for a requirement root.
    pub status: Option<String>,
    /// The run's card; absent for a requirement root or a card-less run.
    pub card_id: Option<Uuid>,
}

/// The bounded governance view of one requirement or run (v1-w2c-5, SPEC_V2 §8.5).
#[derive(Clone, Debug)]
pub struct TraceView {
    /// The root row.
    pub root: TraceViewRoot,
    /// The cards the root reaches: the requirement's ends, or the run's card.
    pub cards: Vec<TraceViewCard>,
    /// The runs on those cards, newest first.
    pub runs: Vec<TraceViewRun>,
    /// The steps of those runs, in position order.
    pub steps: Vec<TraceViewStep>,
    /// The artefacts of those runs, by name.
    pub artefacts: Vec<TraceViewArtefact>,
    /// The governed tool calls of those runs, newest first.
    pub evidence: Vec<TraceViewEvidence>,
    /// Every trace edge incident to a node in this view, newest first.
    pub edges: Vec<TraceLinkRecord>,
    /// True when a hop hit its cap and rows were dropped.
    pub truncated: bool,
}

/// The view's per-hop caps: one screen load never reads an unbounded set.
const VIEW_CARDS: i64 = 100;
/// Runs read per card.
const VIEW_RUNS_PER_CARD: i64 = 50;
/// Steps read per run.
const VIEW_STEPS_PER_RUN: i64 = 100;
/// Artefacts read per run.
const VIEW_ARTEFACTS_PER_RUN: i64 = 100;
/// Tool calls read per run — the append-only ledger grows fastest, so its cap is the largest.
const VIEW_EVIDENCE_PER_RUN: i64 = 200;
/// Trace edges read across the whole view.
const VIEW_EDGES: i64 = 500;

/// A `run` row with its per-card rank, so one statement enforces the per-card cap.
#[derive(sqlx::FromRow)]
struct RankedRunRow {
    rn: i64,
    id: Uuid,
    card_id: Option<Uuid>,
    status: String,
    created_at: DateTime<Utc>,
}

/// A `run_step` row with its per-run rank.
#[derive(sqlx::FromRow)]
struct RankedStepRow {
    rn: i64,
    id: Uuid,
    run_id: Uuid,
    position: i32,
    action: String,
    status: String,
}

/// A `runner_artefact` row with its per-run rank and the domain run it evidences.
#[derive(sqlx::FromRow)]
struct RankedArtefactRow {
    rn: i64,
    id: Uuid,
    run_id: Uuid,
    name: String,
    sha256: String,
    bytes: i64,
}

/// A `tool_call` row with its per-run rank.
#[derive(sqlx::FromRow)]
struct RankedEvidenceRow {
    rn: i64,
    id: Uuid,
    run_id: Option<Uuid>,
    run_step_id: Option<Uuid>,
    card_id: Option<Uuid>,
    tool_key: String,
    status: String,
    http_status: Option<i32>,
}

/// Read the bounded governance view of one requirement or run (v1-w2c-5, SPEC_V2 §8.5).
///
/// The requirement root answers "which cards realise this requirement, and what proves them":
/// the asset's card ends in **either** direction, then each card's runs, each run's steps and
/// its artefacts and governed tool calls. The run root starts at the run's own card and
/// returns the same downward chain; the root run is never omitted even when its card's cap cut
/// it. A root that does not resolve in the caller's tenant is [`TraceLinkStoreError::RootNotFound`],
/// never an empty view.
///
/// Every hop is capped (the `VIEW_*` constants) and `truncated` says whether any cap bit.
/// Edges are read from both directions, so a node filters either end (§5.7).
///
/// `runner_artefact.run_id` names the runner **attempt** (`runner_run`), not the domain run, so
/// an artefact is reached two ways: through the `run.execute` job's payload (the production
/// path — `crates/store/src/run.rs::enqueue_run_execute_tx` writes `{"run_id": …}`), or by id
/// equality when the two rows share an id.
pub async fn view_trace_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    root: TraceRoot,
) -> Result<TraceView, TraceLinkStoreError> {
    let mut truncated = false;

    // The root row, the cards, and (for a run root) the run itself.
    let (view_root, mut cards, root_run) = match root {
        TraceRoot::Requirement(id) => {
            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT title, kind FROM governed_asset WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
            let Some((title, kind)) = row else {
                return Err(TraceLinkStoreError::RootNotFound {
                    root: "requirement",
                    id,
                });
            };
            let mut cards: Vec<TraceViewCard> = sqlx::query_as(
                "SELECT card.id, card.name
                   FROM trace_link link
                   JOIN card ON card.tenant_id = link.tenant_id AND card.id = link.to_id
                  WHERE link.tenant_id = $1
                    AND link.from_type = 'governed_asset' AND link.from_id = $2
                    AND link.to_type = 'card'
                  UNION
                 SELECT card.id, card.name
                   FROM trace_link link
                   JOIN card ON card.tenant_id = link.tenant_id AND card.id = link.from_id
                  WHERE link.tenant_id = $1
                    AND link.to_type = 'governed_asset' AND link.to_id = $2
                    AND link.from_type = 'card'
                  ORDER BY name, id
                  LIMIT $3",
            )
            .bind(tenant.as_uuid())
            .bind(id)
            .bind(VIEW_CARDS + 1)
            .fetch_all(&mut *conn)
            .await?;
            if cards.len() as i64 > VIEW_CARDS {
                truncated = true;
                cards.truncate(VIEW_CARDS as usize);
            }
            (
                TraceViewRoot {
                    root_type: "requirement",
                    id,
                    title: Some(title),
                    kind: Some(kind),
                    status: None,
                    card_id: None,
                },
                cards,
                None,
            )
        }
        TraceRoot::Run(id) => {
            let row: Option<(Option<Uuid>, String, DateTime<Utc>)> = sqlx::query_as(
                "SELECT card_id, status, created_at FROM run WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant.as_uuid())
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
            let Some((card_id, status, created_at)) = row else {
                return Err(TraceLinkStoreError::RootNotFound { root: "run", id });
            };
            let mut cards = Vec::new();
            if let Some(card_id) = card_id {
                let card: Option<TraceViewCard> =
                    sqlx::query_as("SELECT id, name FROM card WHERE tenant_id = $1 AND id = $2")
                        .bind(tenant.as_uuid())
                        .bind(card_id)
                        .fetch_optional(&mut *conn)
                        .await?;
                cards.extend(card);
            }
            (
                TraceViewRoot {
                    root_type: "run",
                    id,
                    title: None,
                    kind: None,
                    status: Some(status.clone()),
                    card_id,
                },
                cards,
                Some(TraceViewRun {
                    id,
                    card_id,
                    status,
                    created_at,
                }),
            )
        }
    };

    let card_ids: Vec<Uuid> = cards.iter().map(|card| card.id).collect();

    // Runs: at most VIEW_RUNS_PER_CARD per card, ranked in the statement.
    let mut runs: Vec<TraceViewRun> = Vec::new();
    if !card_ids.is_empty() {
        let rows: Vec<RankedRunRow> = sqlx::query_as(
            "SELECT rn, id, card_id, status, created_at
               FROM (
                   SELECT run.id, run.card_id, run.status, run.created_at,
                          row_number() OVER (
                              PARTITION BY run.card_id
                              ORDER BY run.created_at DESC, run.id DESC
                          ) AS rn
                     FROM run
                    WHERE run.tenant_id = $1 AND run.card_id = ANY($2)
               ) ranked
              WHERE rn <= $3
              ORDER BY created_at DESC, id DESC",
        )
        .bind(tenant.as_uuid())
        .bind(&card_ids)
        .bind(VIEW_RUNS_PER_CARD + 1)
        .fetch_all(&mut *conn)
        .await?;
        for row in rows {
            if row.rn > VIEW_RUNS_PER_CARD {
                truncated = true;
            } else {
                runs.push(TraceViewRun {
                    id: row.id,
                    card_id: row.card_id,
                    status: row.status,
                    created_at: row.created_at,
                });
            }
        }
    }
    if let Some(root_run) = root_run
        && !runs.iter().any(|run| run.id == root_run.id)
    {
        // The root run is the one row the view may never omit, even when its card is over cap.
        runs.insert(0, root_run);
    }
    let run_ids: Vec<Uuid> = runs.iter().map(|run| run.id).collect();

    // Steps: at most VIEW_STEPS_PER_RUN per run, in position order.
    let mut steps: Vec<TraceViewStep> = Vec::new();
    if !run_ids.is_empty() {
        let rows: Vec<RankedStepRow> = sqlx::query_as(
            "SELECT rn, id, run_id, position, action, status
               FROM (
                   SELECT rs.id, rs.run_id, rs.position, rs.action, rs.status,
                          row_number() OVER (
                              PARTITION BY rs.run_id
                              ORDER BY rs.position, rs.id
                          ) AS rn
                     FROM run_step rs
                    WHERE rs.tenant_id = $1 AND rs.run_id = ANY($2)
               ) ranked
              WHERE rn <= $3
              ORDER BY run_id, position, id",
        )
        .bind(tenant.as_uuid())
        .bind(&run_ids)
        .bind(VIEW_STEPS_PER_RUN + 1)
        .fetch_all(&mut *conn)
        .await?;
        for row in rows {
            if row.rn > VIEW_STEPS_PER_RUN {
                truncated = true;
            } else {
                steps.push(TraceViewStep {
                    id: row.id,
                    run_id: row.run_id,
                    position: row.position,
                    action: row.action,
                    status: row.status,
                });
            }
        }
    }

    // Artefacts: at most VIEW_ARTEFACTS_PER_RUN per run (see `view_trace_tx`'s doc for the two
    // ways a runner artefact belongs to a domain run).
    let mut artefacts: Vec<TraceViewArtefact> = Vec::new();
    if !run_ids.is_empty() {
        let rows: Vec<RankedArtefactRow> = sqlx::query_as(
            "SELECT run.id AS run_id, artefact.rn, artefact.id, artefact.name,
                    artefact.sha256, artefact.bytes
               FROM run
               CROSS JOIN LATERAL (
                   SELECT a.id, a.name, a.sha256, a.bytes,
                          row_number() OVER (ORDER BY a.name, a.id) AS rn
                     FROM runner_artefact a
                    WHERE a.tenant_id = run.tenant_id
                      AND (
                          a.run_id = run.id
                          OR EXISTS (
                              SELECT 1
                                FROM runner_run rr
                                JOIN job ON job.tenant_id = rr.tenant_id AND job.id = rr.job_id
                               WHERE rr.tenant_id = a.tenant_id
                                 AND rr.id = a.run_id
                                 AND job.kind = 'run.execute'
                                 AND job.payload->>'run_id' = run.id::text
                          )
                      )
                    ORDER BY a.name, a.id
                    LIMIT $3
               ) artefact
              WHERE run.tenant_id = $1 AND run.id = ANY($2)
              ORDER BY run.id, artefact.name, artefact.id",
        )
        .bind(tenant.as_uuid())
        .bind(&run_ids)
        .bind(VIEW_ARTEFACTS_PER_RUN + 1)
        .fetch_all(&mut *conn)
        .await?;
        for row in rows {
            if row.rn > VIEW_ARTEFACTS_PER_RUN {
                truncated = true;
            } else {
                artefacts.push(TraceViewArtefact {
                    id: row.id,
                    run_id: row.run_id,
                    name: row.name,
                    sha256: row.sha256,
                    bytes: row.bytes,
                });
            }
        }
    }

    // Evidence: at most VIEW_EVIDENCE_PER_RUN governed tool calls per run, newest first.
    let mut evidence: Vec<TraceViewEvidence> = Vec::new();
    if !run_ids.is_empty() {
        let rows: Vec<RankedEvidenceRow> = sqlx::query_as(
            "SELECT rn, id, run_id, run_step_id, card_id, tool_key, status, http_status
               FROM (
                   SELECT tc.id, tc.run_id, tc.run_step_id, tc.card_id, tc.tool_key, tc.status,
                          tc.http_status,
                          row_number() OVER (
                              PARTITION BY tc.run_id
                              ORDER BY tc.created_at DESC, tc.id DESC
                          ) AS rn
                     FROM tool_call tc
                    WHERE tc.tenant_id = $1 AND tc.run_id = ANY($2)
               ) ranked
              WHERE rn <= $3
              ORDER BY run_id, rn",
        )
        .bind(tenant.as_uuid())
        .bind(&run_ids)
        .bind(VIEW_EVIDENCE_PER_RUN + 1)
        .fetch_all(&mut *conn)
        .await?;
        for row in rows {
            if row.rn > VIEW_EVIDENCE_PER_RUN {
                truncated = true;
            } else {
                evidence.push(TraceViewEvidence {
                    id: row.id,
                    run_id: row.run_id,
                    run_step_id: row.run_step_id,
                    card_id: row.card_id,
                    tool_key: row.tool_key,
                    status: row.status,
                    http_status: row.http_status,
                });
            }
        }
    }

    // Edges among any found node, from either direction, newest first.
    let asset_ids: Vec<Uuid> = match root {
        TraceRoot::Requirement(id) => vec![id],
        TraceRoot::Run(_) => Vec::new(),
    };
    let step_ids: Vec<Uuid> = steps.iter().map(|step| step.id).collect();
    let artefact_ids: Vec<Uuid> = artefacts.iter().map(|artefact| artefact.id).collect();
    let evidence_ids: Vec<Uuid> = evidence.iter().map(|row| row.id).collect();
    let mut edges: Vec<TraceLinkRecord> = sqlx::query_as(
        "SELECT id, from_type, from_id, to_type, to_id, kind, created_at
           FROM trace_link
          WHERE tenant_id = $1
            AND (
                 (from_type = 'governed_asset' AND from_id = ANY($2))
              OR (to_type   = 'governed_asset' AND to_id   = ANY($2))
              OR (from_type = 'card' AND from_id = ANY($3))
              OR (to_type   = 'card' AND to_id   = ANY($3))
              OR (from_type = 'run' AND from_id = ANY($4))
              OR (to_type   = 'run' AND to_id   = ANY($4))
              OR (from_type = 'step' AND from_id = ANY($5))
              OR (to_type   = 'step' AND to_id   = ANY($5))
              OR (from_type = 'artefact' AND from_id = ANY($6))
              OR (to_type   = 'artefact' AND to_id   = ANY($6))
              OR (from_type = 'evidence' AND from_id = ANY($7))
              OR (to_type   = 'evidence' AND to_id   = ANY($7))
            )
          ORDER BY created_at DESC, id DESC
          LIMIT $8",
    )
    .bind(tenant.as_uuid())
    .bind(&asset_ids)
    .bind(&card_ids)
    .bind(&run_ids)
    .bind(&step_ids)
    .bind(&artefact_ids)
    .bind(&evidence_ids)
    .bind(VIEW_EDGES + 1)
    .fetch_all(&mut *conn)
    .await?;
    if edges.len() as i64 > VIEW_EDGES {
        truncated = true;
        edges.truncate(VIEW_EDGES as usize);
    }

    Ok(TraceView {
        root: view_root,
        cards,
        runs,
        steps,
        artefacts,
        evidence,
        edges,
        truncated,
    })
}
