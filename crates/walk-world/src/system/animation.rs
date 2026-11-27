//! Animation: the phase is a function of the tick; moodlets expire by tick.

use bevy_ecs::prelude::*;

use crate::component::{AnimPhase, Avatar, Moodlet, Prop};
use crate::resource::WorldClock;

pub fn advance_phase(clock: Res<WorldClock>, mut avatar: Query<&mut AnimPhase, With<Avatar>>) {
    let Ok(mut phase) = avatar.single_mut() else {
        return;
    };
    phase.0 = (clock.tick % 120) as f32 / 120.0;
}

#[allow(clippy::type_complexity)]
pub fn apply_moodlets_expiry(
    clock: Res<WorldClock>,
    mut commands: Commands,
    moodlets: Query<(Entity, &Moodlet), Or<(With<Avatar>, With<Prop>)>>,
) {
    for (entity, moodlet) in &moodlets {
        if moodlet.expires_at_tick <= clock.tick {
            commands.entity(entity).remove::<Moodlet>();
        }
    }
}
