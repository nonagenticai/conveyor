//! Identity: keys, not allocation order. A mirrored entity carries a [`RecordKey`]; a
//! scene-only entity carries a [`SceneKey`]. The shell exchanges [`InteractableId`]s with the
//! renderer; it never invents one.

use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};

/// The source table a mirrored record belongs to (ECS §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Project,
    Tranche,
    Card,
    RegisterItem,
    Decision,
    Artifact,
    Meeting,
}

/// The stable identity of one mirrored record.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordKey {
    pub kind: RecordKind,
    pub id: String,
}

impl RecordKey {
    pub fn new(kind: RecordKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }
}

/// A scene-only identity (rooms, walls, portals, figures, props). It is not a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SceneKey(pub u64);

/// The index of one wall in the frozen census route table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RouteId(pub u16);

/// The index of one room in the eight-wing world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RoomKey(pub u8);

/// The index of one seat preset (SPEC §3).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct SeatId(pub u8);

/// The identity of one submitted action call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CallId(pub u64);

/// What the renderer may pick. A wall opens a destination; a record opens its sheet.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum InteractableId {
    Record(RecordKey),
    Wall(RouteId),
    Room(RoomKey),
}

/// The RecordKey component on a mirrored entity (reverse lookup lives in [`crate::resource::RecordIndex`]).
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
pub struct RecordRef(pub RecordKey);

/// The SceneKey component on a scene-only entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneRef(pub SceneKey);
