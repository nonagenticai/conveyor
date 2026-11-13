//! The HTTP surface.
//!
//! `ROUTES` is the single declaration of what this service exposes: `router()` builds axum from
//! it and `--check-lock` compares it to the captured contract. A route that exists in one and not
//! the other is a failure, so a handler cannot be added or dropped silently.

pub mod admin;
pub mod canary;
pub mod completions;
pub mod health;
pub mod ledger;
pub mod models;
pub mod providers;
pub mod replay;
pub mod whoami;

use axum::Router;
use axum::routing::{get, patch, post};

/// `(METHOD, path)` for every operation this service registers.
pub const ROUTES: &[(&str, &str)] = &[
    ("GET", "/"),
    ("GET", "/health"),
    ("GET", "/health/live"),
    ("GET", "/health/ready"),
    ("GET", "/api/models"),
    ("GET", "/api/providers"),
    ("GET", "/api/providers/{name}/models"),
    ("GET", "/api/whoami"),
    ("POST", "/api/chat/completions"),
    ("GET", "/api/internal/inference-calls"),
    ("GET", "/api/internal/inference-calls/pinning-summary"),
    ("GET", "/api/internal/inference-calls/repeat-groups"),
    ("GET", "/api/internal/inference-calls/summary"),
    ("GET", "/api/internal/inference-calls/{call_id}/capture"),
    ("GET", "/api/internal/served-models/timeline"),
    ("GET", "/api/replay/invocation-log"),
    (
        "PATCH",
        "/api/replay/invocation-log/by-correlation/{correlation_id}",
    ),
    ("PATCH", "/api/replay/invocation-log/{log_id}"),
    ("POST", "/api/canary/promote-prompt"),
    ("POST", "/api/canary/prompt/configure"),
    ("GET", "/api/canary/prompt/{worker_name}"),
    ("POST", "/api/canary/rollback-prompt"),
    ("GET", "/api/admin/catalog"),
    ("GET", "/api/admin/tenants/{tenant_id}/calls"),
    ("GET", "/api/admin/tenants/{tenant_id}/inference-policy"),
    ("PUT", "/api/admin/tenants/{tenant_id}/inference-policy"),
    (
        "POST",
        "/api/admin/tenants/{tenant_id}/inference-policy/activate",
    ),
];

#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "inference-router",
        description = "OpenAI-compatible inference proxy with intelligent routing and failover \
                       across configurable provider backends.",
        version = env!("CARGO_PKG_VERSION")
    ),
    paths(
        health::root,
        health::health,
        health::liveness,
        health::readiness,
        models::list_models,
        providers::list_providers,
        providers::list_provider_models,
        whoami::whoami,
        completions::create_chat_completion,
        ledger::list_inference_calls,
        ledger::pinning_summary,
        ledger::repeat_groups,
        ledger::calls_summary,
        ledger::capture_call,
        ledger::served_models_timeline,
        replay::list_invocation_log,
        replay::patch_invocation_log_by_correlation,
        replay::patch_invocation_log,
        canary::promote_prompt,
        canary::configure_prompt,
        canary::get_prompt_canary,
        canary::rollback_prompt,
        admin::get_catalog,
        admin::list_tenant_calls,
        admin::get_inference_policy,
        admin::put_inference_policy,
        admin::activate_inference_policy
    ),
    components(schemas(
        crate::contract::schemas::Body_activate_inference_policy_api_admin_tenants__tenant_id__inference_policy_activate_post,
        crate::contract::schemas::ChatCompletionRequest,
        crate::contract::schemas::ChatCompletionResponse,
        crate::contract::schemas::ChatMessage,
        crate::contract::schemas::Choice,
        crate::contract::schemas::ConfigurePromptCanaryRequest,
        crate::contract::schemas::ConfigurePromptCanaryResponse,
        crate::contract::schemas::HTTPValidationError,
        health::HealthResponse,
        crate::contract::schemas::InvocationLogListResponse,
        crate::contract::schemas::InvocationLogRow,
        crate::contract::schemas::JsonSchemaSpec,
        health::LivenessResponse,
        crate::contract::schemas::ModelListResponse,
        crate::contract::schemas::ModelObject,
        crate::contract::schemas::PatchInvocationLogRequest,
        crate::contract::schemas::PatchInvocationLogResponse,
        crate::contract::schemas::PromotePromptRequest,
        crate::contract::schemas::PromotePromptResponse,
        crate::contract::schemas::PromptCanaryState,
        crate::contract::schemas::ProviderListResponse,
        crate::contract::schemas::ProviderModelListResponse,
        crate::contract::schemas::ProviderModelObject,
        crate::contract::schemas::ProviderObject,
        health::ReadinessResponse,
        crate::contract::schemas::ResponseFormat,
        crate::contract::schemas::RollbackPromptRequest,
        crate::contract::schemas::RollbackPromptResponse,
        crate::contract::schemas::ToolCall,
        crate::contract::schemas::ToolCallFunction,
        crate::contract::schemas::ToolFunctionSpec,
        crate::contract::schemas::ToolSpec,
        crate::contract::schemas::Usage,
        crate::contract::schemas::ValidationError,
        crate::contract::schemas::WhoamiResponse
    ))
)]
struct ApiDoc;

/// The document the service publishes (and `--check-lock` compares to the capture).
pub fn openapi() -> utoipa::openapi::OpenApi {
    <ApiDoc as utoipa::OpenApi>::openapi()
}

/// The HTTP application the binary serves.
pub fn router() -> Router {
    Router::new()
        .route("/", get(health::root))
        .route("/health", get(health::health))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        .route("/api/models", get(models::list_models))
        .route("/api/providers", get(providers::list_providers))
        .route(
            "/api/providers/{name}/models",
            get(providers::list_provider_models),
        )
        .route("/api/whoami", get(whoami::whoami))
        .route(
            "/api/chat/completions",
            post(completions::create_chat_completion),
        )
        .route(
            "/api/internal/inference-calls",
            get(ledger::list_inference_calls),
        )
        .route(
            "/api/internal/inference-calls/pinning-summary",
            get(ledger::pinning_summary),
        )
        .route(
            "/api/internal/inference-calls/repeat-groups",
            get(ledger::repeat_groups),
        )
        .route(
            "/api/internal/inference-calls/summary",
            get(ledger::calls_summary),
        )
        .route(
            "/api/internal/inference-calls/{call_id}/capture",
            get(ledger::capture_call),
        )
        .route(
            "/api/internal/served-models/timeline",
            get(ledger::served_models_timeline),
        )
        .route(
            "/api/replay/invocation-log",
            get(replay::list_invocation_log),
        )
        .route(
            "/api/replay/invocation-log/by-correlation/{correlation_id}",
            patch(replay::patch_invocation_log_by_correlation),
        )
        .route(
            "/api/replay/invocation-log/{log_id}",
            patch(replay::patch_invocation_log),
        )
        .route("/api/canary/promote-prompt", post(canary::promote_prompt))
        .route(
            "/api/canary/prompt/configure",
            post(canary::configure_prompt),
        )
        .route(
            "/api/canary/prompt/{worker_name}",
            get(canary::get_prompt_canary),
        )
        .route("/api/canary/rollback-prompt", post(canary::rollback_prompt))
        .route("/api/admin/catalog", get(admin::get_catalog))
        .route(
            "/api/admin/tenants/{tenant_id}/calls",
            get(admin::list_tenant_calls),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/inference-policy",
            get(admin::get_inference_policy).put(admin::put_inference_policy),
        )
        .route(
            "/api/admin/tenants/{tenant_id}/inference-policy/activate",
            post(admin::activate_inference_policy),
        )
}
