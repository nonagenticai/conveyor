#![forbid(unsafe_code)]
//! The tenant's registered inference endpoint (conveyor#160 P2).
//!
//! The executor resolves this row **before** the platform `CONVEYOR_EXTERNAL_INFERENCE_URL` and
//! falls back to it when the row is absent (P1, `invoke::tenant_service_url`). Registration is a
//! tenant-admin write: `inference_endpoint.set` is gated on `tenants:write`, audits through the
//! outbox, and seals the credential under the existing `inference.token` secret — the value is
//! never returned, never logged and never read back.
//!
//! The destination is ordinary configuration and is safe to show; the credential is not. Both
//! halves are validated: an http(s) URL with a host and no embedded userinfo, query or fragment,
//! because the engine appends the service path to it verbatim.

use conveyor_domain::Version;
use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;

use super::invalid;
use crate::integrations::inference::INFERENCE_TOKEN_SECRET;
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::EmptyIn;

/// The registration document. It has no field that could hold a credential value.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EndpointView {
    /// The registered destination, or `null` when the tenant has registered none.
    pub destination_url: Option<String>,
    /// Whether the sealed `inference.token` exists.
    pub credential_present: bool,
}

/// Register or replace the tenant's inference destination.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetEndpointIn {
    /// The base URL of the tenant's inference service (http or https, no userinfo).
    pub destination_url: String,
    /// A bearer to seal under `inference.token`; omitted leaves the existing credential alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("x-secret" = true))]
    pub credential: Option<String>,
}

/// Register the destination, and optionally the credential, for the caller's tenant.
#[crate::action(
    name = "inference_endpoint.set",
    version = 1,
    kind = "internal",
    scopes = ["tenants:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn set_endpoint(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: SetEndpointIn,
) -> Result<HandlerOutput<EndpointView>, ActionError> {
    let action = "inference_endpoint.set";
    let destination = destination_input(&input.destination_url)?;

    let mut credential_present =
        store::secret::exists_named_tx(&mut *tx, ctx.tenant, INFERENCE_TOKEN_SECRET)
            .await
            .map_err(|error| {
                tracing::error!(%error, action, "check the inference credential");
                ActionError::internal()
            })?;

    let mut credential_updated = false;
    if let Some(credential) = input.credential.as_deref() {
        if credential.trim().is_empty() {
            return Err(invalid(
                "credential",
                "required",
                "must not be empty when present",
            ));
        }
        let existing = store::secret::list_secrets_tx(&mut *tx, ctx.tenant)
            .await
            .map_err(|error| {
                tracing::error!(%error, action, "list the tenant's secrets");
                ActionError::internal()
            })?
            .into_iter()
            .find(|record| record.name == INFERENCE_TOKEN_SECRET);
        match existing {
            Some(record) => {
                let version =
                    Version::new(record.version.max(1) as u64).unwrap_or(Version::INITIAL);
                store::secret::rotate_secret_tx(
                    &mut *tx,
                    ctx.tenant,
                    record.id,
                    version,
                    credential.as_bytes(),
                )
                .await
                .map_err(|error| seal_failure(error, action))?;
            }
            None => {
                store::secret::create_secret_tx(
                    &mut *tx,
                    ctx.tenant,
                    INFERENCE_TOKEN_SECRET,
                    credential.as_bytes(),
                    "tenant",
                )
                .await
                .map_err(|error| seal_failure(error, action))?;
            }
        }
        credential_present = true;
        credential_updated = true;
    }

    store::tenant_inference::set_destination(&mut *tx, ctx.tenant, &destination, ctx.actor_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, action, "write the tenant inference endpoint");
            ActionError::internal()
        })?;

    emit_endpoint_event(&mut *tx, &ctx, &destination, credential_updated).await?;

    Ok(HandlerOutput::ok(EndpointView {
        destination_url: Some(destination),
        credential_present,
    }))
}

/// Read the tenant's registration (the destination and whether a credential exists).
#[crate::action(
    name = "inference_endpoint.get",
    version = 1,
    kind = "internal",
    scopes = ["tenants:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn get_endpoint(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    _input: EmptyIn,
) -> Result<EndpointView, ActionError> {
    let destination = store::tenant_inference::destination(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = "inference_endpoint.get", "read the tenant inference endpoint");
            ActionError::internal()
        })?;
    let credential_present =
        store::secret::exists_named_tx(&mut *tx, ctx.tenant, INFERENCE_TOKEN_SECRET)
            .await
            .map_err(|error| {
                tracing::error!(%error, action = "inference_endpoint.get", "check the inference credential");
                ActionError::internal()
            })?;
    Ok(EndpointView {
        destination_url: destination,
        credential_present,
    })
}

/// The audit event: the destination and whether the credential moved — never the credential.
async fn emit_endpoint_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    destination_url: &str,
    credential_updated: bool,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: format!("tenant_inference_endpoint:{}", ctx.tenant),
        event_type: "inference_endpoint.set".to_owned(),
        aggregate_type: "tenant_inference_endpoint".to_owned(),
        aggregate_id: ctx.tenant.as_uuid(),
        payload: json!({
            "destination_url": destination_url,
            "credential_updated": credential_updated,
        }),
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = "inference_endpoint.set", "append endpoint event");
            ActionError::internal()
        })?;
    Ok(())
}

/// Validate the destination the engine will append a path to.
fn destination_input(raw: &str) -> Result<String, ActionError> {
    let text = raw.trim();
    let url = reqwest::Url::parse(text).map_err(|_| {
        invalid(
            "destination_url",
            "invalid_url",
            "must be an absolute http(s) URL",
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid(
            "destination_url",
            "invalid_url",
            "must use http or https",
        ));
    }
    if url.host_str().is_none() {
        return Err(invalid(
            "destination_url",
            "invalid_url",
            "must name a host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid(
            "destination_url",
            "credential_in_url",
            "must not embed a credential; use the credential field",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid(
            "destination_url",
            "invalid_url",
            "must not carry a query or a fragment",
        ));
    }
    Ok(text.to_owned())
}

/// A sealing failure is a refusal only when the credential raced another writer; everything else
/// is ours and must not leak the value or the database message.
fn seal_failure(error: store::secret::SecretError, action: &str) -> ActionError {
    match error {
        store::secret::SecretError::VersionMismatch { current } => {
            ActionError::precondition(format!("the credential moved to version {current}"))
        }
        other => {
            tracing::error!(%other, action, "seal the inference credential");
            ActionError::internal()
        }
    }
}
