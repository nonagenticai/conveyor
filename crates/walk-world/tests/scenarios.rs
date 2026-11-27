mod common;

use std::time::Duration;

use walk_world::{Input, InteractableId, TIMESTEP, WalkWorld};

#[test]
fn walk_to_wall_prompts() {
    let mut world = WalkWorld::new(common::config(1, 1));
    world.advance(Duration::ZERO);

    let wall_id = world
        .view()
        .sprites
        .iter()
        .find(|sprite| matches!(sprite.id, Some(InteractableId::Wall(_))))
        .and_then(|sprite| sprite.id.clone())
        .expect("the config spawned a wall");
    assert!(matches!(wall_id, InteractableId::Wall(_)));

    for _ in 0..40 {
        world.push_input(Input::Move { x: 0.0, z: -1.0 });
        world.advance(TIMESTEP);
    }

    let view = world.view();
    let prompt = view
        .overlays
        .iter()
        .find(|overlay| overlay.kind == "prompt")
        .expect("walking into the wall raises a prompt");
    assert_eq!(prompt.id.as_ref(), Some(&wall_id));
    assert!(prompt.text.contains("Route 0"), "{}", prompt.text);
}
