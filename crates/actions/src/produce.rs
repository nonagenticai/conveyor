//! `produce.script` — the production family's one model-backed action.
//!
//! The content service served storyline and script operations over HTTP (see
//! `conveyor-produce`'s captured contract). ADR 0007 makes that capability an action rather than
//! a service, and the action calls a model through the inference router — so this file is a
//! **transport**, exactly like `youtube.rs` (#182) and `ingest.rs`: the action needs a model call
//! *and* local work that no `HandlerRef::External` passthrough could do. The local half — the shot
//! ids and the duration budget — lives in `conveyor-produce::script`, which is transport-free and
//! tested without a model.
//!
//! `crates/api/tests/externals_declared.rs` lists this file by name, because that list is compared
//! exactly: speaking HTTP from a handler bypasses the engine's timeout, breaker, idempotency claim
//! and audit row, so the exemption is a decision someone made, not a detail.
//!
//! ## What the model is and is not trusted with
//!
//! The model writes prose. It does not decide which shot is which, how many there are, or how long
//! they run — those come from the brief and the target alone. The request therefore sends the shot
//! ids and asks for one description per id; the answer is bound back by id, and a shot the model
//! did not answer for stays empty rather than inheriting its neighbour's text.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use conveyor_produce::script::{BudgetError, MIN_SHOT_MS};

use crate::invoke::service_url;
use crate::{ActionError, FieldError, HandlerCtx};

/// The router's service key (`CONVEYOR_EXTERNAL_INFERENCE_URL`).
const SERVICE: &str = "inference";
/// The router's chat path — the same one `completions.create` declares.
const PATH: &str = "/api/chat/completions";
/// A script is a long completion; the engine's default would cut it off.
const TIMEOUT_MS: u32 = 120_000;
/// A brief shorter than this cannot describe anything.
const MIN_BRIEF_CHARS: usize = 8;
/// The cap on the brief, matching the other text inputs in this crate.
const MAX_BRIEF_CHARS: usize = 8_000;

/// What the caller asks for.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptIn {
    /// What the piece is about, in the caller's words.
    pub brief: String,
    /// The duration the result must fit, in milliseconds. A shot is one second of it.
    pub target_duration_ms: u32,
    /// The model or schema id to route to; the router applies its own policy when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// One shot in the answer.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShotOut {
    /// Stable across runs for the same brief and position.
    pub id: String,
    pub index: u32,
    pub description: String,
    pub duration_ms: u32,
}

/// The storyline and its shot list.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptOut {
    pub storyline: String,
    pub shots: Vec<ShotOut>,
    pub target_duration_ms: u32,
}

/// What the model is asked to return.
#[derive(Debug, Deserialize)]
struct ModelAnswer {
    #[serde(default)]
    storyline: String,
    #[serde(default)]
    shots: Vec<ModelShot>,
}

#[derive(Debug, Deserialize)]
struct ModelShot {
    id: String,
    #[serde(default)]
    description: String,
}

#[crate::action(
    name = "produce.script",
    version = 1,
    kind = "internal",
    scopes = ["model:write"],
    idempotent = false,
    timeout_ms = 120_000,
    effect = "spend",
    confirm = "none",
    tenancy = "tenant"
)]
async fn produce_script(
    _tx: &mut PgConnection,
    _ctx: HandlerCtx,
    input: ScriptIn,
) -> Result<ScriptOut, ActionError> {
    let brief = input.brief.trim();
    if brief.chars().count() < MIN_BRIEF_CHARS {
        return Err(ActionError::invalid_input(vec![FieldError {
            field: "brief".to_owned(),
            code: "too_short".to_owned(),
            detail: format!("a brief needs at least {MIN_BRIEF_CHARS} characters"),
        }]));
    }
    if brief.chars().count() > MAX_BRIEF_CHARS {
        return Err(ActionError::invalid_input(vec![FieldError {
            field: "brief".to_owned(),
            code: "too_long".to_owned(),
            detail: format!("a brief is capped at {MAX_BRIEF_CHARS} characters"),
        }]));
    }
    if input.target_duration_ms < MIN_SHOT_MS {
        return Err(ActionError::invalid_input(vec![FieldError {
            field: "target_duration_ms".to_owned(),
            code: "too_short".to_owned(),
            detail: format!("a target needs at least {MIN_SHOT_MS}ms — one shot"),
        }]));
    }

    // The identity of every shot is decided here, before the model is asked anything, and it is
    // what the request is built from: the model is given the ids, never asked to invent them.
    let skeleton = conveyor_produce::script::skeleton(brief, input.target_duration_ms);
    let ids: Vec<&str> = skeleton.iter().map(|shot| shot.id.as_str()).collect();

    let answer = ask_the_model(&input, brief, &ids, input.target_duration_ms).await?;

    let descriptions: Vec<(String, String)> = answer
        .shots
        .into_iter()
        .filter(|shot| ids.contains(&shot.id.as_str()))
        .map(|shot| (shot.id, shot.description))
        .collect();
    let script = conveyor_produce::script::assemble(
        brief,
        input.target_duration_ms,
        &answer.storyline,
        &descriptions,
    );

    // A model cannot be trusted to respect a budget, so the budget is enforced on the assembled
    // result rather than requested in the prompt. The refusal names the shots to cut.
    conveyor_produce::script::validate(&script).map_err(|error| {
        // The refusal's own words, computed before the match moves the shot ids out of it.
        let text = error.to_string();
        match error {
            BudgetError::Over { offending, .. } => ActionError::invalid_input(vec![FieldError {
                field: "shots".to_owned(),
                code: "over_budget".to_owned(),
                detail: format!("{text} — offending shots: {}", offending.join(", ")),
            }]),
            BudgetError::Under { .. } => ActionError::invalid_input(vec![FieldError {
                field: "shots".to_owned(),
                code: "under_budget".to_owned(),
                detail: text,
            }]),
            BudgetError::Empty => ActionError::invalid_input(vec![FieldError {
                field: "shots".to_owned(),
                code: "empty".to_owned(),
                detail: text,
            }]),
        }
    })?;

    Ok(ScriptOut {
        storyline: script.storyline,
        shots: script
            .shots
            .into_iter()
            .map(|shot| ShotOut {
                id: shot.id,
                index: shot.index,
                description: shot.description,
                duration_ms: shot.duration_ms,
            })
            .collect(),
        target_duration_ms: script.target_duration_ms,
    })
}

