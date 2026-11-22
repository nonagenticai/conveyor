#![forbid(unsafe_code)]
//! HTTP surface: routers, the utoipa document, RFC 9457 `problem+json`, request ids,
//! health, metrics, `/api/me` and the S04 action surface.
//!
//! Every hand-written route is registered through `utoipa_axum::routes!`, so the served
//! router and the OpenAPI document are one object: a handler without `#[utoipa::path]`
//! does not compile in. A plain `Router::merge` would merge routes only and silently drop
//! the second crate's paths from the document — the reason `--print-openapi` would disagree
//! with what serves.
//!
//! The action routes are the one deliberate exception: their literal path items are
//! **generated from the `ActionSpec` registry** (`conveyor_actions::openapi`) rather than
//! written by hand, so there is still exactly one declaration per operation.

pub mod actions;
pub mod board;
pub mod config;
pub mod copilot;
pub mod design;
pub mod documents;
pub mod error;
pub mod extract;
pub mod job_routes;
pub mod metrics;
pub mod models;
pub mod request_id;
pub mod routes;
pub mod services;
pub mod state;
mod web;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub use config::{Config, ConfigError};
pub use error::{ApiError, FieldError, ProblemDetail};
pub use extract::{Caller, RequireLicence};
pub use metrics::Metrics;
pub use state::{AppState, Authenticator};

/// Document-level metadata. `info` is authored here, not taken from Cargo.toml, so the
/// published spec has one owner.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "conveyor",
        version = "0.1.0",
        description = "Tenant-isolated board, governance and processes service."
    ),
    tags(
        (name = "health", description = "Liveness and readiness"),
        (name = "identity", description = "Who the caller is and which tenant it acts in"),
        (name = "realtime", description = "The replaying server-sent event stream"),
        (name = "actions", description = "The action manifest and one invocation per action"),
        (name = "board", description = "The board read projections (GET only)"),
        (name = "channels", description = "Inbound channel callbacks (Telegram)"),
        (name = "copilot", description = "Conversations, turns and proposals"),
        (name = "platform", description = "Super-admin tenant provisioning and recovery"),
        (name = "billing", description = "A tenant's cost and usage, read from billing_service"),
        (name = "jobs", description = "The runner's pod-side byte routes, fenced by a job token")
    )
)]
struct ApiDoc;

/// Every documented hand-written route. `--print-openapi` serialises exactly this, folded
/// with the generated action operations.
fn api_router() -> OpenApiRouter<AppState> {
    // One `routes!` per endpoint. `routes!(a, b, c)` builds ONE MethodRouter from all the
    // handlers, so three GET handlers with three paths overlap and panic at startup.
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes::health::health))
        .routes(routes!(routes::health::live))
        .routes(routes!(routes::health::ready))
        .routes(routes!(routes::me::me))
        .routes(routes!(routes::events::events))
        // A connector callback authenticates with its per-binding secret, never a bearer
        // token: it is deliberately outside `Caller` and the licence gate. One path serves
        // every connector kind (S06d).
        .routes(routes!(routes::channels::channel_inbound))
        .routes(routes!(board::projects))
        .routes(routes!(board::project))
        .routes(routes!(board::board))
        .routes(routes!(board::board_cards))
        .routes(routes!(board::board_webhooks))
        .routes(routes!(board::card))
        .routes(routes!(board::card_comments))
        // The rendered artefacts. Not actions: a browser cannot POST an invocation or send a
        // bearer header (CONTRACT §6), and these share the renderer with `models.render`.
        // One `routes!` per handler — combining GET handlers builds one MethodRouter and the
        // router panics at startup.
        .routes(routes!(documents::document_content))
        // #144 — the runner's pod-side routes. They carry a job token, never a session: a pod has
        // no Caller and no licence, and the fence is the job row itself (ADR 0007 §Decision 4).
        .routes(routes!(job_routes::job_input))
        .routes(routes!(job_routes::job_progress))
        .routes(routes!(job_routes::job_output))
        .routes(routes!(job_routes::job_result))
        .routes(routes!(models::render_model))
        .routes(routes!(models::export_model_xmi))
        // S10 — the copilot. The chat transport is a route, not an action: streaming does not fit
        // an invocation, and that exemption is declared and asserted (S10-D6).
        .routes(routes!(copilot::actions))
        .routes(routes!(copilot::conversations::create))
        .routes(routes!(copilot::conversations::list_turns))
        .routes(routes!(copilot::turns::create))
        // The Platform surface and the billing read. Routes, not actions: provisioning needs
        // a service credential the external-action engine cannot send, and the billing read
        // is a tenant-admin-or-super-admin projection with no invocation semantics.
        .routes(routes!(routes::platform::list))
        .routes(routes!(routes::platform::detail))
        .routes(routes!(routes::platform::create))
        .routes(routes!(routes::platform::invite_admin))
        .routes(routes!(routes::platform::retry))
        .routes(routes!(routes::billing::tenant_billing))
}

