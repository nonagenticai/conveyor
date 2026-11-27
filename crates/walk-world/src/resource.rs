//! Resources: the only clock, the manifest, the outbox, the generation guard, the session and
//! the last frame view.

use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

use crate::command::{ActionCall, Problem};
use crate::component::{CameraMode, Vec2, Vec3};
use crate::entity::{CallId, InteractableId, RoomKey, RouteId, SeatId};
use crate::view::FrameView;
use bevy_ecs::entity::Entity;
use std::collections::BTreeMap;

/// The only clock a system may read.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldClock {
    pub tick: u64,
}

/// One action's gate data, loaded from `actions.describe` by the shell.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub effect: String,
    pub confirm: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub idempotent: bool,
    #[serde(default)]
    pub spec_hash: String,
}

/// The manifest is the UI's contract: an action absent here is not offered.
#[derive(Resource, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActionManifest {
    pub entries: BTreeMap<String, ManifestEntry>,
}

impl ActionManifest {
    pub fn get(&self, name: &str) -> Option<&ManifestEntry> {
        self.entries.get(name)
    }

    pub fn insert(&mut self, name: impl Into<String>, entry: ManifestEntry) {
        self.entries.insert(name.into(), entry);
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Intents awaiting the shell's transport; drained by `WalkWorld::drain_calls`.
#[derive(Resource, Clone, Debug, Default)]
pub struct Outbox {
    pub queue: Vec<ActionCall>,
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnapshotGeneration {
    pub generation: u64,
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraRig {
    pub mode: CameraMode,
    pub focus: Vec3,
    pub position: Vec3,
    pub distance: f32,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub seat: SeatId,
    pub sheet: Option<InteractableId>,
    pub selection: Option<InteractableId>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct FrameViewRes(pub FrameView);

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rand64 {
    state: u64,
}

impl Rand64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
}

/// The proximity prompt the frame view draws and the interaction sheet opens.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct PromptState {
    pub target: Option<InteractableId>,
    pub text: String,
}

/// The last refusal or transport failure, surfaced on the subject and in the sheet.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct LastProblem {
    pub call: Option<CallId>,
    pub problem: Option<Problem>,
}

/// The movement intent for this tick; written by `read_input`, consumed by `apply_intent`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Intent {
    pub direction: Vec2,
}

/// The monotonic call counter; a call id is stable across a retry.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CallSeq {
    pub next: u64,
}

impl CallSeq {
    pub fn take(&mut self) -> CallId {
        self.next += 1;
        CallId(self.next)
    }
}

/// The forward index from a record key to its mirrored entity; written only by projection.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordIndex {
    pub entities: BTreeMap<crate::entity::RecordKey, bevy_ecs::entity::Entity>,
}

/// The scene-only reverse index: the route and room entities the sheet needs.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneIndex {
    pub walls: BTreeMap<RouteId, Entity>,
    pub rooms: BTreeMap<RoomKey, Entity>,
}

/// The construction inputs: the census rooms and walls; the crate computes the layout.
#[derive(Resource, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldConfig {
    pub seed: u64,
    #[serde(default)]
    pub rooms: Vec<RoomConfig>,
    #[serde(default)]
    pub walls: Vec<WallConfig>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RoomConfig {
    pub id: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WallConfig {
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub room: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub view: String,
}

impl WorldConfig {
    /// The default eight rooms when the shell has no census yet (tests and the first boot).
    pub fn with_default_rooms() -> Self {
        let rooms = [
            "portfolio",
            "organisation",
            "procedures",
            "governance",
            "knowledge",
            "measure",
            "operations",
            "utilities",
        ]
        .into_iter()
        .map(|id| RoomConfig {
            id: id.to_owned(),
            label: id.to_owned(),
        })
        .collect();
        Self {
            seed: 1,
            rooms,
            walls: Vec::new(),
        }
    }
}
