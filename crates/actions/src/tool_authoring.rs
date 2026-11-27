#![forbid(unsafe_code)]
//! T15 — tool authoring and BYO tools (SPEC_V2 §11.3–§11.6, §11.8, D42, D43, D50; issue #86).
//!
//! The platform (T13) can call a published tool; the estate wave (T14) publishes the platform's
//! own toolkits. This module is the third half: a tenant (or a development job) authors a tool.
//!
//! ```text
//! toolkits.import_openapi / tool_authoring.draft / toolkits.discover_mcp
//!        │  governed_asset(kind='tool_draft')
//!        ▼
//! tool_authoring.validate ──► tool_authoring.test ──► tool_authoring.review
//!        │                                                   │
//!        ▼                                                   ▼
//! tool_authoring.publish ── approval gate (human by default) ──► immutable toolkit version
//!        │                                                   │
//!        ▼                                                   ▼
//! tools.dispatch ◄── tools.call (the broker, binding-scoped)   tool_authoring.promote
//! ```
//!
//! What is enforced here, because it is the whole point of the slice:
//!
//! * **Publication refuses an unbounded draft** — wildcard egress, a free-form input schema, a
//!   write with no idempotency declaration. A missing effect class defaults to `read`, which is
//!   the spec's default and the safe one (§11.5 item 1, §11.6).
//! * **Publication is gated** — the tenant's `approval_policy` row for `tool_authoring.publish`
//!   supplies approver kind, quorum and expiry; absent means one human decision (§5.4, D43).
//!   A destructive tool is refused unless a named approver reviewed it and the policy names a
//!   human — an auto policy is not a named approver.
//! * **The publishing job cannot widen its own ceiling** — when the call names its own binding,
//!   the draft's highest effect may not exceed that binding's `effect_ceiling` (§11.6 final
//!   sentence).
//! * **A BYO destination is a registered source, not a URL** — `tools.dispatch` is reachable
//!   only from the broker (`Authority::Internal`), and the broker passes the `tool_source` id the
//!   publication pinned; the URL and the egress allowlist are rows. A call-time `url` member is
//!   refused by the closed `tools.call` schema before any handler runs (§11.3 step 5, D42).
//! * **Promotion is not an admin toggle** — a tenant admin is refused; the request names a
//!   development project whose review gate is approved (§11.5 final sentence).
//!
//! The SQL lives in [`conveyor_store::tool_authoring`]; this module is the projection — the
//! closed input schemas, the validation, the typed refusals and the one destination dispatch.

use chrono::Utc;
use conveyor_domain::development::{ApproverKind, auto_merge_in_force};
use conveyor_store as store;
use conveyor_store::approval_policy::{ApprovalPolicy, ApprovalRequirement};
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use std::borrow::Cow;

use crate::invoke::{ActionError, HandlerCtx, InvokeOutcome, InvokeRequest};
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, AgentTier, Confirm, Effect, FieldError, HandlerRef,
    SchemaRef, Scope, Tenancy,
};

/// The gate every publication request cites and every decision resolves.
pub const PUBLISH_GATE: &str = "tool_authoring.publish";

/// The one action an authored tool dispatches through.
pub const DISPATCH_ACTION: &str = "tools.dispatch";

/// The largest destination body the dispatch passes through (the broker caps again).
pub const MAX_DESTINATION_BYTES: usize = 1024 * 1024;

/// Whether `name` is a governed authoring gate (it may carry an `approval_policy` row).
pub fn is_governed_gate(name: &str) -> bool {
    name == PUBLISH_GATE
}

// ── shared shapes ───────────────────────────────────────────────────────────────────────

fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

fn finding_error(findings: &[Finding]) -> ActionError {
    let errors = findings
        .iter()
        .map(|finding| FieldError {
            field: match &finding.operation {
                Some(operation) => format!("operations.{operation}.{}", finding.field),
                None => finding.field.clone(),
            },
            code: finding.code.clone(),
            detail: finding.detail.clone(),
        })
        .collect();
    ActionError::new(422, "draft_invalid", "the draft cannot be published").with_errors(errors)
}

fn authoring_store_error(
    error: store::tool_authoring::ToolAuthoringStoreError,
    action: &str,
) -> ActionError {
    match error {
        store::tool_authoring::ToolAuthoringStoreError::NotFound(what) => {
            ActionError::resource_not_found(what)
        }
        store::tool_authoring::ToolAuthoringStoreError::DraftPublished => ActionError::new(
            409,
            "draft_published",
            "this draft is published; a changed contract is a new draft",
        ),
        store::tool_authoring::ToolAuthoringStoreError::SourceConflict { key } => ActionError::new(
            409,
            "source_conflict",
            format!("destination {key:?} is registered with a different URL or kind"),
        ),
        store::tool_authoring::ToolAuthoringStoreError::Database(error) => {
            tracing::error!(%error, action, "tool authoring store failure");
            ActionError::internal()
        }
    }
}

/// A validation finding. One per field per operation, so the caller can fix each.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    /// The operation key, when the finding is about one operation.
    pub operation: Option<String>,
    /// The field the finding names.
    pub field: String,
    /// A stable slug (`input_schema_open`, `egress_wildcard`, `idempotency_missing`, …).
    pub code: String,
    /// What is wrong.
    pub detail: String,
}

/// One operation as the draft carries it and the validator sees it.
#[derive(Clone, Debug)]
struct DraftOperation {
    key: String,
    method: Option<String>,
    path: Option<String>,
    mcp_tool: Option<String>,
    summary: String,
    input_schema: Value,
    output_schema: Option<Value>,
    effect: Option<String>,
    idempotency: Option<String>,
    timeout_secs: i32,
    redact: Vec<String>,
    egress: Vec<String>,
}

impl DraftOperation {
    fn to_json(&self) -> Value {
        json!({
            "key": self.key,
            "method": self.method,
            "path": self.path,
            "mcp_tool": self.mcp_tool,
            "summary": self.summary,
            "input_schema": self.input_schema,
            "output_schema": self.output_schema,
            "effect": self.effect,
            "idempotency": self.idempotency,
            "timeout_secs": self.timeout_secs,
            "redact": self.redact,
            "egress": self.egress,
        })
    }

    fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            key: value.get("key")?.as_str()?.to_owned(),
            method: value
                .get("method")
                .and_then(Value::as_str)
                .map(str::to_owned),
            path: value.get("path").and_then(Value::as_str).map(str::to_owned),
            mcp_tool: value
                .get("mcp_tool")
                .and_then(Value::as_str)
                .map(str::to_owned),
            summary: value
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            input_schema: value.get("input_schema").cloned().unwrap_or(Value::Null),
            output_schema: value
                .get("output_schema")
                .filter(|schema| !schema.is_null())
                .cloned(),
            effect: value
                .get("effect")
                .and_then(Value::as_str)
                .map(str::to_owned),
            idempotency: value
                .get("idempotency")
                .and_then(Value::as_str)
                .map(str::to_owned),
            timeout_secs: value
                .get("timeout_secs")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or(30),
            redact: strings(value.get("redact")),
            egress: strings(value.get("egress")),
        })
    }
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The draft's toolkit and source as stored in `meta`.
#[derive(Clone, Debug)]
struct DraftToolkit {
    key: String,
    title: String,
    service: String,
    owner: String,
    auth_adapter: String,
    source_kind: String,
    source_key: String,
    base_url: String,
    credential_alias: Option<String>,
    egress: Vec<String>,
    source_id: Option<Uuid>,
    operations: Vec<DraftOperation>,
}

impl DraftToolkit {
    fn to_meta(&self) -> Value {
        json!({
            "toolkit": {
                "key": self.key,
                "title": self.title,
                "service": self.service,
                "owner": self.owner,
                "auth_adapter": self.auth_adapter,
                "catalog": "tenant",
            },
            "source": {
                "kind": self.source_kind,
                "key": self.source_key,
                "base_url": self.base_url,
                "credential_alias": self.credential_alias,
                "egress": self.egress,
            },
            "source_id": self.source_id,
            "operations": self.operations.iter().map(DraftOperation::to_json).collect::<Vec<_>>(),
        })
    }

