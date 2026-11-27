//! Messages and entity events. Cross-system streams are messages; per-entity reactions are
//! entity events (ECS §7). The distinction is the 0.20 API's, not a convention.

use bevy_ecs::prelude::{Entity, EntityEvent, Message};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::command::{ActionCall, Outcome};
use crate::component::CameraMode;
use crate::entity::{CallId, InteractableId};
use crate::projection::{Delta, Snapshot};

/// Player input. A held move is pushed every frame by the shell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Input {
    Move {
        x: f32,
        z: f32,
    },
    Stop,
    Interact,
    Select(InteractableId),
    ClearSelection,
    OpenSheet(InteractableId),
    CloseSheet,
    SetCamera {
        mode: CameraMode,
    },
    Submit {
        action: String,
        input: Value,
        #[serde(default)]
        if_match: Option<u64>,
        #[serde(default)]
        approval_id: Option<String>,
        #[serde(default)]
        subject: Option<InteractableId>,
    },
}

#[derive(Message, Clone, Debug)]
pub struct InputIn(pub Input);

#[derive(Message, Clone, Debug)]
pub struct SnapshotIn(pub Snapshot);

#[derive(Message, Clone, Debug)]
pub struct DeltaIn(pub Delta);

#[derive(Message, Clone, Debug, Serialize, Deserialize)]
pub struct ReceiptIn {
    pub id: CallId,
    pub outcome: Outcome,
}

/// A submitted call before manifest validation.
#[derive(Message, Clone, Debug)]
pub struct ActionRequested {
    pub call: ActionCall,
    pub subject: Option<InteractableId>,
}

/// A call that passed the client's manifest gate; the outbox drains it.
#[derive(Message, Clone, Debug)]
pub struct ValidatedRequested {
    pub call: ActionCall,
    pub subject: Option<InteractableId>,
}

/// A terminal result for the sheet presenter.
#[derive(Message, Clone, Debug)]
pub struct ActionResolved {
    pub id: CallId,
    pub outcome: Outcome,
}

/// The proximity prompt changed; the frame view and the sheet read the resource.
#[derive(Message, Clone, Debug)]
pub struct PromptChanged {
    pub target: Option<InteractableId>,
    pub text: String,
}

/// The delta sequence had a gap; the shell must refetch the snapshot.
#[derive(Message, Clone, Copy, Debug)]
pub struct ResyncRequested;

/// Entity-local reactions (observers). The systems also derive the same presentation state;
/// these events are the contract for entity-scoped extensions.
#[derive(EntityEvent, Clone, Debug)]
pub struct RecordUpdated {
    pub entity: Entity,
}

#[derive(EntityEvent, Clone, Debug)]
pub struct WallEntered {
    pub entity: Entity,
}

#[derive(EntityEvent, Clone, Debug)]
pub struct SheetOpened {
    pub entity: Entity,
}

#[derive(EntityEvent, Clone, Debug)]
pub struct SheetClosed {
    pub entity: Entity,
}

#[derive(EntityEvent, Clone, Debug)]
pub struct ReceiptApplied {
    pub entity: Entity,
}
