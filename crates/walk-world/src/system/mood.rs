//! Mood and badges are derived from the mirror and the command state; never authored.

use bevy_ecs::prelude::*;

use crate::component::{Avatar, Badge, Mood, Moodlet, PendingCall};
use crate::entity::RecordRef;
use crate::resource::{LastProblem, WorldClock};

pub fn derive_mood(
    clock: Res<WorldClock>,
    last: Res<LastProblem>,
    pending: Query<&PendingCall>,
    mut commands: Commands,
    mut avatar: Query<(Entity, &mut Mood), With<Avatar>>,
) {
    let Ok((entity, mut mood)) = avatar.single_mut() else {
        return;
    };
    let waiting = !pending.is_empty();
    if last.problem.is_some() {
        mood.valence = -1.0;
        mood.label = "needs attention".into();
        commands.entity(entity).insert(Moodlet {
            expires_at_tick: clock.tick + 120,
        });
    } else if waiting {
        mood.valence = 0.5;
        mood.label = "waiting".into();
    } else {
        mood.valence = 1.0;
        mood.label = "steady".into();
    }
}

pub fn derive_badges(records: Query<&RecordRef>, mut avatar: Query<&mut Badge, With<Avatar>>) {
    let Ok(mut badge) = avatar.single_mut() else {
        return;
    };
    badge.count = records.iter().count().min(u16::MAX as usize) as u16;
    badge.kind = "records".into();
}