    fn from_meta(meta: &Value) -> Result<Self, ActionError> {
        let toolkit = meta.get("toolkit").ok_or_else(|| {
            invalid(
                "draft",
                "invalid_value",
                "the draft carries no toolkit document",
            )
        })?;
        let source = meta.get("source").unwrap_or(&Value::Null);
        let operations = meta
            .get("operations")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(DraftOperation::from_json)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(Self {
            key: toolkit
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            title: toolkit
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            service: toolkit
                .get("service")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            owner: toolkit
                .get("owner")
                .and_then(Value::as_str)
                .unwrap_or("tenant")
                .to_owned(),
            auth_adapter: toolkit
                .get("auth_adapter")
                .and_then(Value::as_str)
                .unwrap_or("bearer")
                .to_owned(),
            source_kind: source
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("openapi")
                .to_owned(),
            source_key: source
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            base_url: source
                .get("base_url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            credential_alias: source
                .get("credential_alias")
                .and_then(Value::as_str)
                .map(str::to_owned),
            egress: strings(source.get("egress")),
            source_id: meta
                .get("source_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok()),
            operations,
        })
    }
}

// ── validation: the publication gate's first lock ───────────────────────────────────────

fn effect_of(operation: &DraftOperation) -> Result<Effect, Finding> {
    match operation.effect.as_deref() {
        None | Some("") => Ok(Effect::Read),
        Some(raw) => Effect::parse(raw).ok_or_else(|| Finding {
            operation: Some(operation.key.clone()),
            field: "effect".to_owned(),
            code: "effect_invalid".to_owned(),
            detail: format!("{raw:?} is not read, write, spend or destructive"),
        }),
    }
}

fn idempotency_of(operation: &DraftOperation, effect: Effect) -> Result<String, Finding> {
    match operation.idempotency.as_deref() {
        Some(raw @ ("required" | "optional" | "forbidden")) => Ok(raw.to_owned()),
        Some(raw) => Err(Finding {
            operation: Some(operation.key.clone()),
            field: "idempotency".to_owned(),
            code: "idempotency_invalid".to_owned(),
            detail: format!("{raw:?} is not required, optional or forbidden"),
        }),
        None if matches!(effect, Effect::Write | Effect::Destructive) => Err(Finding {
            operation: Some(operation.key.clone()),
            field: "idempotency".to_owned(),
            code: "idempotency_missing".to_owned(),
            detail: "a write must declare an idempotency mode (`x-idempotency`)".to_owned(),
        }),
        // A read or a spend with no declaration is the platform default, not a hole: the
        // rule is §11.3's "writes take an idempotency_key", and a read needs none.
        None => Ok("optional".to_owned()),
    }
}

fn closed_object(schema: &Value) -> bool {
    schema.get("type").and_then(Value::as_str) == Some("object")
        && schema.get("additionalProperties").and_then(Value::as_bool) == Some(false)
}

fn host_of(base_url: &str) -> Option<String> {
    let url = reqwest::Url::parse(base_url).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.host_str().map(str::to_owned)
}

fn egress_admits(egress: &[String], host: &str) -> bool {
    egress.iter().any(|entry| entry == host)
}

/// Validate every operation and the source. Publication refuses on any finding; the
/// `tool_authoring.validate` action reports the same list.
fn validate_draft(toolkit: &DraftToolkit) -> (Vec<(String, Effect, String)>, Vec<Finding>) {
    let mut findings = Vec::new();
    let mut resolved = Vec::new();

    let base_url = toolkit.base_url.trim_end_matches('/');
    let host = match host_of(base_url) {
        None => {
            findings.push(Finding {
                operation: None,
                field: "source.base_url".to_owned(),
                code: "base_url_invalid".to_owned(),
                detail: format!("{:?} is not an absolute http(s) base URL", toolkit.base_url),
            });
            None
        }
        Some(host) => Some(host),
    };
    if toolkit.source_kind != "openapi" && toolkit.source_kind != "mcp" {
        findings.push(Finding {
            operation: None,
            field: "source.kind".to_owned(),
            code: "source_kind_invalid".to_owned(),
            detail: "a source is `openapi` or `mcp`".to_owned(),
        });
    }
    if toolkit.operations.is_empty() {
        findings.push(Finding {
            operation: None,
            field: "operations".to_owned(),
            code: "no_operations".to_owned(),
            detail: "a toolkit version carries at least one operation".to_owned(),
        });
    }

    let mut seen = std::collections::BTreeSet::new();
    for operation in &toolkit.operations {
        if !valid_key(&operation.key, "abcdefghijklmnopqrstuvwxyz") {
            findings.push(Finding {
                operation: Some(operation.key.clone()),
                field: "key".to_owned(),
                code: "key_invalid".to_owned(),
                detail: "a tool key is lower_snake/dot, starting with a letter".to_owned(),
            });
        }
        if !seen.insert(operation.key.clone()) {
            findings.push(Finding {
                operation: Some(operation.key.clone()),
                field: "key".to_owned(),
                code: "key_duplicate".to_owned(),
                detail: "two operations share this key".to_owned(),
            });
        }
        match toolkit.source_kind.as_str() {
            "openapi" => {
                if operation.method.is_none() || operation.path.is_none() {
                    findings.push(Finding {
                        operation: Some(operation.key.clone()),
                        field: "operation".to_owned(),
                        code: "operation_incomplete".to_owned(),
                        detail: "an OpenAPI operation names a method and a path".to_owned(),
                    });
                }
            }
            "mcp" if operation.mcp_tool.as_deref().unwrap_or_default().is_empty() => {
                findings.push(Finding {
                    operation: Some(operation.key.clone()),
                    field: "mcp_tool".to_owned(),
                    code: "operation_incomplete".to_owned(),
                    detail: "an MCP operation names the remote tool".to_owned(),
                });
            }
            _ => {}
        }

        let effect = match effect_of(operation) {
            Ok(effect) => effect,
            Err(finding) => {
                findings.push(finding);
                Effect::Read
            }
        };
        let idempotency = match idempotency_of(operation, effect) {
            Ok(idempotency) => idempotency,
            Err(finding) => {
                findings.push(finding);
                "optional".to_owned()
            }
        };

        if !closed_object(&operation.input_schema) {
            findings.push(Finding {
                operation: Some(operation.key.clone()),
                field: "input_schema".to_owned(),
                code: "input_schema_open".to_owned(),
                detail: "the input schema must be a closed object (`additionalProperties: false`)"
                    .to_owned(),
            });
        }

        // The operation's effective egress: its own declaration, else the source's, else the
        // registered host. A wildcard is never an allowlist.
        let mut egress = if operation.egress.is_empty() {
            toolkit.egress.clone()
        } else {
            operation.egress.clone()
        };
        if egress.is_empty()
            && let Some(host) = &host
        {
            egress.push(host.clone());
        }
        if egress.is_empty() {
            findings.push(Finding {
                operation: Some(operation.key.clone()),
                field: "egress".to_owned(),
                code: "egress_wildcard".to_owned(),
                detail: "the operation names no allowed host".to_owned(),
            });
        }
        for entry in &egress {
            if entry.trim().is_empty() || entry.contains('*') {
                findings.push(Finding {
                    operation: Some(operation.key.clone()),
                    field: "egress".to_owned(),
                    code: "egress_wildcard".to_owned(),
                    detail: format!("{entry:?} is not a concrete host"),
                });
            } else if let Some(host) = &host
                && !egress_admits(&egress, host)
            {
                findings.push(Finding {
                    operation: Some(operation.key.clone()),
                    field: "egress".to_owned(),
                    code: "egress_destination_not_allowed".to_owned(),
                    detail: format!("the destination host {host:?} is not in the allowlist"),
                });
                break;
            }
        }

        resolved.push((operation.key.clone(), effect, idempotency));
    }

    (resolved, findings)
}

fn max_effect(resolved: &[(String, Effect, String)]) -> Effect {
    resolved
        .iter()
        .map(|(_, effect, _)| *effect)
        .max_by_key(|effect| effect.rank())
        .unwrap_or(Effect::Read)
}

fn valid_key(key: &str, alphabet: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    alphabet.contains(first)
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '.')
}

// ── OpenAPI 3.1 import ──────────────────────────────────────────────────────────────────

/// The operation key an OpenAPI operation publishes under.
fn operation_key(operation_id: Option<&str>, method: &str, path: &str) -> String {
    let raw = operation_id.map(str::to_owned).unwrap_or_else(|| {
        format!(
            "{}_{}",
            method.to_ascii_lowercase(),
            path.trim_matches('/').replace('/', "_")
        )
    });
    let mut cleaned = String::new();
    for ch in raw.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() || lower == '_' || lower == '.' {
            cleaned.push(lower);
        } else {
            cleaned.push('_');
        }
    }
    while cleaned.contains("__") {
        cleaned = cleaned.replace("__", "_");
    }
    let cleaned = cleaned.trim_matches('_').to_owned();
    let cleaned = if cleaned.is_empty() {
        "operation".to_owned()
    } else {
        cleaned
    };
    if cleaned.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        format!("op_{cleaned}")
    } else {
        cleaned
    }
}

/// The closed empty input schema an operation with no request body carries.
fn empty_input_schema() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

fn parse_operation(
    document: &Value,
    path: &str,
    method: &str,
    operation: &Value,
) -> Result<DraftOperation, ActionError> {
    let operation = crate::ingest::resolve(operation, document, 0);
    let operation_id = operation.get("operationId").and_then(Value::as_str);
    let key = operation_key(operation_id, method, path);
    let input_schema = operation
        .pointer("/requestBody/content/application~1json/schema")
        .cloned()
        .unwrap_or_else(empty_input_schema);
    let output_schema = {
        let schema = crate::ingest::success_schema(&operation);
        if schema == json!({"type": "object"}) {
            None
        } else {
            Some(schema)
        }
    };
    let effect = operation
        .get("x-effect")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let idempotency = operation
        .get("x-idempotency")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let timeout_secs = operation
        .get("x-timeout-seconds")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(30)
        .clamp(1, 300);
    Ok(DraftOperation {
        key,
        method: Some(method.to_ascii_uppercase()),
        path: Some(path.to_owned()),
        mcp_tool: None,
        summary: operation
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        input_schema,
        output_schema,
        effect,
        idempotency,
        timeout_secs,
        redact: strings(operation.get("x-redact")),
        egress: strings(operation.get("x-egress")),
    })
}

