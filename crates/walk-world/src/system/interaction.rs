//! Interaction: the room the avatar occupies, the prompt, and the highlight.

use bevy_ecs::prelude::*;

use crate::component::{Avatar, Highlight, InRoom, Interactable, Position, Room, Wall};
use crate::entity::InteractableId;
use crate::message::PromptChanged;
use crate::resource::{PromptState, SessionState};

pub fn resolve_proximity(
    mut avatar: Query<(&Position, &mut InRoom), With<Avatar>>,
    walls: Query<(&Position, &Wall, &Interactable)>,
    rooms: Query<(&Position, &Room)>,
    mut prompt: ResMut<PromptState>,
    mut changed: MessageWriter<PromptChanged>,
) {
    let Ok((position, mut in_room)) = avatar.single_mut() else {
        return;
    };

    in_room.0 = rooms
        .iter()
        .find(|(room_position, room)| {
            (position.0.x - room_position.0.x).abs() <= room.size.x / 2.0
                && (position.0.z - room_position.0.z).abs() <= room.size.z / 2.0
        })
        .map(|(_, room)| room.key);

    let mut best: Option<(f32, InteractableId, String)> = None;
    for (wall_position, wall, interactable) in &walls {
        let distance = (wall_position.0 - position.0).length();
        if distance > 1.2 {
            continue;
        }
        if best
            .as_ref()
            .map(|(closest, _, _)| distance < *closest)
            .unwrap_or(true)
        {
            best = Some((
                distance,
                interactable.target.clone(),
                format!("Enter {}", wall.label),
            ));
        }
    }

    let target = best.as_ref().map(|(_, target, _)| target.clone());
    let text = best
        .as_ref()
        .map(|(_, _, text)| text.clone())
        .unwrap_or_default();
    if prompt.target != target || prompt.text != text {
        prompt.target = target;
        prompt.text = text;
        changed.write(PromptChanged {
            target: prompt.target.clone(),
            text: prompt.text.clone(),
        });
    }
}

pub fn resolve_pick(
    session: Res<SessionState>,
    mut interactables: Query<(&Interactable, &mut Highlight)>,
) {
    for (interactable, mut highlight) in &mut interactables {
        highlight.on = session.selection.as_ref() == Some(&interactable.target);
    }
}
