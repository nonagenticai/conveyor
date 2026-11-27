//! Camera: third person is the avatar minus its facing; the boom never enters a wall.

use bevy_ecs::prelude::*;

use crate::component::{Avatar, CameraMode, Facing, Position, Vec3, Wall};
use crate::resource::CameraRig;

pub fn follow(avatar: Query<(&Position, &Facing), With<Avatar>>, mut rig: ResMut<CameraRig>) {
    let Ok((position, facing)) = avatar.single() else {
        return;
    };
    rig.focus = position.0;
    let direction = Vec3::new(facing.0.x, 0.0, facing.0.z);
    rig.position = match rig.mode {
        CameraMode::Third => position.0 - direction.scale(rig.distance) + Vec3::new(0.0, 3.0, 0.0),
        CameraMode::First => position.0 + direction.scale(0.15) + Vec3::new(0.0, 1.6, 0.0),
    };
}

pub fn boom_collision(
    avatar: Query<&Position, With<Avatar>>,
    walls: Query<&Position, With<Wall>>,
    mut rig: ResMut<CameraRig>,
) {
    let Ok(position) = avatar.single() else {
        return;
    };
    let mut distance = rig.distance;
    for wall in &walls {
        if (position.0 - wall.0).length() < 1.0 {
            distance = distance.min(1.0);
        }
    }
    // Near a wall the avatar must not fill the screen; 1.9 m is the Walk5 floor.
    rig.distance = distance.max(1.9);
}