/// Parse an OpenAPI 3.x document into draft operations.
fn parse_openapi(document: &Value) -> Result<Vec<DraftOperation>, ActionError> {
    let version = document
        .get("openapi")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !version.starts_with("3.") {
        return Err(invalid(
            "document.openapi",
            "invalid_value",
            "an authored toolkit is an OpenAPI 3.x document",
        ));
    }
    let Some(paths) = document.get("paths").and_then(Value::as_object) else {
        return Err(invalid(
            "document.paths",
            "required",
            "the document names no paths",
        ));
    };
    let mut operations = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    for (path, item) in paths {
        let item = crate::ingest::resolve(item, document, 0);
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(operation) = item.get(method).filter(|value| value.is_object()) else {
                continue;
            };
            let parsed = parse_operation(document, path, method, operation)?;
            if !keys.insert(parsed.key.clone()) {
                return Err(invalid(
                    "document.paths",
                    "duplicate_operation",
                    format!("two operations derive the key {:?}", parsed.key),
                ));
            }
            operations.push(parsed);
        }
    }
    if operations.is_empty() {
        return Err(invalid(
            "document.paths",
            "invalid_value",
            "the document declares no operations",
        ));
    }
    operations.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(operations)
}

// ── views ───────────────────────────────────────────────────────────────────────────────

/// `toolkits.import_openapi` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportOpenApiIn {
    /// The toolkit key (`weather`).
    pub key: String,
    /// The toolkit's title.
    pub title: String,
    /// The destination alias; defaults to `key`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// The owner label; defaults to `tenant`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// The OpenAPI 3.x document, as JSON.
    pub document: Value,
    /// The destination base URL registered and reviewed at publication.
    pub base_url: String,
    /// The sealed secret's name the broker sends as the bearer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_alias: Option<String>,
    /// `bearer` (default), `none` or `sealed-profile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_adapter: Option<String>,
    /// The host allowlist; defaults to the base URL's host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress: Option<Vec<String>>,
}

/// One operation on the wire.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OperationOut {
    /// The operation key.
    pub key: String,
    /// The effect the version will carry (a missing declaration is `read`).
    pub effect: String,
    /// The idempotency mode the version will carry.
    pub idempotency: String,
}

/// `toolkits.import_openapi` answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ImportOpenApiOut {
    /// The draft's governed-asset id.
    pub draft_id: Uuid,
    /// The toolkit key.
    pub key: String,
    /// `draft`.
    pub status: String,
    /// The imported operations with their resolved defaults.
    pub operations: Vec<OperationOut>,
}

/// `tool_authoring.draft` input: an explicit operation list (the tool-request path's writer).
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftIn {
    /// The toolkit key.
    pub key: String,
    /// The toolkit's title.
    pub title: String,
    /// The destination alias; defaults to `key`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// The owner label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// The registered source, for a new source; or `source_id` for an existing one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<DraftSourceIn>,
    /// An existing `tool_source` id (the MCP registration path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<Uuid>,
    /// The operations.
    pub operations: Vec<DraftOperationIn>,
}

/// The source document a draft names.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftSourceIn {
    /// `openapi` or `mcp`.
    pub kind: String,
    /// The destination alias.
    pub key: String,
    /// The registered base URL.
    pub base_url: String,
    /// The sealed secret's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_alias: Option<String>,
    /// The host allowlist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress: Option<Vec<String>>,
}

/// One operation a draft carries.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftOperationIn {
    /// The operation key.
    pub key: String,
    /// The HTTP method, for an OpenAPI operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// The path, for an OpenAPI operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The remote tool name, for an MCP operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_tool: Option<String>,
    /// A summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The closed input schema.
    pub input_schema: Value,
    /// The output schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    /// `read | write | spend | destructive`; absent is `read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    /// `required | optional | forbidden`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<String>,
    /// 1..=300.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<i32>,
    /// The result fields the broker redacts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redact: Option<Vec<String>>,
    /// The operation's host allowlist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress: Option<Vec<String>>,
}

/// `tool_authoring.validate` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateIn {
    /// The draft to validate.
    pub draft_id: Uuid,
}

/// The validation report.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ValidateOut {
    /// The draft.
    pub draft_id: Uuid,
    /// The toolkit key.
    pub key: String,
    /// True when publication would accept the draft.
    pub valid: bool,
    /// The highest effect the published version would carry.
    pub max_effect: String,
    /// Every operation with its resolved defaults.
    pub operations: Vec<OperationOut>,
    /// Every finding; empty when `valid`.
    pub findings: Vec<Finding>,
}

/// A recorded replay for `tool_authoring.test`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplayIn {
    /// The operation key.
    pub operation: String,
    /// The request document the caller would send.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<Value>,
    /// The recorded response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<ReplayResponseIn>,
}

/// The recorded response of a replay.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplayResponseIn {
    /// The status the destination answered.
    pub status: u16,
    /// The body it answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

/// `tool_authoring.test` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TestIn {
    /// The draft to contract-test.
    pub draft_id: Uuid,
    /// The recorded replay.
    pub replay: ReplayIn,
}

/// One contract check.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TestCheck {
    /// What was checked.
    pub check: String,
    /// Whether it held.
    pub ok: bool,
    /// Why it did not.
    pub detail: String,
}

/// `tool_authoring.test` answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TestOut {
    /// The draft.
    pub draft_id: Uuid,
    /// The operation tested.
    pub operation: String,
    /// True when every check held.
    pub passed: bool,
    /// The checks.
    pub checks: Vec<TestCheck>,
}

/// `tool_authoring.review` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewIn {
    /// The draft under review.
    pub draft_id: Uuid,
    /// `approve` or `reject`.
    pub decision: String,
    /// The reviewer's note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The person who reviewed a destructive tool — required for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named_approver: Option<String>,
}

/// `tool_authoring.review` answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReviewOut {
    /// The draft.
    pub draft_id: Uuid,
    /// `in_review` or `archived`.
    pub status: String,
    /// The recorded decision.
    pub decision: String,
    /// The named approver, for a destructive tool.
    pub named_approver: Option<String>,
}

/// `tool_authoring.publish` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublishIn {
    /// The validated draft.
    pub draft_id: Uuid,
    /// The decided gate; absent requests one and returns it pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<Uuid>,
    /// The authoring job's own grant; its ceiling bounds the published effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_id: Option<Uuid>,
}

/// `tool_authoring.publish` answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PublishOut {
    /// The draft.
    pub draft_id: Uuid,
    /// `pending_approval | published | unchanged`.
    pub outcome: String,
    /// The publication gate.
    pub approval_id: Option<Uuid>,
    /// The gate's status after this call.
    pub approval_status: String,
    /// How many decisions the tenant's policy requires.
    pub required_decisions: u16,
    /// `human` or `auto`.
    pub approver_kind: String,
    /// The toolkit version row, when one was written.
    pub toolkit_id: Option<Uuid>,
    /// The toolkit key.
    pub toolkit_key: String,
    /// The version this publish created.
    pub toolkit_version: Option<i64>,
    /// The content hash of the installed operation set.
    pub content_hash: Option<String>,
    /// The highest effect the version carries.
    pub max_effect: String,
}

/// `toolkits.register_mcp` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegisterMcpIn {
    /// The destination alias.
    pub key: String,
    /// The title.
    pub title: String,
    /// The streamable-HTTP MCP endpoint (`https://mcp.example.com/mcp`).
    pub url: String,
    /// The sealed secret's name the broker sends as the bearer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_alias: Option<String>,
    /// `bearer` (default), `none` or `sealed-profile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_adapter: Option<String>,
    /// The host allowlist; defaults to the URL's host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress: Option<Vec<String>>,
}

/// `toolkits.register_mcp` answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RegisterMcpOut {
    /// The registered source.
    pub source_id: Uuid,
    /// The destination alias.
    pub key: String,
    /// `mcp`.
    pub kind: String,
    /// The registered URL.
    pub base_url: String,
    /// The credential alias.
    pub credential_alias: Option<String>,
    /// The host allowlist.
    pub egress: Vec<String>,
    /// `published`.
    pub status: String,
}

/// `toolkits.discover_mcp` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoverMcpIn {
    /// The registered source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<Uuid>,
    /// The registered source, by destination alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_key: Option<String>,
    /// The toolkit key; defaults to the source's alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The toolkit title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// `toolkits.discover_mcp` answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DiscoverMcpOut {
    /// The draft the discovery wrote.
    pub draft_id: Uuid,
    /// The toolkit key.
    pub key: String,
    /// The discovered tool names, in the server's order.
    pub discovered: Vec<String>,
    /// How many operations the draft carries.
    pub operations: usize,
}

/// `tool_authoring.promote` input.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromoteIn {
    /// The published tenant toolkit.
    pub toolkit_id: Uuid,
    /// The development project whose platform review sanctions the promotion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_project_id: Option<Uuid>,
}

/// `tool_authoring.promote` answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PromoteOut {
    /// The request row.
    pub promotion_id: Uuid,
    /// The toolkit.
    pub toolkit_id: Uuid,
    /// The development project.
    pub development_project_id: Uuid,
    /// The approved review gate.
    pub review_approval_id: Uuid,
    /// `requested`.
    pub status: String,
}

// ── the actions ─────────────────────────────────────────────────────────────────────────

