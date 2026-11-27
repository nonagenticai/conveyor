//! The outbox: validated calls leave as intents; the receipt is the only way back.

use bevy_ecs::prelude::*;

use crate::command::{Outcome, Problem};
use crate::component::{Interactable, PendingCall};
use crate::message::{ActionResolved, ReceiptIn, ValidatedRequested};
use crate::resource::{LastProblem, Outbox};

pub fn drain_requests(
    mut reader: MessageReader<ValidatedRequested>,
    mut outbox: ResMut<Outbox>,
    mut commands: Commands,
    subjects: Query<(Entity, &Interactable)>,
) {
    for request in reader.read() {
        outbox.queue.push(request.call.clone());
        if let Some(target) = &request.subject
            && let Some((entity, _)) = subjects
                .iter()
                .find(|(_, interactable)| &interactable.target == target)
        {
            commands.entity(entity).insert(PendingCall(request.call.id));
        }
    }
}

pub fn apply_receipt(
    mut reader: MessageReader<ReceiptIn>,
    mut commands: Commands,
    mut last: ResMut<LastProblem>,
    mut resolved: MessageWriter<ActionResolved>,
    pending: Query<(Entity, &PendingCall)>,
) {
    for receipt in reader.read() {
        if let Some((entity, _)) = pending.iter().find(|(_, call)| call.0 == receipt.id) {
            commands.entity(entity).remove::<PendingCall>();
        }
        match &receipt.outcome {
            Outcome::Refused(problem) => {
                last.call = Some(receipt.id);
                last.problem = Some(problem.clone());
            }
            Outcome::Failed(detail) => {
                last.call = Some(receipt.id);
                last.problem = Some(Problem::new(502, "transport", "Failed", detail.clone()));
            }
            Outcome::TimedOut => {
                last.call = Some(receipt.id);
                last.problem = Some(Problem::new(
                    504,
                    "timeout",
                    "Timed out",
                    "the call did not answer before its deadline",
                ));
            }
            Outcome::Receipt(_) => {}
        }
        resolved.write(ActionResolved {
            id: receipt.id,
            outcome: receipt.outcome.clone(),
        });
    }
}
