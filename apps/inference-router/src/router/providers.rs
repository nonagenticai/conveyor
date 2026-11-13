//! `GET /api/providers` and `GET /api/providers/{name}/models` — catalog enumeration for UIs.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::auth;
use crate::catalog::{Protocol, ProviderEntry};
use crate::contract::schemas::{
    HTTPValidationError, ProviderListResponse, ProviderModelListResponse, ProviderModelObject,
    ProviderObject,
};
use crate::routing::Refusal;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/api/providers",
    tag = "providers",
    summary = "List providers",
    params(
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header),
        ("X-Effective-Tenant-Id" = Option<String>, Header),
        ("X-Subject-Id" = Option<String>, Header),
        ("X-Worker-Name" = Option<String>, Header),
        ("X-App-Id" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = ProviderListResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn list_providers(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (identity, effective) = match state.caller(&headers).await {
        Ok(caller) => caller,
        Err(refusal) => return refusal.into_response(),
    };
    let commercial_access = auth::has_commercial_access(
        &state.config().commercial_platform_tenant_ids,
        &identity,
        effective.tenant_id.as_deref(),
    );
    let catalog = state.router().catalog();
    // ⚠️ No `enabled` filter here: the reference lists every provider the tenant may be routed
    // to, and a disabled provider is still a name a UI may know. /api/models DOES filter.
    let visible: Vec<&ProviderEntry> = catalog
        .providers
        .iter()
        .filter(|provider| provider.customer_eligible || commercial_access)
        .collect();
    let names: std::collections::HashSet<&str> =
        visible.iter().map(|entry| entry.name.as_str()).collect();
    let data = visible
        .iter()
        .map(|entry| ProviderObject {
            cost_input_per_1m: entry.cost_input_per_1m,
            cost_output_per_1m: entry.cost_output_per_1m,
            default_model: entry.model.clone(),
            name: entry.name.clone(),
            protocol: protocol_name(entry.protocol),
        })
        .collect();
    Json(ProviderListResponse {
        data,
        default_chain: catalog
            .default_chain
            .iter()
            .filter(|name| names.contains(name.as_str()))
            .cloned()
            .collect(),
        object: "list".to_owned(),
    })
    .into_response()
}

fn protocol_name(protocol: Protocol) -> String {
    match protocol {
        Protocol::Openai => "openai",
        Protocol::Anthropic => "anthropic",
    }
    .to_owned()
}

#[utoipa::path(
    get,
    path = "/api/providers/{name}/models",
    tag = "providers",
    summary = "List one provider's models",
    params(
        ("name" = String, Path),
        ("X-Tenant-Id" = Option<String>, Header),
        ("X-License-Id" = Option<String>, Header),
        ("X-Effective-Tenant-Id" = Option<String>, Header),
        ("X-Subject-Id" = Option<String>, Header),
        ("X-Worker-Name" = Option<String>, Header),
        ("X-App-Id" = Option<String>, Header)
    ),
    responses(
        (status = 200, body = ProviderModelListResponse),
        (status = 422, body = HTTPValidationError)
    )
)]
pub async fn list_provider_models(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Response {
    let (identity, effective) = match state.caller(&headers).await {
        Ok(caller) => caller,
        Err(refusal) => return refusal.into_response(),
    };
    let commercial_access = auth::has_commercial_access(
        &state.config().commercial_platform_tenant_ids,
        &identity,
        effective.tenant_id.as_deref(),
    );
    let catalog = state.router().catalog();
    let entry = catalog.providers.iter().find(|provider| {
        provider.name == name && (provider.customer_eligible || commercial_access)
    });
    // A provider this tenant may not use answers exactly like an unknown one, so its existence
    // is not disclosed to a customer.
    let Some(entry) = entry else {
        return Refusal::detail(
            StatusCode::NOT_FOUND,
            format!("Unknown provider: {name}"),
            "unknown_provider",
        )
        .into_response();
    };
    let data = entry
        .model_ids()
        .into_iter()
        .map(|id| ProviderModelObject {
            id,
            object: "model".to_owned(),
        })
        .collect();
    Json(ProviderModelListResponse {
        data,
        object: "list".to_owned(),
        provider: entry.name.clone(),
    })
    .into_response()
}