/// Import an OpenAPI 3.x document as a tool draft (`governed_asset(kind='tool_draft')`).
#[crate::action(
    name = "toolkits.import_openapi",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn toolkits_import_openapi(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ImportOpenApiIn,
) -> Result<crate::spec::HandlerOutput<ImportOpenApiOut>, ActionError> {
    let key = input.key.trim().to_owned();
    if !valid_key(&key, "abcdefghijklmnopqrstuvwxyz") || key.contains('.') {
        return Err(invalid(
            "key",
            "invalid_value",
            "a toolkit key is lower_snake, starting with a letter",
        ));
    }
    let service = input
        .service
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&key)
        .to_owned();
    let base_url = input.base_url.trim().trim_end_matches('/').to_owned();
    let Some(host) = host_of(&base_url) else {
        return Err(invalid(
            "base_url",
            "invalid_value",
            "the destination must be an absolute http(s) URL",
        ));
    };
    if input.base_url.contains('*') {
        return Err(invalid(
            "base_url",
            "invalid_value",
            "the destination may not be a wildcard",
        ));
    }
    let operations = parse_openapi(&input.document)?;
    let egress = input
        .egress
        .filter(|entries| !entries.is_empty())
        .unwrap_or_else(|| vec![host.clone()]);
    let toolkit = DraftToolkit {
        key: key.clone(),
        title: input.title.trim().to_owned(),
        service: service.clone(),
        owner: input
            .owner
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("tenant")
            .to_owned(),
        auth_adapter: input.auth_adapter.unwrap_or_else(|| "bearer".to_owned()),
        source_kind: "openapi".to_owned(),
        source_key: service,
        base_url,
        credential_alias: input.credential_alias,
        egress,
        source_id: None,
        operations,
    };
    let meta = toolkit.to_meta();
    let body = serde_json::to_string(&input.document).unwrap_or_default();
    let draft = store::tool_authoring::upsert_draft_tx(
        &mut *tx,
        ctx.tenant,
        &store::tool_authoring::NewDraft {
            key: &key,
            title: &toolkit.title,
            body: &body,
            meta,
            author_id: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| authoring_store_error(error, "toolkits.import_openapi"))?;

    let (resolved, _findings) = validate_draft(&toolkit);
    let operations = resolved
        .into_iter()
        .map(|(key, effect, idempotency)| OperationOut {
            key,
            effect: effect.as_str().to_owned(),
            idempotency,
        })
        .collect();
    Ok(crate::spec::HandlerOutput::created(ImportOpenApiOut {
        draft_id: draft.id,
        key,
        status: draft.status,
        operations,
    }))
}

