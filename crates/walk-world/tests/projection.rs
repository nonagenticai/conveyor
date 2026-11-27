mod common;

use std::time::Duration;

use walk_world::{Input, TIMESTEP, WalkWorld};

#[test]
fn projection_is_idempotent() {
    let mut world = WalkWorld::new(common::config(1, 0));
    let snapshot = common::snapshot(1, &["a", "b"]);

    world.push_snapshot(&snapshot);
    world.advance(Duration::ZERO);
    let first = serde_json::to_string(&world.view()).expect("view json");

    world.push_snapshot(&snapshot);
    world.advance(Duration::ZERO);
    let second = serde_json::to_string(&world.view()).expect("view json");

    assert_eq!(
        first, second,
        "applying the same snapshot twice must be a no-op"
    );
}

#[test]
fn despawn_requires_two_generations() {
    let mut world = WalkWorld::new(common::config(1, 0));
    world.push_snapshot(&common::snapshot(1, &["a"]));
    world.advance(Duration::ZERO);
    assert!(common::has_record(&world, "a"));

    world.push_snapshot(&common::snapshot(2, &[]));
    world.advance(Duration::ZERO);
    assert!(
        common::has_record(&world, "a"),
        "one absent generation survives"
    );

    world.push_snapshot(&common::snapshot(3, &[]));
    world.advance(Duration::ZERO);
    assert!(
        !common::has_record(&world, "a"),
        "two absent generations despawn"
    );
}

#[test]
fn sheet_subject_is_pinned() {
    let mut world = WalkWorld::new(common::config(1, 0));
    world.push_snapshot(&common::snapshot(1, &["a"]));
    world.advance(Duration::ZERO);

    world.push_input(Input::OpenSheet(common::record("a")));
    world.advance(TIMESTEP);
    assert!(world.save().sheet.is_some());

    for generation in 2..=4 {
        world.push_snapshot(&common::snapshot(generation, &[]));
        world.advance(Duration::ZERO);
        assert!(
            common::has_record(&world, "a"),
            "the sheet's subject is pinned across generation {generation}"
        );
    }

    world.push_input(Input::CloseSheet);
    world.advance(TIMESTEP);
    world.push_snapshot(&common::snapshot(5, &[]));
    world.advance(Duration::ZERO);
    assert!(
        !common::has_record(&world, "a"),
        "after the close the guard despawns it"
    );
}