/// Ask the router for a storyline and one description per shot id.
async fn ask_the_model(
    input: &ScriptIn,
    brief: &str,
    ids: &[&str],
    target_duration_ms: u32,
) -> Result<ModelAnswer, ActionError> {
    let base = service_url(SERVICE).ok_or_else(|| ActionError::unconfigured(SERVICE))?;
    let url = format!("{}{PATH}", base.trim_end_matches('/'));

    let id_list = ids.join(", ");
    let system = format!(
        "You write shot lists for short videos. Answer with a single JSON object and nothing \
         else: {{\"storyline\": \"<one paragraph>\", \"shots\": [{{\"id\": \"<id>\", \
         \"description\": \"<what the shot shows>\"}}]}}. The shots array must contain exactly \
         these ids, in this order: {id_list}. Each description is one sentence. Do not add, drop \
         or renumber a shot."
    );
    let user = format!(
        "Brief: {brief}\nTarget duration: {target_duration_ms}ms across {} shots.",
        ids.len()
    );

    let body = serde_json::json!({
        "model": input.model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.2,
        "response_format": {"type": "json_object"}
    });

    // The exemption from the no-reqwest rule is this transport owning its own timeout and error
    // mapping, exactly as `youtube.rs` does.
    let response = reqwest::Client::new()
        .post(&url)
        .timeout(Duration::from_millis(u64::from(TIMEOUT_MS)))
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                ActionError::timeout("produce.script", TIMEOUT_MS)
            } else {
                ActionError::upstream_failed(SERVICE, 502)
            }
        })?;
    if !response.status().is_success() {
        return Err(ActionError::upstream_failed(
            SERVICE,
            response.status().as_u16(),
        ));
    }

    // OpenAI-shaped answer: the content is a JSON document the model wrote, not a field the router
    // models, so it is parsed from the string rather than from the envelope.
    let document: serde_json::Value = response
        .json()
        .await
        .map_err(|_| ActionError::upstream_failed(SERVICE, 502))?;
    answer_from_document(&document)
}

/// The model's answer from the router's OpenAI-shaped document.
///
/// A `length` finish is refused **before** the content is read: its visible tail can look like a complete
/// JSON document. 422, not 5xx — the workflow tick maps 5xx to `Deferred` and would re-buy the same capped
/// call.
fn answer_from_document(document: &serde_json::Value) -> Result<ModelAnswer, ActionError> {
    let choice = &document["choices"][0];
    if choice["finish_reason"].as_str() == Some("length") {
        return Err(ActionError::new(
            422,
            "inference-truncated",
            "produce.script: the router's answer was truncated at its token cap (finish_reason \
             \"length\"); refusing the partial document",
        ));
    }
    let content = choice["message"]["content"]
        .as_str()
        .ok_or_else(|| ActionError::upstream_failed(SERVICE, 502))?;
    serde_json::from_str::<ModelAnswer>(content)
        .map_err(|_| ActionError::upstream_failed(SERVICE, 502))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_truncated_answer_is_refused_before_it_is_parsed() {
        // The content is a complete-looking JSON document; only `finish_reason` says it was cut.
        let document = serde_json::json!({
            "choices": [{
                "finish_reason": "length",
                "message": {"content": "{\"storyline\": \"a walk\", \"shots\": []}"}
            }]
        });
        // The premise: on its own the content parses as a complete document, so only `finish_reason` can
        // tell it was cut.
        let content = document["choices"][0]["message"]["content"]
            .as_str()
            .unwrap();
        serde_json::from_str::<ModelAnswer>(content).expect("the content alone parses");
        let error = answer_from_document(&document).unwrap_err();
        assert_eq!(error.status, 422);
        assert_eq!(error.code, "inference-truncated");
    }

    #[test]
    fn a_complete_answer_parses() {
        let document = serde_json::json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {"content":
                    "{\"storyline\": \"a walk\", \"shots\": [{\"id\": \"s1\", \"description\": \"a\"}]}"}
            }]
        });
        let answer = answer_from_document(&document).unwrap();
        assert_eq!(answer.storyline, "a walk");
        assert_eq!(answer.shots.len(), 1);
        assert_eq!(answer.shots[0].description, "a");
    }
}