/// Write or replace a draft from an explicit operation list.
#[crate::action(
    name = "tool_authoring.draft",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_authoring_draft(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DraftIn,
) -> Result<crate::spec::HandlerOutput<ValidateOut>, ActionError> {
    let key = input.key.trim().to_owned();
    if !valid_key(&key, "abcdefghijklmnopqrstuvwxyz") || key.contains('.') {
        return Err(invalid(
            "key",
            "invalid_value",
            "a toolkit key is lower_snake, starting with a letter",
        ));
    }
    let (source_kind, source_key, base_url, credential_alias, egress, source_id) =
        match (&input.source, input.source_id) {
            (Some(source), None) => {
                let base_url = source.base_url.trim().trim_end_matches('/').to_owned();
                if host_of(&base_url).is_none() || base_url.contains('*') {
                    return Err(invalid(
                        "source.base_url",
                        "invalid_value",
                        "the destination must be an absolute http(s) URL with no wildcard",
                    ));
                }
                let host = host_of(&base_url).expect("checked above");
                (
                    source.kind.clone(),
                    source.key.clone(),
                    base_url,
                    source.credential_alias.clone(),
                    source.egress.clone().unwrap_or_else(|| vec![host]),
                    None,
                )
            }
            (None, Some(source_id)) => {
                let source = store::tool_authoring::source_tx(&mut *tx, ctx.tenant, source_id)
                    .await
                    .map_err(|error| authoring_store_error(error, "tool_authoring.draft"))?
                    .ok_or_else(|| invalid("source_id", "not_found", "no such source"))?;
                (
                    source.kind,
                    source.key,
                    source.base_url,
                    source.credential_alias,
                    strings(Some(&source.egress)),
                    Some(source.id),
                )
            }
            _ => {
                return Err(invalid(
                    "source",
                    "invalid_value",
                    "a draft names exactly one source (`source` or `source_id`)",
                ));
            }
        };
    let operations = input
        .operations
        .into_iter()
        .map(|operation| DraftOperation {
            key: operation.key,
            method: operation.method,
            path: operation.path,
            mcp_tool: operation.mcp_tool,
            summary: operation.summary.unwrap_or_default(),
            input_schema: operation.input_schema,
            output_schema: operation.output_schema,
            effect: operation.effect,
            idempotency: operation.idempotency,
            timeout_secs: operation.timeout_secs.unwrap_or(30).clamp(1, 300),
            redact: operation.redact.unwrap_or_default(),
            egress: operation.egress.unwrap_or_default(),
        })
        .collect();
    let toolkit = DraftToolkit {
        key: key.clone(),
        title: input.title.trim().to_owned(),
        service: input.service.unwrap_or_else(|| source_key.clone()),
        owner: input.owner.unwrap_or_else(|| "tenant".to_owned()),
        auth_adapter: "bearer".to_owned(),
        source_kind,
        source_key,
        base_url,
        credential_alias,
        egress,
        source_id,
        operations,
    };
    let meta = toolkit.to_meta();
    let draft = store::tool_authoring::upsert_draft_tx(
        &mut *tx,
        ctx.tenant,
        &store::tool_authoring::NewDraft {
            key: &key,
            title: &toolkit.title,
            body: "",
            meta,
            author_id: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| authoring_store_error(error, "tool_authoring.draft"))?;
    let out = validate_out(draft.id, &toolkit);
    Ok(crate::spec::HandlerOutput::created(out))
}

fn validate_out(draft_id: Uuid, toolkit: &DraftToolkit) -> ValidateOut {
    let (resolved, findings) = validate_draft(toolkit);
    let max = max_effect(&resolved);
    ValidateOut {
        draft_id,
        key: toolkit.key.clone(),
        valid: findings.is_empty(),
        max_effect: max.as_str().to_owned(),
        operations: resolved
            .into_iter()
            .map(|(key, effect, idempotency)| OperationOut {
                key,
                effect: effect.as_str().to_owned(),
                idempotency,
            })
            .collect(),
        findings,
    }
}

async fn load_toolkit(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    draft_id: Uuid,
) -> Result<(store::tool_authoring::DraftRecord, DraftToolkit), ActionError> {
    let draft = store::tool_authoring::draft_tx(&mut *tx, ctx.tenant, draft_id)
        .await
        .map_err(|error| authoring_store_error(error, "tool draft"))?
        .ok_or_else(|| ActionError::resource_not_found("tool draft"))?;
    let toolkit = DraftToolkit::from_meta(&draft.meta)?;
    Ok((draft, toolkit))
}

/// Validate a draft: the closed schema, the effect/idempotency rules and the egress.
#[crate::action(
    name = "tool_authoring.validate",
    version = 1,
    kind = "internal",
    scopes = ["tools:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_authoring_validate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ValidateIn,
) -> Result<ValidateOut, ActionError> {
    let (draft, toolkit) = load_toolkit(&mut *tx, &ctx, input.draft_id).await?;
    Ok(validate_out(draft.id, &toolkit))
}

/// Contract-test a draft against a recorded replay.
#[crate::action(
    name = "tool_authoring.test",
    version = 1,
    kind = "internal",
    scopes = ["tools:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_authoring_test(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TestIn,
) -> Result<TestOut, ActionError> {
    let (draft, toolkit) = load_toolkit(&mut *tx, &ctx, input.draft_id).await?;
    let (resolved, findings) = validate_draft(&toolkit);
    let mut checks = vec![TestCheck {
        check: "draft_validates".to_owned(),
        ok: findings.is_empty(),
        detail: if findings.is_empty() {
            String::new()
        } else {
            findings
                .iter()
                .map(|finding| format!("{}: {}", finding.field, finding.detail))
                .collect::<Vec<_>>()
                .join("; ")
        },
    }];
    let operation = toolkit
        .operations
        .iter()
        .find(|operation| operation.key == input.replay.operation);
    match operation {
        None => checks.push(TestCheck {
            check: "operation_exists".to_owned(),
            ok: false,
            detail: format!("the draft has no operation {:?}", input.replay.operation),
        }),
        Some(operation) => {
            checks.push(TestCheck {
                check: "operation_exists".to_owned(),
                ok: true,
                detail: String::new(),
            });
            if let Some(request) = &input.replay.request {
                let errors = crate::spec::validate_input(&operation.input_schema, request);
                checks.push(TestCheck {
                    check: "request_matches_closed_schema".to_owned(),
                    ok: errors.is_empty(),
                    detail: errors
                        .iter()
                        .map(|error| format!("{}: {}", error.field, error.detail))
                        .collect::<Vec<_>>()
                        .join("; "),
                });
            }
            if let Some(response) = &input.replay.response {
                let status_ok = (200..300).contains(&response.status);
                checks.push(TestCheck {
                    check: "replay_status_is_success".to_owned(),
                    ok: status_ok,
                    detail: if status_ok {
                        String::new()
                    } else {
                        format!("the recorded replay answered {}", response.status)
                    },
                });
                if let (Some(schema), Some(body)) = (&operation.output_schema, &response.body) {
                    let errors = crate::spec::validate_input(schema, body);
                    checks.push(TestCheck {
                        check: "replay_body_matches_output_schema".to_owned(),
                        ok: errors.is_empty(),
                        detail: errors
                            .iter()
                            .map(|error| format!("{}: {}", error.field, error.detail))
                            .collect::<Vec<_>>()
                            .join("; "),
                    });
                }
            }
        }
    }
    let _ = resolved;
    let passed = checks.iter().all(|check| check.ok);
    Ok(TestOut {
        draft_id: draft.id,
        operation: input.replay.operation,
        passed,
        checks,
    })
}

/// Record the security review of a draft. A destructive tool needs a named approver.
#[crate::action(
    name = "tool_authoring.review",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_authoring_review(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ReviewIn,
) -> Result<ReviewOut, ActionError> {
    let (draft, toolkit) = load_toolkit(&mut *tx, &ctx, input.draft_id).await?;
    if draft.status == "approved" {
        return Err(ActionError::new(
            409,
            "draft_published",
            "a published draft cannot be reviewed again",
        ));
    }
    let decision = input.decision.trim();
    if decision != "approve" && decision != "reject" {
        return Err(invalid(
            "decision",
            "invalid_value",
            "the review decision is `approve` or `reject`",
        ));
    }
    let (resolved, findings) = validate_draft(&toolkit);
    let max = max_effect(&resolved);
    let named = input
        .named_approver
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if decision == "approve" && max == Effect::Destructive && named.is_none() {
        return Err(ActionError::new(
            422,
            "approver_required",
            "a destructive tool needs a named approver at review",
        ));
    }
    let mut meta = draft.meta.clone();
    if let Some(object) = meta.as_object_mut() {
        object.insert(
            "review".to_owned(),
            json!({
                "decision": decision,
                "note": input.note.unwrap_or_default(),
                "named_approver": named,
                "reviewer_id": ctx.actor_id,
                "reviewed_at": Utc::now(),
                "findings": findings.len(),
            }),
        );
    }
    let status = if decision == "approve" {
        "in_review"
    } else {
        "archived"
    };
    let record = store::tool_authoring::set_draft_status_tx(
        &mut *tx,
        ctx.tenant,
        draft.id,
        status,
        Some(&meta),
        &format!("review:{decision}"),
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| authoring_store_error(error, "tool_authoring.review"))?;
    Ok(ReviewOut {
        draft_id: record.id,
        status: record.status,
        decision: decision.to_owned(),
        named_approver: named,
    })
}

fn policy_of(policy: Option<ApprovalPolicy>) -> ApprovalPolicy {
    policy.unwrap_or_else(|| ApprovalPolicy::default_for(PUBLISH_GATE))
}

/// The publication gate's terms as the tenant configured them.
fn publication_policy(
    policy: &ApprovalPolicy,
) -> Result<(u16, Option<chrono::DateTime<Utc>>), ActionError> {
    if policy.required_decisions == 0 || policy.required_decisions > 255 {
        return Err(ActionError::new(
            500,
            "policy_invalid",
            "the publication policy's required_decisions is outside 1..=255",
        ));
    }
    Ok((policy.required_decisions, policy.expires_at))
}

fn auto_publication_admits(policy: &ApprovalPolicy, max: Effect) -> bool {
    if policy.approver_kind != ApproverKind::Auto {
        return false;
    }
    if !auto_merge_in_force(policy.expires_at, Utc::now()) {
        return false;
    }
    let allowed = policy
        .conditions
        .get("max_effect")
        .and_then(Value::as_str)
        .and_then(Effect::parse)
        .unwrap_or(Effect::Read);
    max.rank() <= allowed.rank()
}

/// Publish a validated draft behind the tenant's publication gate.
#[crate::action(
    name = "tool_authoring.publish",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_authoring_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PublishIn,
) -> Result<PublishOut, ActionError> {
    let (draft, toolkit) = load_toolkit(&mut *tx, &ctx, input.draft_id).await?;

    // A draft publishes once: a second call answers `unchanged` rather than bumping a version.
    if let Some((toolkit_id, toolkit_key, toolkit_version, meta)) =
        store::tool_authoring::published_for_draft_tx(&mut *tx, ctx.tenant, draft.id)
            .await
            .map_err(|error| authoring_store_error(error, "tool_authoring.publish"))?
    {
        return Ok(PublishOut {
            draft_id: draft.id,
            outcome: "unchanged".to_owned(),
            approval_id: None,
            approval_status: "approved".to_owned(),
            required_decisions: 1,
            approver_kind: "human".to_owned(),
            toolkit_id: Some(toolkit_id),
            toolkit_key,
            toolkit_version: Some(toolkit_version),
            content_hash: meta
                .get("content_hash")
                .and_then(Value::as_str)
                .map(str::to_owned),
            max_effect: meta
                .get("max_effect")
                .and_then(Value::as_str)
                .unwrap_or("read")
                .to_owned(),
        });
    }

    let (resolved, findings) = validate_draft(&toolkit);
    if !findings.is_empty() {
        return Err(finding_error(&findings));
    }
    let max = max_effect(&resolved);

    // P4.1 — the publishing job publishes only inside its own binding's ceiling.
    if let Some(binding_id) = input.binding_id {
        let grant = store::tool_authoring::binding_grant_tx(&mut *tx, ctx.tenant, binding_id)
            .await
            .map_err(|error| authoring_store_error(error, "tool_authoring.publish"))?
            .ok_or_else(|| ActionError::resource_not_found("tool binding"))?;
        if !grant.enabled {
            return Err(ActionError::new(
                403,
                "binding_disabled",
                "the authoring job's binding is disabled",
            ));
        }
        let ceiling = Effect::parse(&grant.effect_ceiling).ok_or_else(|| {
            ActionError::new(
                500,
                "binding_invalid",
                "the authoring job's binding carries an unknown ceiling",
            )
        })?;
        if max.rank() > ceiling.rank() {
            return Err(ActionError::new(
                403,
                "effect_ceiling",
                format!(
                    "this binding allows at most {} and the draft publishes a {} tool",
                    ceiling.as_str(),
                    max.as_str()
                ),
            ));
        }
    }

    let policy = policy_of(
        store::approval_policy::get(&mut *tx, ctx.tenant, PUBLISH_GATE)
            .await
            .map_err(|error| {
                tracing::error!(%error, "read the publication policy");
                ActionError::internal()
            })?,
    );
    let (required_decisions, expires_at) = publication_policy(&policy)?;

    // P2.3 — a destructive tool needs a named approver, and an auto/waived policy is not one.
    if max == Effect::Destructive {
        let named = draft
            .meta
            .pointer("/review/named_approver")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        if !named {
            return Err(ActionError::new(
                422,
                "approver_required",
                "a destructive tool needs a named approver at review",
            ));
        }
        if policy.approval == ApprovalRequirement::Waived
            || policy.approver_kind != ApproverKind::Human
        {
            return Err(ActionError::new(
                422,
                "approver_required",
                "a destructive tool needs a policy that names a human approver",
            ));
        }
    }

    let gate_meta = json!({
        "target": draft.id.to_string(),
        "draft_id": draft.id.to_string(),
        "toolkit_key": toolkit.key,
        "max_effect": max.as_str(),
    });
    // A caller that names a decided gate is continuing a request; the gate must be THIS
    // draft's publication gate, whatever its status. A caller that names none is asking for
    // one, and gets the pending row (the unique index keeps it single).
    let (mut record, _created) = match input.approval_id {
        Some(approval_id) => {
            let record = store::approval::get_approval_tx(&mut *tx, ctx.tenant, approval_id)
                .await
                .map_err(|error| {
                    tracing::error!(%error, "read the named approval");
                    ActionError::internal()
                })?
                .ok_or_else(|| {
                    ActionError::new(
                        404,
                        "approval_not_found",
                        format!("no approval {approval_id} in this tenant"),
                    )
                })?;
            let named_draft = record
                .meta
                .get("draft_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if record.approval.gate != PUBLISH_GATE
                || record.approval.kind != store::tool_authoring::GATE_KIND
                || named_draft != draft.id.to_string()
            {
                return Err(ActionError::new(
                    403,
                    "approval_wrong_gate",
                    format!("approval {approval_id} is not this draft's publication gate"),
                ));
            }
            (record, false)
        }
        None => store::tool_authoring::request_publication_gate_tx(
            &mut *tx,
            ctx.tenant,
            draft.id,
            required_decisions,
            Some(ctx.actor_id),
            expires_at,
            &gate_meta,
        )
        .await
        .map_err(|error| authoring_store_error(error, "tool_authoring.publish"))?,
    };

    // The earned auto path: only a non-destructive draft inside the policy's terms, and the
    // decision row says `rule` because a policy has no principal.
    if record.approval.status == conveyor_domain::ApprovalStatus::Pending
        && max != Effect::Destructive
        && auto_publication_admits(&policy, max)
    {
        let policy_document = json!({
            "action": PUBLISH_GATE,
            "approver_kind": policy.approver_kind.as_str(),
            "conditions": policy.conditions,
            "expires_at": policy.expires_at,
        });
        store::tool_authoring::auto_approve_tx(
            &mut *tx,
            ctx.tenant,
            record.approval.id,
            ctx.actor_id,
            &policy_document,
        )
        .await
        .map_err(|error| authoring_store_error(error, "tool_authoring.publish"))?;
        record = store::approval::get_approval_tx(&mut *tx, ctx.tenant, record.approval.id)
            .await
            .map_err(|_| ActionError::internal())?
            .ok_or_else(ActionError::internal)?;
    }

    if matches!(
        record.approval.status,
        conveyor_domain::ApprovalStatus::Rejected
            | conveyor_domain::ApprovalStatus::Expired
            | conveyor_domain::ApprovalStatus::Voided
    ) {
        return Err(ActionError::new(
            403,
            "approval_not_approved",
            format!(
                "the publication gate is {}; a new draft is needed",
                record.approval.status.as_str()
            ),
        ));
    }
    if record.approval.status != conveyor_domain::ApprovalStatus::Approved {
        return Ok(PublishOut {
            draft_id: draft.id,
            outcome: "pending_approval".to_owned(),
            approval_id: Some(record.approval.id),
            approval_status: record.approval.status.as_str().to_owned(),
            required_decisions,
            approver_kind: policy.approver_kind.as_str().to_owned(),
            toolkit_id: None,
            toolkit_key: toolkit.key,
            toolkit_version: None,
            content_hash: None,
            max_effect: max.as_str().to_owned(),
        });
    }

    // ── the version ───────────────────────────────────────────────────────────────────
    let source = match toolkit.source_id {
        Some(source_id) => store::tool_authoring::source_tx(&mut *tx, ctx.tenant, source_id)
            .await
            .map_err(|error| authoring_store_error(error, "tool_authoring.publish"))?
            .ok_or_else(|| ActionError::resource_not_found("tool source"))?,
        None => {
            let egress = source_egress(&toolkit);
            store::tool_authoring::insert_source_tx(
                &mut *tx,
                ctx.tenant,
                &store::tool_authoring::NewSource {
                    key: &toolkit.source_key,
                    title: &toolkit.title,
                    kind: &toolkit.source_kind,
                    base_url: &toolkit.base_url,
                    auth_adapter: &toolkit.auth_adapter,
                    credential_alias: toolkit.credential_alias.as_deref(),
                    egress: &Value::Array(egress.into_iter().map(Value::String).collect()),
                    meta: json!({"registered_by": "tool_authoring.publish"}),
                },
            )
            .await
            .map_err(|error| authoring_store_error(error, "tool_authoring.publish"))?
        }
    };
    let version =
        store::tool_authoring::latest_toolkit_version_tx(&mut *tx, ctx.tenant, &toolkit.key)
            .await
            .map_err(|error| authoring_store_error(error, "tool_authoring.publish"))?
            .map_or(1, |(_, version, _)| version + 1);

    let content = json!({
        "toolkit": toolkit.key,
        "service": toolkit.service,
        "source": {
            "kind": source.kind,
            "key": source.key,
            "base_url": source.base_url,
        },
        "operations": resolved.iter().map(|(key, effect, idempotency)| {
            let operation = toolkit.operations.iter().find(|operation| &operation.key == key);
            json!({
                "key": key,
                "effect": effect.as_str(),
                "idempotency": idempotency,
                "input_schema": operation.map(|operation| operation.input_schema.clone()),
                "destination": operation.map(destination_json),
            })
        }).collect::<Vec<_>>(),
    });
    let hash = conveyor_domain::compare::content_hash(&content);

    // The Values NewPublishedTool borrows must outlive the struct, so they are built first.
    let destinations: Vec<Value> = toolkit.operations.iter().map(destination_json).collect();
    let redactions: Vec<Value> = toolkit
        .operations
        .iter()
        .map(|operation| {
            Value::Array(
                operation
                    .redact
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            )
        })
        .collect();
    let published_tools: Vec<store::tool_authoring::NewPublishedTool<'_>> = toolkit
        .operations
        .iter()
        .zip(destinations.iter())
        .zip(redactions.iter())
        .filter_map(|((operation, destination), redact)| {
            resolved
                .iter()
                .find(|(key, _, _)| key == &operation.key)
                .map(
                    |(key, effect, idempotency)| store::tool_authoring::NewPublishedTool {
                        key,
                        handler_action: DISPATCH_ACTION,
                        input_schema: &operation.input_schema,
                        output_schema: operation.output_schema.as_ref(),
                        effect: effect.as_str(),
                        idempotency,
                        timeout_secs: operation.timeout_secs,
                        redact,
                        source_id: source.id,
                        destination,
                    },
                )
        })
        .collect();

    let toolkit_meta = json!({
        "content_hash": hash,
        "catalog": "tenant",
        "source_id": source.id,
        "draft_id": draft.id,
        "egress": source.egress,
        "max_effect": max.as_str(),
    });
    let toolkit_id = store::tool_authoring::insert_published_toolkit_tx(
        &mut *tx,
        ctx.tenant,
        &store::tool_authoring::NewPublishedToolkit {
            key: &toolkit.key,
            toolkit_version: version,
            title: &toolkit.title,
            service: &source.key,
            auth_adapter: &source.auth_adapter,
            licence_app: None,
            owner: &toolkit.owner,
            meta: &toolkit_meta,
            tools: &published_tools,
        },
    )
    .await
    .map_err(|error| authoring_store_error(error, "tool_authoring.publish"))?;

    // §11.6.3: publishing the tool rebinds every step that was waiting for its operation. A step
    // already pinned is left alone — binding is a forward act, never a silent upgrade.
    for tool in &published_tools {
        let action = format!("{}.{}", toolkit.key, tool.key);
        store::process::pin_steps_to_tool_tx(
            &mut *tx, ctx.tenant, &action, toolkit_id, version, tool.key,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, %action, "rebind the steps waiting for the published tool");
            ActionError::internal()
        })?;
    }

    let published = json!({
        "toolkit_id": toolkit_id,
        "toolkit_key": toolkit.key,
        "toolkit_version": version,
        "content_hash": hash,
        "max_effect": max.as_str(),
    });
    let mut meta = draft.meta.clone();
    if let Some(object) = meta.as_object_mut() {
        object.insert("publication".to_owned(), published.clone());
    }
    store::tool_authoring::set_draft_status_tx(
        &mut *tx,
        ctx.tenant,
        draft.id,
        "approved",
        Some(&meta),
        "published",
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| authoring_store_error(error, "tool_authoring.publish"))?;
    store::tool_authoring::record_publication_tx(
        &mut *tx,
        ctx.tenant,
        record.approval.id,
        &published,
    )
    .await
    .map_err(|error| authoring_store_error(error, "tool_authoring.publish"))?;

    Ok(PublishOut {
        draft_id: draft.id,
        outcome: "published".to_owned(),
        approval_id: Some(record.approval.id),
        approval_status: "approved".to_owned(),
        required_decisions,
        approver_kind: policy.approver_kind.as_str().to_owned(),
        toolkit_id: Some(toolkit_id),
        toolkit_key: toolkit.key,
        toolkit_version: Some(version),
        content_hash: Some(hash),
        max_effect: max.as_str().to_owned(),
    })
}

