//! `GET /api/models` — the tenant-scoped model listing.
//!
//! The listing is derived from the live provider catalog and narrowed by the tenant policy; the
//! handler lands with the auth/policy slice. The route and its contract are fixed here.

use std::collections::HashSet;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::auth;
use crate::catalog::ProviderEntry;
use crate::contract::schemas::{HTTPValidationError, ModelListResponse, ModelObject};
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/api/models",
    tag = "models",
    summary = "List models",
    params(
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header),
        ("X-Effective-Tenant-Id" = Option<String>, Header),
        ("X-Subject-Id" = Option<String>, Header),
        ("X-Worker-Name" = Option<String>, Header),
        ("X-App-Id" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = ModelListResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn list_models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (identity, effective) = match state.caller(&headers).await {
        Ok(caller) => caller,
        Err(refusal) => return refusal.into_response(),
    };
    // Commercial providers are listed only for a PROVEN platform tenant; an asserted header is
    // a customer. One decision, shared with /api/providers.
    let commercial_access = auth::has_commercial_access(
        &state.config().commercial_platform_tenant_ids,
        &identity,
        effective.tenant_id.as_deref(),
    );
    let catalog = state.router().catalog();
    let enabled: Vec<&ProviderEntry> = catalog
        .providers
        .iter()
        .filter(|provider| provider.enabled && (provider.customer_eligible || commercial_access))
        .collect();
    let enabled_names: HashSet<&str> = enabled.iter().map(|entry| entry.name.as_str()).collect();
    let effective_chain: Vec<String> = catalog
        .default_chain
        .iter()
        .filter(|name| enabled_names.contains(name.as_str()))
        .cloned()
        .collect();

    let mut data = Vec::new();
    if !effective_chain.is_empty() {
        data.push(model_object("default", effective_chain));
    }
    // A shared alias is owned by the FIRST provider in catalog order.
    let mut seen: HashSet<String> = HashSet::from(["default".to_owned()]);
    for entry in enabled {
        for model_id in entry.model_ids() {
            if !seen.insert(model_id.clone()) {
                continue;
            }
            data.push(model_object(&model_id, vec![entry.name.clone()]));
        }
    }
    Json(ModelListResponse {
        data,
        object: "list".to_owned(),
    })
    .into_response()
}

fn model_object(id: &str, provider_chain: Vec<String>) -> ModelObject {
    ModelObject {
        has_grammar: false,
        has_json_schema: false,
        id: id.to_owned(),
        object: "model".to_owned(),
        owned_by: "inference-router".to_owned(),
        provider_chain: Some(provider_chain),
    }
}
