//! The frame view and the sheet context: the renderer's only input and the sheet's only input.
//! The view is quantized so the native and wasm hashes can be compared.

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::command::Problem;
use crate::component::{Avatar, Badge, CameraMode, InRoom, Mood, Position, Room, Title, Wall};
use crate::entity::{CallId, InteractableId, RecordRef, RoomKey};
use crate::resource::{CameraRig, FrameViewRes, PromptState, SessionState};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraView {
    pub mode: CameraMode,
    pub position: [f32; 3],
    pub target: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpriteView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<InteractableId>,
    pub kind: String,
    pub position: [f32; 3],
    pub size: [f32; 2],
    pub facing: f32,
    pub tint: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OverlayView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<InteractableId>,
    pub kind: String,
    pub anchor: [f32; 3],
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameView {
    pub camera: CameraView,
    pub sprites: Vec<SpriteView>,
    pub overlays: Vec<OverlayView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<InteractableId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<RoomKey>,
}

/// One action offered by the sheet, already gated by the manifest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActionInfo {
    pub name: String,
    pub effect: String,
    pub confirm: String,
    pub scopes: Vec<String>,
    pub idempotent: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SheetContext {
    pub interactable: Option<InteractableId>,
    pub title: String,
    pub headline: String,
    pub version: Option<u64>,
    pub actions: Vec<ActionInfo>,
    pub pending: Option<CallId>,
    pub problem: Option<Problem>,
}

fn q(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0
}

fn qv(value: crate::component::Vec3) -> [f32; 3] {
    [q(value.x), q(value.y), q(value.z)]
}

/// Derive the draw list from the scene and the mirror; the adapter never sees components.
#[allow(clippy::too_many_arguments)]
pub fn build_frame_view(
    rig: Res<CameraRig>,
    prompt: Res<PromptState>,
    session: Res<SessionState>,
    rooms: Query<(&Position, &Room)>,
    walls: Query<(&Position, &Wall, &crate::component::Interactable)>,
    records: Query<(&Position, &Title, &RecordRef)>,
    avatar: Query<(&Position, &InRoom, &Mood, &Badge), With<Avatar>>,
    mut frame: ResMut<FrameViewRes>,
) {
    let mut sprites: Vec<SpriteView> = Vec::new();
    let mut overlays: Vec<OverlayView> = Vec::new();
    let mut room_key = None;

    for (position, room) in &rooms {
        sprites.push(SpriteView {
            id: None,
            kind: "room".into(),
            position: qv(position.0),
            size: [q(room.size.x), q(room.size.z)],
            facing: 0.0,
            tint: "#f8efd9".into(),
            label: room.label.clone(),
        });
    }

    for (position, wall, interactable) in &walls {
        sprites.push(SpriteView {
            id: Some(interactable.target.clone()),
            kind: "wall".into(),
            position: qv(position.0),
            size: [0.56, 0.8],
            facing: 0.0,
            tint: "#2f8070".into(),
            label: wall.label.clone(),
        });
    }

    for (position, title, reference) in &records {
        sprites.push(SpriteView {
            id: Some(InteractableId::Record(reference.0.clone())),
            kind: "record".into(),
            position: qv(position.0),
            size: [0.4, 0.4],
            facing: 0.0,
            tint: "#ab7744".into(),
            label: title.0.clone(),
        });
    }

    if let Ok((position, in_room, mood, badge)) = avatar.single() {
        sprites.push(SpriteView {
            id: None,
            kind: "avatar".into(),
            position: qv(position.0),
            size: [0.3, 0.6],
            facing: 0.0,
            tint: "#8fd14f".into(),
            label: "You".into(),
        });
        room_key = in_room.0;
        if !mood.label.is_empty() {
            overlays.push(OverlayView {
                id: None,
                kind: "mood".into(),
                anchor: qv(position.0),
                text: mood.label.clone(),
            });
        }
        if badge.count > 0 {
            overlays.push(OverlayView {
                id: None,
                kind: "badge".into(),
                anchor: qv(position.0),
                text: format!("{} {}", badge.count, badge.kind),
            });
        }
    }

    if let Some(target) = &prompt.target {
        overlays.push(OverlayView {
            id: Some(target.clone()),
            kind: "prompt".into(),
            anchor: [0.0, 0.0, 0.0],
            text: prompt.text.clone(),
        });
    }

    sprites.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.kind.cmp(&b.kind)));

    frame.0 = FrameView {
        camera: CameraView {
            mode: rig.mode,
            position: qv(rig.position),
            target: qv(rig.focus),
        },
        sprites,
        overlays,
        highlight: session.selection.clone(),
        room: room_key,
    };
}

/// The canonical hash both the native replay test and the wasm world spec compare.
pub fn hash_view(view: &FrameView) -> String {
    let json = serde_json::to_string(view).unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&json, &mut hasher);
    format!("{:016x}", std::hash::Hasher::finish(&hasher))
}