/// The source allowlist: the declared source hosts plus every operation's declared egress.
/// The publish validator has already refused a wildcard and a destination outside the
/// operation's list, so the union is concrete and covers every call the version can make.
fn source_egress(toolkit: &DraftToolkit) -> Vec<String> {
    let mut hosts: Vec<String> = toolkit.egress.clone();
    for operation in &toolkit.operations {
        for entry in &operation.egress {
            if !hosts.contains(entry) {
                hosts.push(entry.clone());
            }
        }
    }
    if hosts.is_empty()
        && let Some(host) = host_of(&toolkit.base_url)
    {
        hosts.push(host);
    }
    hosts
}

fn destination_json(operation: &DraftOperation) -> Value {
    if let Some(mcp_tool) = &operation.mcp_tool {
        json!({"kind": "mcp", "mcp_tool": mcp_tool})
    } else {
        json!({
            "kind": "openapi",
            "method": operation.method,
            "path": operation.path,
        })
    }
}

/// Register a streamable-HTTP MCP endpoint as a tenant destination.
#[crate::action(
    name = "toolkits.register_mcp",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn toolkits_register_mcp(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RegisterMcpIn,
) -> Result<crate::spec::HandlerOutput<RegisterMcpOut>, ActionError> {
    let key = input.key.trim().to_owned();
    if !valid_key(&key, "abcdefghijklmnopqrstuvwxyz") || key.contains('.') {
        return Err(invalid(
            "key",
            "invalid_value",
            "a destination alias is lower_snake, starting with a letter",
        ));
    }
    let url = input.url.trim().trim_end_matches('/').to_owned();
    let parsed = reqwest::Url::parse(&url).map_err(|_| {
        invalid(
            "url",
            "invalid_value",
            "the MCP endpoint must be an absolute URL",
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(invalid(
            "url",
            "invalid_value",
            "the MCP endpoint must be http(s)",
        ));
    }
    if parsed.username() != "" || parsed.password().is_some() {
        return Err(invalid(
            "url",
            "invalid_value",
            "a registered endpoint never carries credentials in its URL",
        ));
    }
    if input.url.contains('*') {
        return Err(invalid(
            "url",
            "invalid_value",
            "a registered endpoint is concrete, never a wildcard",
        ));
    }
    let Some(host) = parsed.host_str().map(str::to_owned) else {
        return Err(invalid("url", "invalid_value", "the endpoint has no host"));
    };
    let egress = input
        .egress
        .filter(|entries| !entries.is_empty())
        .unwrap_or_else(|| vec![host.clone()]);
    if egress.is_empty()
        || egress
            .iter()
            .any(|entry| entry.trim().is_empty() || entry.contains('*'))
    {
        return Err(invalid(
            "egress",
            "invalid_value",
            "an allowlist names concrete hosts, never a wildcard",
        ));
    }
    if !egress.iter().any(|entry| entry == &host) {
        return Err(invalid(
            "egress",
            "invalid_value",
            "the endpoint's own host must be in its allowlist",
        ));
    }
    if let Some(alias) = input.credential_alias.as_deref() {
        let known = store::secret::exists_named_tx(&mut *tx, ctx.tenant, alias)
            .await
            .map_err(|error| {
                tracing::error!(%error, "check the credential alias");
                ActionError::internal()
            })?;
        if !known {
            return Err(invalid(
                "credential_alias",
                "not_found",
                format!("this tenant has no secret named {alias:?}"),
            ));
        }
    }
    let source = store::tool_authoring::insert_source_tx(
        &mut *tx,
        ctx.tenant,
        &store::tool_authoring::NewSource {
            key: &key,
            title: input.title.trim(),
            kind: "mcp",
            base_url: &url,
            auth_adapter: &input.auth_adapter.unwrap_or_else(|| "bearer".to_owned()),
            credential_alias: input.credential_alias.as_deref(),
            egress: &Value::Array(egress.iter().cloned().map(Value::String).collect()),
            meta: json!({"registered_by": "toolkits.register_mcp"}),
        },
    )
    .await
    .map_err(|error| authoring_store_error(error, "toolkits.register_mcp"))?;
    Ok(crate::spec::HandlerOutput::created(RegisterMcpOut {
        source_id: source.id,
        key: source.key,
        kind: source.kind,
        base_url: source.base_url,
        credential_alias: source.credential_alias,
        egress,
        status: source.status,
    }))
}

async fn mcp_client(
    url: &str,
    token: Option<String>,
) -> Result<RunningService<RoleClient, ()>, ActionError> {
    let config = match token {
        Some(token) => StreamableHttpClientTransportConfig::with_uri(url).auth_header(token),
        None => StreamableHttpClientTransportConfig::with_uri(url),
    };
    let transport = StreamableHttpClientTransport::from_config(config);
    ().serve(transport).await.map_err(|error| {
        tracing::warn!(%error, %url, "connect the MCP client");
        ActionError::new(
            502,
            "mcp_unreachable",
            format!("the MCP endpoint {url} did not complete the initialize handshake"),
        )
    })
}

async fn alias_token(
    tx: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    alias: Option<&str>,
) -> Result<Option<String>, ActionError> {
    let Some(alias) = alias else {
        return Ok(None);
    };
    match store::secret::open_named_tx(&mut *tx, tenant, alias).await {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(store::secret::SecretError::NotFound) => Err(invalid(
            "credential_alias",
            "not_found",
            format!("this tenant has no secret named {alias:?}"),
        )),
        Err(error) => {
            tracing::warn!(%error, "open the destination credential");
            Err(ActionError::new(
                503,
                "credential_unavailable",
                "the destination credential could not be opened",
            ))
        }
    }
}

/// Discover a registered MCP server's tools into a tool draft.
#[crate::action(
    name = "toolkits.discover_mcp",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn toolkits_discover_mcp(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DiscoverMcpIn,
) -> Result<DiscoverMcpOut, ActionError> {
    let source = match (input.source_id, input.source_key.as_deref()) {
        (Some(id), _) => store::tool_authoring::source_tx(&mut *tx, ctx.tenant, id).await,
        (None, Some(key)) => {
            store::tool_authoring::source_by_key_tx(&mut *tx, ctx.tenant, key).await
        }
        (None, None) => {
            return Err(invalid(
                "source_id",
                "required",
                "name the registered source by id or alias",
            ));
        }
    }
    .map_err(|error| authoring_store_error(error, "toolkits.discover_mcp"))?
    .ok_or_else(|| ActionError::resource_not_found("tool source"))?;
    if source.kind != "mcp" {
        return Err(invalid(
            "source_id",
            "invalid_value",
            "discovery reads a registered MCP endpoint, not an OpenAPI source",
        ));
    }
    let token = alias_token(&mut *tx, ctx.tenant, source.credential_alias.as_deref()).await?;
    let client = mcp_client(&source.base_url, token).await?;
    let listed = match client.list_tools(None).await {
        Ok(listed) => listed,
        Err(error) => {
            let _ = client.cancel().await;
            tracing::warn!(%error, source = %source.key, "list the MCP tools");
            return Err(ActionError::new(
                502,
                "mcp_discovery_failed",
                "the MCP endpoint refused or failed `tools/list`",
            ));
        }
    };
    let mut discovered = Vec::new();
    let mut operations = Vec::new();
    for tool in &listed.tools {
        let key = operation_key(Some(tool.name.as_ref()), "mcp", tool.name.as_ref());
        if operations
            .iter()
            .any(|operation: &DraftOperation| operation.key == key)
        {
            continue;
        }
        discovered.push(tool.name.to_string());
        operations.push(DraftOperation {
            key,
            method: None,
            path: None,
            mcp_tool: Some(tool.name.to_string()),
            summary: tool.description.as_deref().unwrap_or_default().to_owned(),
            input_schema: Value::Object((*tool.input_schema).clone()),
            output_schema: tool
                .output_schema
                .as_ref()
                .map(|schema| Value::Object((**schema).clone())),
            effect: None,
            idempotency: None,
            timeout_secs: 30,
            redact: Vec::new(),
            egress: strings(Some(&source.egress)),
        });
    }
    let _ = client.cancel().await;
    let key = input.key.unwrap_or_else(|| source.key.clone());
    let toolkit = DraftToolkit {
        key: key.clone(),
        title: input.title.unwrap_or_else(|| source.title.clone()),
        service: source.key.clone(),
        owner: "tenant".to_owned(),
        auth_adapter: source.auth_adapter.clone(),
        source_kind: "mcp".to_owned(),
        source_key: source.key.clone(),
        base_url: source.base_url.clone(),
        credential_alias: source.credential_alias.clone(),
        egress: strings(Some(&source.egress)),
        source_id: Some(source.id),
        operations,
    };
    let meta = toolkit.to_meta();
    let draft = store::tool_authoring::upsert_draft_tx(
        &mut *tx,
        ctx.tenant,
        &store::tool_authoring::NewDraft {
            key: &key,
            title: &toolkit.title,
            body: "",
            meta,
            author_id: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| authoring_store_error(error, "toolkits.discover_mcp"))?;
    Ok(DiscoverMcpOut {
        draft_id: draft.id,
        key,
        discovered,
        operations: toolkit.operations.len(),
    })
}

/// Request promotion of a tenant toolkit into the platform catalogue. A tenant admin alone is
/// refused; the request names a development project whose review gate is approved.
#[crate::action(
    name = "tool_authoring.promote",
    version = 1,
    kind = "internal",
    scopes = ["tools:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn tool_authoring_promote(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PromoteIn,
) -> Result<PromoteOut, ActionError> {
    let toolkit = store::tool::toolkit_by_id(&mut *tx, ctx.tenant, input.toolkit_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the toolkit");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("toolkit"))?;
    if toolkit.status != "published" {
        return Err(ActionError::new(
            409,
            "toolkit_not_published",
            "only a published tenant toolkit can be promoted",
        ));
    }
    if toolkit.meta.get("catalog").and_then(Value::as_str) != Some("tenant") {
        return Err(ActionError::new(
            422,
            "promotion_not_tenant",
            "the toolkit is not a tenant-authored toolkit",
        ));
    }
    let Some(development_project_id) = input.development_project_id else {
        return Err(ActionError::new(
            403,
            "platform_review_required",
            "promoting a toolkit to the platform catalogue is a development project with a \
             platform review, never an admin toggle",
        ));
    };
    let project =
        store::tool_authoring::development_project_tx(&mut *tx, ctx.tenant, development_project_id)
            .await
            .map_err(|error| authoring_store_error(error, "tool_authoring.promote"))?
            .ok_or_else(|| ActionError::resource_not_found("development project"))?;
    if project.1 == "refused" {
        return Err(ActionError::new(
            409,
            "project_refused",
            "a refused development project cannot carry a promotion",
        ));
    }
    let review = store::tool_authoring::approved_development_review_tx(
        &mut *tx,
        ctx.tenant,
        development_project_id,
    )
    .await
    .map_err(|error| authoring_store_error(error, "tool_authoring.promote"))?
    .ok_or_else(|| {
        ActionError::new(
            403,
            "platform_review_required",
            "the development project has no approved review gate",
        )
    })?;
    let promotion = store::tool_authoring::open_promotion_tx(
        &mut *tx,
        ctx.tenant,
        toolkit.id,
        development_project_id,
        review,
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| authoring_store_error(error, "tool_authoring.promote"))?;
    Ok(PromoteOut {
        promotion_id: promotion.id,
        toolkit_id: toolkit.id,
        development_project_id: promotion.development_project_id,
        review_approval_id: promotion.review_approval_id,
        status: promotion.status,
    })
}

// ── tools.dispatch — the one registered-destination path ───────────────────────────────

/// The dispatch input the broker builds. It is never a caller's document: the broker's
/// `tools.call` schema is closed and its handler passes the pinned tool's own operation here.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ToolDispatchIn {
    /// The registered source the publication pinned.
    source_id: Uuid,
    /// The toolkit key (audit context).
    toolkit_key: String,
    /// The operation key (audit context).
    tool_key: String,
    /// `{kind, method, path}` or `{kind, mcp_tool}`.
    destination: Value,
    /// The tool's own validated input document.
    arguments: Value,
    /// The grant's credential alias, when it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credential_alias: Option<String>,
}

fn dispatch_input_schema() -> crate::spec::Schema {
    schemars::schema_for!(ToolDispatchIn)
}

fn dispatch_output_schema() -> crate::spec::Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The registered destination's result document, capped by the broker.",
        "additionalProperties": true
    })
}

