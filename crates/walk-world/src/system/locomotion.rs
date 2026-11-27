//! Locomotion: the intent becomes a velocity and a facing; no trigonometry.

use bevy_ecs::prelude::*;

use crate::component::{Avatar, Facing, Vec2, Vec3, Velocity};
use crate::resource::Intent;

pub const SPEED: f32 = 3.0;

pub fn apply_intent(
    intent: Res<Intent>,
    mut avatar: Query<(&mut Velocity, &mut Facing), With<Avatar>>,
) {
    let Ok((mut velocity, mut facing)) = avatar.single_mut() else {
        return;
    };
    let direction = intent.direction;
    let length = (direction.x * direction.x + direction.z * direction.z).sqrt();
    if length <= 0.01 {
        velocity.0 = Vec3::default();
        return;
    }
    let unit = Vec2::new(direction.x / length, direction.z / length);
    velocity.0 = Vec3::new(unit.x * SPEED, 0.0, unit.z * SPEED);
    facing.0 = unit;
}
