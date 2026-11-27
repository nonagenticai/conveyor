//! Components. Each field is written by the system named in the ECS design §4; long-form
//! content never enters a component, only what the world draws or the command path needs.

use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};

use crate::entity::{CallId, RoomKey, RouteId, SeatId};

/// A tiny local vector; the crate deliberately avoids the `bevy_math` dependency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn scale(self, factor: f32) -> Self {
        Self::new(self.x * factor, self.y * factor, self.z * factor)
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.z * self.z).sqrt()
    }

    pub fn normalized(self) -> Self {
        let length = self.length();
        if length <= f32::EPSILON {
            Self::default()
        } else {
            Self::new(self.x / length, 0.0, self.z / length)
        }
    }
}

impl std::ops::Add for Vec3 {
    type Output = Vec3;

    fn add(self, other: Vec3) -> Vec3 {
        Vec3::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Vec3;

    fn sub(self, other: Vec3) -> Vec3 {
        Vec3::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

/// A tiny local 2-vector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub z: f32,
}

impl Vec2 {
    pub const fn new(x: f32, z: f32) -> Self {
        Self { x, z }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Position(pub Vec3);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Velocity(pub Vec3);

/// The horizontal facing, a unit vector; no angle keeps the simulation free of transcendentals.
#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Facing(pub Vec2);

impl Default for Facing {
    fn default() -> Self {
        Self(Vec2::new(0.0, -1.0))
    }
}

/// A wall's axis-aligned half-extents in the horizontal plane.
#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Collider {
    pub half_extents: Vec2,
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Avatar;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InRoom(pub Option<RoomKey>);

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub route: RouteId,
    pub label: String,
}

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub key: RoomKey,
    pub label: String,
    pub size: Vec2,
}

#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interactable {
    pub target: crate::entity::InteractableId,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Highlight {
    pub on: bool,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Badge {
    pub count: u16,
    pub kind: String,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Mood {
    pub valence: f32,
    pub label: String,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Moodlet {
    pub expires_at_tick: u64,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Version(pub u64);

/// A short label the world may draw; long-form content stays in the sheet's fetch.
#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Title(pub String);

#[derive(Component, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Headline(pub String);

/// A call this entity is waiting on; presentation only.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingCall(pub CallId);

/// The first generation in which this mirrored entity was absent from a snapshot.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbsentSince(pub u64);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnimPhase(pub f32);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatPreset(pub SeatId);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prop;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Advisor(pub SeatId);

/// First or third person; pure session state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraMode {
    #[default]
    Third,
    First,
}
