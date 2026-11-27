//! Collision: move, then push out of the walls along the smaller overlap.

use bevy_ecs::prelude::*;

use crate::component::{Avatar, Collider, Position, Velocity, Wall};

/// The fixed timestep the shell and the replay use.
pub const STEP_SECS: f32 = 1.0 / 60.0;

pub fn integrate(mut avatar: Query<(&mut Position, &Velocity), With<Avatar>>) {
    let Ok((mut position, velocity)) = avatar.single_mut() else {
        return;
    };
    position.0 = position.0 + velocity.0.scale(STEP_SECS);
    position.0.y = 0.0;
}

#[allow(clippy::type_complexity)]
pub fn resolve_walls(
    mut avatar: Query<(&mut Position, &Collider), With<Avatar>>,
    walls: Query<(&Position, &Collider), (With<Wall>, Without<Avatar>)>,
) {
    let Ok((mut position, body)) = avatar.single_mut() else {
        return;
    };
    for (wall_position, wall_body) in &walls {
        let dx = position.0.x - wall_position.0.x;
        let dz = position.0.z - wall_position.0.z;
        let overlap_x = body.half_extents.x + wall_body.half_extents.x - dx.abs();
        let overlap_z = body.half_extents.z + wall_body.half_extents.z - dz.abs();
        if overlap_x <= 0.0 || overlap_z <= 0.0 {
            continue;
        }
        if overlap_x < overlap_z {
            position.0.x += if dx >= 0.0 { overlap_x } else { -overlap_x };
        } else {
            position.0.z += if dz >= 0.0 { overlap_z } else { -overlap_z };
        }
    }
}
