#![forbid(unsafe_code)]
//! External action ingest from a service's `openapi.json` (S04 §5.4).
//!
//! `openapiv3` parses the document. Each operation becomes a `kind: external` row in
//! `action_registry`, keyed `(tenant, name, spec_version)`; `approved_at` decides what
//! serves (S4-D3). A refresh diffs against the highest **approved** version:
//!
//! * an added optional input member or an added output member auto-approves;
//! * a removed or retyped input member, a newly required one, a removed output member, or
//!   a moved path/method is **breaking** — the row is written unapproved and the caller
//!   gets `action-schema-breaking` (`422`, CLI exit 2) until
//!   `conveyor admin actions approve <name> --version <n>`.
//!
//! An external name that collides with a compiled action is refused (`action-name-conflict`,
//! S4-D4): one namespace, no precedence.

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use sqlx::PgPool;

/// The service's operations, after name derivation and schema extraction.
#[derive(Clone, Debug)]
pub struct DiscoveredAction {
    /// `<resource>.<verb>`.
    pub name: String,
    /// `POST` etc.
    pub method: String,
    /// The operation's path in the service document.
    pub path: String,
    /// The service key.
    pub service: String,
    /// The resolved request schema.
    pub input: Value,
    /// The resolved success schema.
    pub output: Value,
    /// The request's required members.
    pub required: Vec<String>,
    /// sha256 of `{method, path, input, output}`.
    pub schema_hash: String,
    /// Pinned at ingest: the compiled default unless the document overrides it.
    pub timeout_ms: u32,
    /// Whether replaying the call is safe.
    pub idempotent: bool,
}

/// Why ingest could not proceed.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    /// The document is not valid OpenAPI.
    #[error("openapi: {0}")]
    Parse(String),
    /// An operation could not be named.
    #[error("operation {0:?} cannot be named: {1}")]
    Unnameable(String, String),
    /// The name already exists as a compiled action (S4-D4).
    #[error("action-name-conflict: {0:?} is compiled into this binary")]
    NameConflict(String),
    /// The refresh is breaking and needs explicit approval.
    #[error("action-schema-breaking: {name} version {version}: {changes:?}")]
    Breaking {
        /// The action name.
        name: String,
        /// The version that would have been written.
        version: i32,
        /// One line per breaking change.
        changes: Vec<String>,
    },
    /// The database refused the write.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
}

/// What one ingest did.
#[derive(Clone, Debug, Default)]
pub struct IngestOutcome {
    /// `(name, spec_version)` rows written.
    pub written: Vec<(String, i32)>,
    /// `(name, spec_version)` rows approved automatically.
    pub approved: Vec<(String, i32)>,
    /// Breaking refreshes, each written unapproved.
    pub breaking: Vec<(String, i32, Vec<String>)>,
}

impl IngestOutcome {
    /// True when any refresh needs an explicit approval.
    pub fn has_breaking(&self) -> bool {
        !self.breaking.is_empty()
    }

    /// The CLI's exit code for this outcome: `2` when a refresh still needs approval.
    /// The one place that number lives; `main` maps it without restating the rule.
    pub fn exit_code(&self) -> Option<u8> {
        self.has_breaking().then_some(2)
    }
}

