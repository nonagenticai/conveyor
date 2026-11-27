//! Intent: input becomes a movement intent or a submitted call; the manifest is the gate.

use bevy_ecs::prelude::*;

use crate::command::{ActionCall, Outcome, Problem};
use crate::component::Vec2;
use crate::message::{ActionRequested, ActionResolved, Input, InputIn, ValidatedRequested};
use crate::resource::{
    ActionManifest, CallSeq, CameraRig, Intent, LastProblem, PromptState, SessionState,
};

pub fn read_input(
    mut reader: MessageReader<InputIn>,
    mut intent: ResMut<Intent>,
    mut session: ResMut<SessionState>,
    mut camera: ResMut<CameraRig>,
    mut sequence: ResMut<CallSeq>,
    mut requests: MessageWriter<ActionRequested>,
    prompt: Res<PromptState>,
) {
    for InputIn(input) in reader.read() {
        match input {
            Input::Move { x, z } => intent.direction = Vec2::new(*x, *z),
            Input::Stop => intent.direction = Vec2::default(),
            Input::Interact => {
                if let Some(target) = prompt.target.clone() {
                    session.sheet = Some(target);
                }
            }
            Input::Select(target) => session.selection = Some(target.clone()),
            Input::ClearSelection => session.selection = None,
            Input::OpenSheet(target) => session.sheet = Some(target.clone()),
            Input::CloseSheet => session.sheet = None,
            Input::SetCamera { mode } => camera.mode = *mode,
            Input::Submit {
                action,
                input,
                if_match,
                approval_id,
                subject,
            } => {
                let id = sequence.take();
                requests.write(ActionRequested {
                    call: ActionCall {
                        id,
                        action: action.clone(),
                        input: input.clone(),
                        idempotency_key: format!("walk-{}", id.0),
                        if_match: *if_match,
                        approval_id: approval_id.clone(),
                    },
                    subject: subject.clone(),
                });
            }
        }
    }
}

pub fn validate_against_manifest(
    mut reader: MessageReader<ActionRequested>,
    manifest: Res<ActionManifest>,
    mut valid: MessageWriter<ValidatedRequested>,
    mut resolved: MessageWriter<ActionResolved>,
    mut last: ResMut<LastProblem>,
) {
    for request in reader.read() {
        let refusal = match manifest.get(&request.call.action) {
            None => Some(Problem::new(
                403,
                "not_offered",
                "Forbidden",
                format!(
                    "{} is absent from the manifest; the control is not offered",
                    request.call.action
                ),
            )),
            Some(entry) => {
                let gated = entry.effect == "destructive"
                    || entry.confirm == "card"
                    || entry.confirm == "card-typed";
                if gated && request.call.approval_id.is_none() {
                    Some(Problem::new(
                        403,
                        "approval_required",
                        "Forbidden",
                        format!(
                            "{} is {}: an approved approval must be named in `approval_id`",
                            request.call.action, entry.effect
                        ),
                    ))
                } else {
                    None
                }
            }
        };
        match refusal {
            Some(problem) => {
                last.call = Some(request.call.id);
                last.problem = Some(problem.clone());
                resolved.write(ActionResolved {
                    id: request.call.id,
                    outcome: Outcome::Refused(problem),
                });
            }
            None => {
                valid.write(ValidatedRequested {
                    call: request.call.clone(),
                    subject: request.subject.clone(),
                });
            }
        }
    }
}