/// `tools.dispatch` — reachable only from the broker (`HandlerRef::ToolDestination`).
fn tools_dispatch() -> ActionSpec {
    ActionSpec {
        name: Cow::Borrowed(DISPATCH_ACTION),
        version: 1,
        kind: ActionKind::Internal,
        input: SchemaRef::Compiled(dispatch_input_schema),
        output: SchemaRef::Compiled(dispatch_output_schema),
        scopes: Cow::Borrowed(&[Scope::RunsWrite]),
        idempotent: false,
        timeout_ms: 300_000,
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints {
            tier: AgentTier::None,
            proposal_required: false,
        },
        task_capable: false,
        captures_usage: false,
        artefact: None,
        approval_target: None,
        handler: HandlerRef::ToolDestination,
    }
}

inventory::submit! { crate::registry::ActionRegistration::new(tools_dispatch) }

/// Dispatch one authored call to its registered source.
///
/// `invoke` routes [`HandlerRef::ToolDestination`] here and refuses every authority but
/// [`Authority::Internal`] first, so this function is the broker's arm and never a caller's.
pub(crate) async fn invoke_destination(
    pool: &PgPool,
    spec: &ActionSpec,
    ctx: HandlerCtx,
    input: Value,
    _request: &InvokeRequest,
) -> Result<InvokeOutcome, ActionError> {
    let call: ToolDispatchIn = serde_json::from_value(input).map_err(|error| {
        tracing::error!(%error, action = %spec.name, "the broker's dispatch document is invalid");
        ActionError::internal()
    })?;
    tracing::debug!(
        toolkit = %call.toolkit_key,
        tool = %call.tool_key,
        "dispatch a registered tool"
    );
    let mut tx = store::begin_scoped(pool, ctx.tenant)
        .await
        .map_err(|_| ActionError::internal())?;
    let source = store::tool_authoring::source_tx(&mut tx, ctx.tenant, call.source_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the registered source");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "tool_not_ready",
                "the registered destination no longer exists",
            )
        })?;
    if source.status != "published" {
        return Err(ActionError::new(
            409,
            "tool_not_ready",
            "the registered destination is not published",
        ));
    }
    let alias = call
        .credential_alias
        .as_deref()
        .or(source.credential_alias.as_deref());
    let token = alias_token(&mut tx, ctx.tenant, alias).await?;
    let _ = tx.rollback().await;

    // The call-time egress check: a registration that slipped past validation still cannot
    // reach a host outside its allowlist.
    let base = reqwest::Url::parse(source.base_url.trim_end_matches('/')).map_err(|_| {
        ActionError::new(500, "tool_misconfigured", "the registered URL is not a URL")
    })?;
    let host = base.host_str().unwrap_or_default().to_owned();
    let egress = strings(Some(&source.egress));
    if !egress_admits(&egress, &host) {
        return Err(ActionError::new(
            403,
            "tool_egress_forbidden",
            format!("{host:?} is not in the registered allowlist"),
        ));
    }

    let kind = call
        .destination
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or(source.kind.as_str());
    match kind {
        "mcp" => {
            let tool = call
                .destination
                .get("mcp_tool")
                .and_then(Value::as_str)
                .ok_or_else(ActionError::internal)?;
            let client = mcp_client(&source.base_url, token).await?;
            let arguments = call.arguments.as_object().cloned().unwrap_or_default();
            let result = client
                .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
                .await;
            let _ = client.cancel().await;
            let result = result.map_err(|error| {
                tracing::warn!(%error, tool, "the MCP destination call failed");
                ActionError::new(
                    502,
                    "tool_backend_error",
                    format!("the MCP destination failed to answer {tool:?}"),
                )
            })?;
            let body = match result.structured_content {
                Some(value) => value,
                None => {
                    let mut texts = Vec::new();
                    for block in &result.content {
                        if let rmcp::model::ContentBlock::Text(text) = block {
                            texts.push(text.text.clone());
                        }
                    }
                    if texts.len() == 1 {
                        serde_json::from_str(&texts[0])
                            .unwrap_or_else(|_| json!({"text": texts[0]}))
                    } else {
                        json!({"content": texts})
                    }
                }
            };
            let status = if result.is_error.unwrap_or(false) {
                422
            } else {
                200
            };
            Ok(InvokeOutcome::new(status, body))
        }
        "openapi" => {
            let method = call
                .destination
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("POST");
            let path = call
                .destination
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("/");
            let url = format!("{}{}", source.base_url.trim_end_matches('/'), path);
            let client = reqwest::Client::new();
            let mut request = match method {
                "GET" => client.get(&url).query(&query_pairs(&call.arguments)),
                "DELETE" => client.delete(&url).query(&query_pairs(&call.arguments)),
                "PUT" => client.put(&url).json(&call.arguments),
                "PATCH" => client.patch(&url).json(&call.arguments),
                _ => client.post(&url).json(&call.arguments),
            };
            request = request.header(crate::invoke::TENANT_HEADER, ctx.tenant.to_string());
            if let Some(token) = &token {
                request = request.bearer_auth(token);
            }
            let response = request.send().await.map_err(|error| {
                tracing::warn!(%error, %url, "the registered destination call failed");
                ActionError::new(
                    502,
                    "tool_backend_error",
                    format!("the destination {host} did not answer"),
                )
            })?;
            let status = response.status().as_u16();
            let bytes = response.bytes().await.unwrap_or_default();
            if bytes.len() > MAX_DESTINATION_BYTES {
                return Err(ActionError::new(
                    502,
                    "tool_response_too_large",
                    format!(
                        "the destination answered {} bytes; the cap is {MAX_DESTINATION_BYTES}",
                        bytes.len()
                    ),
                ));
            }
            let body = if bytes.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&bytes)
                    .unwrap_or_else(|_| json!({"body": String::from_utf8_lossy(&bytes)}))
            };
            Ok(InvokeOutcome::new(status, body))
        }
        other => Err(ActionError::new(
            500,
            "tool_misconfigured",
            format!("the registered source kind {other:?} is unknown"),
        )),
    }
}