/// The OpenAPI document. Needs no environment and no database: this is what makes
/// `--print-openapi` safe to run in a bare workspace (CONTRACT §16).
///
/// The utoipa router produces the base (the hand-written routes); the action operations are
/// folded in from the `ActionSpec` registry as JSON.
pub fn openapi() -> serde_json::Value {
    let (_, spec) = api_router().split_for_parts();
    let base = serde_json::to_value(&spec).expect("the OpenAPI document serializes");
    conveyor_actions::openapi::fold(base, conveyor_actions::registry())
}

/// The HTTP application the binary serves.
///
/// `/metrics` is added after `split_for_parts` on purpose: it is a Prometheus text
/// endpoint, not a JSON operation, and must not appear in the OpenAPI document. The MCP
/// transport is likewise not a REST operation.
pub fn app(state: AppState) -> Router {
    let (router, _) = api_router().split_for_parts();
    let mut router = router
        .route("/api/actions", get(actions::manifest))
        // The `spec_exempt` fallback (S4-D2): an unknown name is `action-not-found`, not a
        // router-level 404, and it is the same handler the literal routes use.
        .route(
            "/api/actions/{name}/invocations",
            post(actions::invoke_http),
        );

    // One literal route per registered action. Static segments win over `{name}`, so the
    // fallback only ever sees a name that is not in the registry.
    for spec in conveyor_actions::registry().iter() {
        router = router.route(&spec.route_path(), post(actions::invoke_http));
    }

    let mcp = Router::new()
        .route_service(
            "/mcp",
            conveyor_actions::mcp::service(conveyor_actions::mcp::McpDeps {
                pool: state.pool.clone(),
                app_key: state.config.app_key.clone(),
            }),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), mcp_auth));
    router = router.merge(mcp);

    router
        .route("/metrics", axum::routing::get(metrics::metrics))
        // The dev cookie, same-origin: the server now serves the UI itself, so the old
        // dev-server proxy's login endpoint has to exist here too. It 404s unless dev auth is
        // enabled (the handler checks).
        .route("/__dev/login", post(web::dev_login))
        // Every unmatched path is a problem document, never axum's empty-bodied 404 — unless
        // this build carries the static UI, in which case a non-API path is the SPA entry.
        .fallback(web::not_found_or_web)
        // axum's default body limit is 2 MiB, which would refuse a legal document upload BEFORE the action
        // ever saw it: the 10 MiB cap travels as base64 in JSON, i.e. ~14 MB of body. The limit here is the
        // transport envelope, not a rule — `documents.create` still enforces the 10 MiB byte cap, and this
        // only makes sure the request reaches it.
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        // Inner of the request-id layer, so the problem body it builds is completed with
        // `instance` and `request_id` on the way out.
        .layer(middleware::from_fn(framework_problems))
        .layer(middleware::from_fn(request_id::layer))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// The 404 fallback: an unmatched path is RFC 9457, with the method and path in `detail`.
async fn not_found(method: Method, uri: Uri) -> ApiError {
    ApiError::not_found(format!("no route for {method} {}", uri.path()))
}

/// Turn the framework's own rejections into `problem+json` (CONTRACT §5: no plain-text or
/// empty-body errors).
///
/// A handler's `ApiError` is already a problem; these responses bypass one:
/// - a **405** from a `MethodRouter` (`Allow` header, plain body),
/// - a **`Query`/`Path` extraction rejection** (400, `text/plain`),
/// - the router's default **404**, if a future router stops using [`not_found`].
///
/// The layer sits inside the request-id middleware, so the body it produces receives
/// `instance` and `request_id` before it is sent.
async fn framework_problems(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    if content_type.is_some_and(|value| value.starts_with("application/problem+json")) {
        return response;
    }
    // A 400 that already carries JSON (rmcp's transport, for one) is not an extractor
    // rejection and is left alone; the Query/Path rejections are `text/plain`.
    let plain = content_type.is_none_or(|value| value.starts_with("text/plain"));
    let error = match status {
        StatusCode::NOT_FOUND => ApiError::not_found(format!("no route for {method} {path}")),
        StatusCode::METHOD_NOT_ALLOWED => ApiError::new(
            status,
            "method_not_allowed",
            format!("{method} is not allowed for {path}"),
        ),
        StatusCode::BAD_REQUEST if plain => {
            ApiError::bad_request("the request parameters could not be parsed")
        }
        _ => return response,
    };
    // RFC 9110 requires `Allow` on a 405; keep the framework's header.
    let allow = response.headers().get(header::ALLOW).cloned();
    let mut rewritten = error.into_response();
    if let Some(allow) = allow {
        rewritten.headers_mut().insert(header::ALLOW, allow);
    }
    rewritten
}

