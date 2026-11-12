#![forbid(unsafe_code)]
//! The copilot: conversations, the turn runner and proposals (S10).
//!
//! One action surface (S04's registry, projected by [`conveyor_actions::agent_surface`]), one
//! approval machine (S06), one SSE stream (S02's `event`/`topic`). The copilot adds the conversation
//! and the proposal shape — never a second registry and never a second approval (S10-D1/D2).
//!
//! Routes (§3): `POST /api/copilot/conversations`, `POST …/{id}/turns`, `GET …/{id}/turns` and
//! `GET /api/copilot/actions`.

pub mod agui;
pub mod conversations;
pub mod proposal;
pub mod turns;

use axum::Json;
use conveyor_store::TenantId;
use conveyor_store::copilot::CopilotStoreError;

use crate::error::ApiError;
use crate::extract::{Caller, RequireLicence};

use crate::state::AppState;

/// An open, tenant-scoped transaction; `begin_scoped` sets `app.current_tenant` with `SET LOCAL`,
/// so every statement inside runs under RLS and the value dies at COMMIT (CONTRACT §2).
pub(crate) type ScopedTx = sqlx::Transaction<'static, sqlx::Postgres>;

pub(crate) async fn begin(state: &AppState, tenant: TenantId) -> Result<ScopedTx, ApiError> {
    Ok(conveyor_store::begin_scoped(&state.pool, tenant).await?)
}

impl From<CopilotStoreError> for ApiError {
    fn from(error: CopilotStoreError) -> Self {
        match error {
            CopilotStoreError::ConversationNotFound => {
                ApiError::not_found("no such conversation in this tenant")
            }
            CopilotStoreError::TurnNotFound => ApiError::not_found("no such turn in this tenant"),
            CopilotStoreError::Archived(_) => {
                ApiError::conflict("the conversation is archived; no turn may be appended")
            }
            CopilotStoreError::Vocabulary { what, value } => {
                tracing::error!(what, value, "a stored copilot value does not parse");
                ApiError::internal("the request could not be completed")
            }
            CopilotStoreError::Parent(error) => ApiError::bad_request(error.to_string()),
            CopilotStoreError::Database(error) => ApiError::from(error),
        }
    }
}

/// `GET /api/copilot/actions` — the surface this caller's model may call (S10 §5.2).
///
/// The same projection `/mcp` renders as `tools/list`: an action outside it is absent here and
/// absent there, and calling it is `action-not-found` (S04-D6), never `403`.
#[utoipa::path(
    get,
    path = "/api/copilot/actions",
    tag = "copilot",
    responses(
        (status = 200, description = "The actions this caller's copilot may call, with their hints"),
        (status = 401, description = "No credential, or one that is refused", body = crate::error::ProblemDetail),
        (status = 403, description = "The licence does not permit this tenant", body = crate::error::ProblemDetail)
    )
)]
pub async fn actions(caller: Caller, _licence: RequireLicence) -> Json<serde_json::Value> {
    let entries = conveyor_actions::agent_surface::entries(
        conveyor_actions::registry(),
        caller.principal.kind,
        caller.role,
        caller.channel.as_ref(),
    );
    Json(serde_json::json!({ "actions": entries }))
}