fn query_pairs(arguments: &Value) -> Vec<(String, String)> {
    arguments
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| match value {
                    Value::String(text) => Some((key.clone(), text.clone())),
                    Value::Number(number) => Some((key.clone(), number.to_string())),
                    Value::Bool(flag) => Some((key.clone(), flag.to_string())),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(key: &str, effect: Option<&str>, idempotency: Option<&str>) -> DraftOperation {
        DraftOperation {
            key: key.to_owned(),
            method: Some("POST".to_owned()),
            path: Some("/things".to_owned()),
            mcp_tool: None,
            summary: String::new(),
            input_schema: json!({
                "type": "object",
                "properties": {"name": {"type": "string"}},
                "additionalProperties": false,
            }),
            output_schema: None,
            effect: effect.map(str::to_owned),
            idempotency: idempotency.map(str::to_owned),
            timeout_secs: 30,
            redact: Vec::new(),
            egress: Vec::new(),
        }
    }

    fn toolkit(operations: Vec<DraftOperation>) -> DraftToolkit {
        DraftToolkit {
            key: "weather".to_owned(),
            title: "Weather".to_owned(),
            service: "weather".to_owned(),
            owner: "tenant".to_owned(),
            auth_adapter: "bearer".to_owned(),
            source_kind: "openapi".to_owned(),
            source_key: "weather".to_owned(),
            base_url: "https://api.example.com".to_owned(),
            credential_alias: None,
            egress: vec!["api.example.com".to_owned()],
            source_id: None,
            operations,
        }
    }

    #[test]
    fn a_missing_effect_defaults_to_read_and_a_write_needs_idempotency() {
        let (resolved, findings) = validate_draft(&toolkit(vec![operation("a.b", None, None)]));
        assert!(findings.is_empty(), "{findings:?}");
        assert_eq!(resolved[0].1, Effect::Read);

        let (_, findings) =
            validate_draft(&toolkit(vec![operation("a.write", Some("write"), None)]));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].code, "idempotency_missing");

        let (_, findings) = validate_draft(&toolkit(vec![operation(
            "a.write",
            Some("write"),
            Some("required"),
        )]));
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn an_open_schema_and_a_wildcard_egress_are_findings() {
        let mut open = operation("a.open", None, None);
        open.input_schema = json!({"type": "object"});
        let (_, findings) = validate_draft(&toolkit(vec![open]));
        assert!(findings.iter().any(|f| f.code == "input_schema_open"));

        let mut wildcard = operation("a.wild", None, None);
        wildcard.egress = vec!["*".to_owned()];
        let (_, findings) = validate_draft(&toolkit(vec![wildcard]));
        assert!(findings.iter().any(|f| f.code == "egress_wildcard"));
    }

    #[test]
    fn the_operation_key_is_stable_and_closed() {
        assert_eq!(
            operation_key(Some("Get Weather"), "get", "/x"),
            "get_weather"
        );
        assert_eq!(operation_key(None, "post", "/v1/things"), "post_v1_things");
        assert_eq!(operation_key(Some("1st"), "get", "/x"), "op_1st");
    }

    #[test]
    fn an_openapi_document_parses_into_draft_operations() {
        let document = json!({
            "openapi": "3.1.0",
            "paths": {
                "/widgets": {
                    "post": {
                        "operationId": "createWidget",
                        "summary": "Create a widget",
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {"name": {"type": "string"}},
                                        "additionalProperties": false
                                    }
                                }
                            }
                        },
                        "responses": {
                            "201": {
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object",
                                            "properties": {"id": {"type": "string"}},
                                            "required": ["id"]
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let operations = parse_openapi(&document).expect("parse");
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].key, "createwidget");
        assert_eq!(operations[0].method.as_deref(), Some("POST"));
        assert!(operations[0].effect.is_none());
        assert!(operations[0].output_schema.is_some());
    }
}