/// The `/mcp` credential gate. It runs BEFORE the rmcp service, so an unresolvable tenant
/// is an HTTP `401` and never reaches `tools/list`; the resolved `Session` is left in the
/// request extensions for the handler.
async fn mcp_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();
    if let Err(error) = ready(&mut parts, state).await {
        return error.into_response();
    }
    next.run(Request::from_parts(parts, body)).await
}

async fn ready(parts: &mut axum::http::request::Parts, state: AppState) -> Result<(), ApiError> {
    use axum::extract::FromRequestParts;
    Caller::from_request_parts(parts, &state).await?;
    RequireLicence::from_request_parts(parts, &state).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::extract::Query;
    use axum::http::Request;
    use tower::ServiceExt;

    /// The error paths with no `AppState`: the fallback and the rewriting layer are
    /// stateless, so the test serves the same router the binary does.
    fn errors_app() -> Router {
        #[derive(serde::Deserialize)]
        struct Params {
            limit: i64,
        }
        async fn query_handler(Query(params): Query<Params>) -> String {
            params.limit.to_string()
        }
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/api/events", get(query_handler))
            .fallback(not_found)
            .layer(middleware::from_fn(framework_problems))
            .layer(middleware::from_fn(request_id::layer))
    }

    async fn problem(request: Request<Body>) -> (StatusCode, serde_json::Value) {
        let response = errors_app().oneshot(request).await.expect("request");
        let status = response.status();
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/problem+json"),
            "{status} must be a problem document"
        );
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).expect("a JSON problem body"),
        )
    }

    #[tokio::test]
    async fn an_unknown_path_is_a_404_problem_with_instance_and_request_id() {
        let (status, body) = problem(
            Request::builder()
                .uri("/nope")
                .header(request_id::HEADER, "req-404")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        for member in [
            "type",
            "title",
            "status",
            "detail",
            "instance",
            "request_id",
        ] {
            assert!(body.get(member).is_some(), "missing {member}: {body}");
        }
        assert_eq!(body["type"], "not_found");
        assert_eq!(body["status"], 404);
        assert_eq!(body["instance"], "/nope");
        assert_eq!(body["request_id"], "req-404");
        assert!(body["detail"].as_str().unwrap().contains("/nope"));
    }

    #[tokio::test]
    async fn a_wrong_method_is_a_405_problem_and_keeps_allow() {
        let response = errors_app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(
            response.headers().get(header::ALLOW).is_some(),
            "RFC 9110 requires Allow on a 405"
        );
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/problem+json")
        );
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["type"], "method_not_allowed");
        assert_eq!(body["status"], 405);
        assert!(body.get("instance").is_some());
        assert!(body.get("request_id").is_some());
    }

    #[tokio::test]
    async fn a_query_rejection_is_a_400_problem_not_plain_text() {
        let (status, body) = problem(
            Request::builder()
                .uri("/api/events?limit=not-a-number")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["type"], "bad_request");
        assert_eq!(body["status"], 400);
        assert_eq!(body["instance"], "/api/events");
    }

    #[test]
    fn the_openapi_document_contains_the_identity_route_and_the_health_routes() {
        let spec = openapi();
        let paths = spec["paths"].as_object().expect("paths");
        for path in [
            "/api/me",
            "/health",
            "/health/live",
            "/health/ready",
            "/api/projects",
            "/api/projects/{id}",
            "/api/boards/{id}",
            "/api/boards/{id}/cards",
            "/api/boards/{id}/webhooks",
            "/api/cards/{id}",
            "/api/cards/{id}/comments",
            "/api/channels/{binding_id}/inbound",
        ] {
            assert!(paths.contains_key(path), "the document is missing {path}");
        }
        assert!(
            !paths.contains_key("/metrics"),
            "/metrics is a text scrape endpoint, not a documented JSON operation"
        );
    }

    #[test]
    fn every_action_has_a_documented_operation() {
        let spec = openapi();
        let registry = conveyor_actions::registry();
        assert_eq!(
             registry.len(),
            527,
            "the live registry: S04–S11 + S16 (216) + S17's 8 benchmark actions (224) + S13's 6 \
             workflows actions (230) + S19 Part B's 7 eval actions (237) + S13's 3 workflow_runs \
             (240) + S15's 3 knowledge/memory actions (243) + S15 step 5's 7 capability actions \
             (250) + the RBAC pass's 7 (roles.*, invites.*, members.remove) (257) + S13 step 8's \
             agent.run and inference.chat (259) + S13 step 9's workflow_traces.export (260) + S20's six \
             eval_climbs.* (266) + S20's eval_runs.regrade (267) + the web-UI lane's runs.get (268) + \
             S13's cards.list (269) + S19 Part C's seven (report, the three eval_audits.*, the two \
             eval_findings.*, export) (276) + S18 step 2's benchmark_runs.trend (277) + T1's 27
             plan-spine actions (378, internal 360) + S13's SOP/map \
             surface (procedures.*, process_maps.*) and S19's eval_grades.* + laya.judge (288) + \
             S13 session 3's four process_maps writes (292) + S06d's seven channels admin actions (299) + \
             #70's five console actions (documents.compose, projects.update, publish.start, \
             publish.status, repositories.update) (304) + #92's card_types.list (305) + T0's fifteen \
             organisation actions (320) + #139's eleven media kinds and the media.capabilities probe \
             (332) + #161's decks.render (333) + #182 slice A's five youtube actions \
             (client_set, client_get, connect, status, disconnect) (338) + slice B's three \
             publishing actions (upload, privacy, retract) (341) + the work store's five \
             (#156: documents.publish, documents.request_publish, documents.revision_list/get/restore) \
             (346) + #174's media.generate_image (347) + #173's produce.speech and media.transcribe \
             (349) + #160's inference_endpoint.get and inference_endpoint.set (351) + T1's 27
             plan-spine actions (378) + #170's six ops.* cluster reads (384) + #171's three
(404) + #282's three approval-policy actions (approval_policies.list, approval_policies.get, approval_policies.set) (407) + T3's six PM actions (pm.charter|plan|get|route_gate|close, changes.request) (413) + #305's fourteen MSP edit/read actions (benefits update/delete/link_list/unlink/transition_list/transition_upsert, tranches update/delete/review_list, cadences update/delete, wbs.delete, registers.delete, stakeholders.profile_delete) (427) + #154's analyse.site (428) + #164's analyse.video (429) + #303's two retirement actions (benefits.retire, tranches.retire) (431) + #295's cards.ledger_append and cards.ledger_list (433) + T9's twenty-six architecture actions (artifacts list/get/entries/links/create/update/approve/supersede/link, repository_search, profile get/tailor, board open/get/decide, contracts request/get/list/approve/supersede, phases_advance, reviews record/waive, maturity assess/plan, skills_assess) (459) + T13's tools.call (460) + #366's two spreadsheet actions (462) + T4's four coach actions (coach.interrupt|triage|remedy|escalate) (466) + T14's three estate-toolkit actions (toolkits.publish, tools.probe, tools.usage) (469) + #385's three \
             architecture reads (maturity_list, skills_list, board_list) (472) + T11's eight metrics actions (metrics.flow, metrics.job, metrics.usage, metrics.programme, metrics.maturity, metrics.reconcile, metrics.threshold_get, metrics.threshold_update) (480) + T16's five procedures actions (procedures.ingest_sop, procedures.ingest_process, procedures.ingest_rdf, procedures.validate, procedures.review_source) (485) + T7's six operations actions (ops.incident|triage|mitigate|postmortem|resolve, ops.flag_flip) (491) + #450's retirement of the worker-graph read, with the four V1 displays served internally (490) + #462's programmes.refresh (491) + #508's six \
             write-path actions (projects.activate|pause|archive|baseline|programme_meta, \
             tranches.authorise) (497) + #511's five \
             rework-path actions (programmes.baseline|baseline_approve|baselines_list, \
             projects.charter, projects.reopen) (502) + #524's closure-request action \
             (projects.close_request) (503) + #526's reviews_get (504), its four SAFe demo and \
             lesson actions (508), its two retro actions (510), its cadences.approve_plan \
             (511) and its tranches.resume (512), plus the five tenant-asset authoring actions \
             (authoring.intake|draft|review|publish, authoring.assets) (517) + T5's ten kata \
             actions (kata.open|observe|analyse|propose|measure|adopt|propagate|close, \
             kata.get|list) (527). \
             The count is asserted here because this crate serves the registry; a lane that adds \
             an action bumps it."
        );
        let paths = spec["paths"].as_object().expect("paths");
        for action in registry.iter() {
            let path = action.route_path();
            assert!(
                paths.contains_key(&path),
                "{path} is not in the served document"
            );
            assert!(
                paths[&path].get("post").is_some(),
                "{path} has no POST operation"
            );
        }
        assert!(paths["/api/actions"].get("get").is_some());
    }
}