fn is_word(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `operationId` when it is `<resource>.<verb>`; otherwise `<tag>.<method>_<last segment>`.
fn derive_name(
    operation_id: Option<&str>,
    tags: &[String],
    path: &str,
    method: &str,
) -> Result<String, IngestError> {
    if let Some(id) = operation_id
        && let Some((resource, verb)) = id.split_once('.')
        && !verb.contains('.')
        && is_word(resource)
        && is_word(verb)
    {
        return Ok(id.to_owned());
    }
    let tag = tags
        .first()
        .map(|tag| sanitize(tag))
        .filter(|tag| is_word(tag))
        .unwrap_or_else(|| "external".to_owned());
    let segment = path
        .trim_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty() && !segment.starts_with('{'))
        .map(sanitize)
        .unwrap_or_else(|| "root".to_owned());
    let name = format!("{tag}.{}_{}", method.to_ascii_lowercase(), segment);
    if !is_word(name.split_once('.').map(|(r, _)| r).unwrap_or("")) {
        return Err(IngestError::Unnameable(
            path.to_owned(),
            "no parseable operationId and no usable tag".into(),
        ));
    }
    Ok(name)
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Resolve a same-document `$ref` chain (the stored spec must stand alone).
fn resolve(value: &Value, root: &Value, depth: usize) -> Value {
    if depth > 32 {
        return value.clone();
    }
    match value {
        Value::Object(map) => {
            if let Some(reference) = map.get("$ref").and_then(Value::as_str)
                && let Some(target) = pointer(root, reference)
            {
                return resolve(target, root, depth + 1);
            }
            Value::Object(
                map.iter()
                    .map(|(key, nested)| (key.clone(), resolve(nested, root, depth + 1)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| resolve(item, root, depth + 1))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn pointer<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let path = reference.strip_prefix("#/")?;
    let mut current = root;
    for segment in path.split('/') {
        let segment = segment.replace("~1", "/").replace("~0", "~");
        current = current.get(&segment)?;
    }
    Some(current)
}

/// Parse a service document into external actions.
pub fn discover(
    service: &str,
    bytes: &[u8],
    compiled_names: &[&str],
) -> Result<Vec<DiscoveredAction>, IngestError> {
    let document: Value =
        serde_json::from_slice(bytes).map_err(|error| IngestError::Parse(error.to_string()))?;
    let spec: openapiv3::OpenAPI = serde_json::from_value(document.clone())
        .map_err(|error| IngestError::Parse(error.to_string()))?;

    let mut out = Vec::new();
    for (path, item) in spec.paths.paths.iter() {
        let openapiv3::ReferenceOr::Item(item) = item else {
            continue;
        };
        let operations = [
            ("GET", item.get.as_ref()),
            ("POST", item.post.as_ref()),
            ("PUT", item.put.as_ref()),
            ("PATCH", item.patch.as_ref()),
            ("DELETE", item.delete.as_ref()),
        ];
        for (method, operation) in operations {
            let Some(operation) = operation else { continue };
            let name = derive_name(
                operation.operation_id.as_deref(),
                &operation.tags,
                path,
                method,
            )?;
            if compiled_names.contains(&name.as_str()) {
                return Err(IngestError::NameConflict(name));
            }
            let operation = serde_json::to_value(operation)
                .map_err(|error| IngestError::Parse(error.to_string()))?;
            let operation = resolve(&operation, &document, 0);

            let input = operation
                .pointer("/requestBody/content/application~1json/schema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
            let required = input
                .get("required")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();

            let output = success_schema(&operation);
            let schema_hash = crate::spec::sha256_hex(
                &serde_json::to_vec(&json!({
                    "method": method,
                    "path": path,
                    "input": input,
                    "output": output,
                }))
                .unwrap_or_default(),
            );
            out.push(DiscoveredAction {
                name,
                method: method.to_owned(),
                path: path.clone(),
                service: service.to_owned(),
                input,
                output,
                required,
                schema_hash,
                timeout_ms: 30_000,
                idempotent: method == "GET",
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn success_schema(operation: &Value) -> Value {
    let Some(responses) = operation.get("responses").and_then(Value::as_object) else {
        return json!({"type": "object"});
    };
    // The lowest 2xx is the operation's success document.
    let mut keys: Vec<&String> = responses
        .keys()
        .filter(|key| key.starts_with('2') || key.as_str() == "default")
        .collect();
    keys.sort();
    for key in keys {
        if let Some(schema) = responses[key].pointer("/content/application~1json/schema") {
            return schema.clone();
        }
    }
    json!({"type": "object"})
}

/// The stored spec document for a discovered action.
pub fn stored_spec(action: &DiscoveredAction) -> Value {
    json!({
        "method": action.method,
        "path": action.path,
        "input": action.input,
        "output": action.output,
        "required": action.required,
        "schema_hash": action.schema_hash,
    })
}

/// Compare the approved spec with a refresh.
///
/// Returns one line per breaking change; an empty vector is an `added-optional` (or
/// no-change) refresh, which auto-approves.
pub fn breaking_changes(approved: &Value, next: &DiscoveredAction) -> Vec<String> {
    let mut changes = Vec::new();

    let previous_method = approved.get("method").and_then(Value::as_str).unwrap_or("");
    if !previous_method.eq_ignore_ascii_case(&next.method) {
        changes.push(format!(
            "method changed {previous_method:?} -> {:?}",
            next.method
        ));
    }
    let previous_path = approved.get("path").and_then(Value::as_str).unwrap_or("");
    if previous_path != next.path {
        changes.push(format!("path changed {previous_path:?} -> {:?}", next.path));
    }

    let previous_input = approved.get("input").cloned().unwrap_or_else(|| json!({}));
    let previous_properties = previous_input
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let next_properties = next
        .input
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    for (field, previous) in &previous_properties {
        match next_properties.get(field) {
            None => changes.push(format!("input field {field:?} removed")),
            Some(current) => {
                let before = previous.get("type").cloned().unwrap_or(Value::Null);
                let after = current.get("type").cloned().unwrap_or(Value::Null);
                if before != after {
                    changes.push(format!("input field {field:?} retyped {before} -> {after}"));
                }
            }
        }
    }
    let previous_required: Vec<&str> = approved
        .get("required")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    // A field that is now required and was not before is breaking whether it was optional
    // or absent: a client that does not send it fails either way.
    for field in &next.required {
        if !previous_required.contains(&field.as_str()) {
            changes.push(format!("input field {field:?} is newly required"));
        }
    }

    let previous_output = approved.get("output").cloned().unwrap_or_else(|| json!({}));
    let previous_out = previous_output
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let next_out = next
        .output
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for field in previous_out.keys() {
        if !next_out.contains_key(field) {
            changes.push(format!("output field {field:?} removed"));
        }
    }

    changes
}

/// The production ingest path `admin actions ingest` runs.
///
/// It is shared with `ingest_diff.rs` so the gate's exit-2 claim is exercised through the
/// real sequence — discover against the COMPILED names (so a collision is refused), apply,
/// then (with `approve`) approve a breaking refresh. `main` only prints and maps
/// [`IngestOutcome::exit_code`].
pub async fn ingest_production(
    pool: &PgPool,
    tenant: TenantId,
    service: &str,
    bytes: &[u8],
    approve: bool,
) -> Result<IngestOutcome, IngestError> {
    let compiled = crate::registry::registry().names();
    let actions = discover(service, bytes, &compiled)?;
    let mut outcome = apply(pool, tenant, &actions, None).await?;
    if approve {
        for (name, version, _changes) in outcome.breaking.clone() {
            let mut tx = store::begin_scoped(pool, tenant).await?;
            store::actions::registry_approve(&mut tx, &name, version, None).await?;
            store::actions::registry_revoke_others(&mut tx, &name, version).await?;
            tx.commit().await?;
            outcome.approved.push((name, version));
        }
        outcome.breaking.clear();
    }
    Ok(outcome)
}

/// Ingest one service document into `tenant`'s registry.
///
/// A brand-new action and a non-breaking refresh are approved immediately; a breaking
/// refresh writes an **unapproved** row and is reported in [`IngestOutcome::breaking`].
pub async fn apply(
    pool: &PgPool,
    tenant: TenantId,
    actions: &[DiscoveredAction],
    approved_by: Option<uuid::Uuid>,
) -> Result<IngestOutcome, IngestError> {
    let mut outcome = IngestOutcome::default();
    let mut tx = store::begin_scoped(pool, tenant).await?;
    let mut existing = store::actions::approved_registry(&mut tx).await?;
    existing.sort_by(|a, b| a.name.cmp(&b.name));

    for action in actions {
        let previous = existing.iter().find(|row| row.name == action.name);
        let (version, approved, changes) = match previous {
            None => (1, true, Vec::new()),
            Some(row) => {
                let changes = breaking_changes(&row.spec, action);
                (row.spec_version + 1, changes.is_empty(), changes)
            }
        };
        let spec = stored_spec(action);
        let pending = store::actions::PendingRegistryRow {
            name: &action.name,
            spec_version: version,
            kind: "external",
            service: &action.service,
            spec: spec.clone(),
            schema_hash: &action.schema_hash,
            timeout_ms: action.timeout_ms as i32,
            idempotent: action.idempotent,
        };
        let inserted = store::actions::registry_insert_pending(&mut tx, tenant, &pending).await?;
        if inserted.is_some() {
            outcome.written.push((action.name.clone(), version));
        }
        if approved {
            store::actions::registry_approve(&mut tx, &action.name, version, approved_by).await?;
            store::actions::registry_revoke_others(&mut tx, &action.name, version).await?;
            outcome.approved.push((action.name.clone(), version));
        } else {
            outcome
                .breaking
                .push((action.name.clone(), version, changes));
        }
    }
    tx.commit().await?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &[u8] = include_bytes!("../tests/fixtures/external/openapi.json");

    #[test]
    fn the_fixture_document_derives_four_parseable_names() {
        let actions = discover("fixtures", FULL, &[]).expect("parse");
        let names: Vec<&str> = actions.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "fixtures.echo",
                "fixtures.fail",
                "fixtures.slow",
                "fixtures.timeout"
            ]
        );
        for action in &actions {
            assert_eq!(action.method, "POST");
            assert!(!action.schema_hash.is_empty());
        }
    }

    #[test]
    fn a_name_that_collides_with_a_compiled_action_is_refused() {
        let error = discover("fixtures", FULL, &["fixtures.echo"]).unwrap_err();
        assert!(matches!(error, IngestError::NameConflict(name) if name == "fixtures.echo"));
    }

    #[test]
    fn removing_an_input_field_is_breaking_and_adding_one_is_not() {
        let actions = discover("fixtures", FULL, &[]).expect("parse");
        let echo = actions
            .iter()
            .find(|action| action.name == "fixtures.echo")
            .expect("echo");
        let stored = stored_spec(echo);
        assert!(breaking_changes(&stored, echo).is_empty());

        // Added-optional: not breaking.
        let mut added = echo.clone();
        added.input["properties"]["extra"] = json!({"type": "string"});
        assert!(breaking_changes(&stored, &added).is_empty());

        // Removed input field: breaking.
        let mut removed = echo.clone();
        removed.input.as_object_mut().unwrap().remove("properties");
        removed.input["properties"] = json!({});
        let changes = breaking_changes(&stored, &removed);
        assert!(
            changes
                .iter()
                .any(|line| line.contains("message") && line.contains("removed")),
            "{changes:?}"
        );

        // Newly required: breaking.
        let mut required = echo.clone();
        required.required = vec!["message".into(), "extra".into()];
        required.input["properties"]["extra"] = json!({"type": "string"});
        let changes = breaking_changes(&stored, &required);
        assert!(
            changes.iter().any(|line| line.contains("newly required")),
            "{changes:?}"
        );

        // Removed output field: breaking.
        let mut no_output = echo.clone();
        no_output.output = json!({"type": "object", "properties": {}});
        let changes = breaking_changes(&stored, &no_output);
        assert!(
            changes.iter().any(|line| line.contains("output field")),
            "{changes:?}"
        );
    }
}
