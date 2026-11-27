mod common;

use walk_world::{TIMESTEP, WalkWorld};

#[test]
fn no_schedule_ambiguity() {
    // `WalkWorld::new` sets `ambiguity_detection: Error` and `shuffle_seed: Some(0)` on both
    // schedules; a system pair whose order is not fixed panics on the first run, so a clean
    // advance is the assertion.
    let mut world = WalkWorld::new(common::config(2, 2));
    for _ in 0..5 {
        world.advance(TIMESTEP);
    }
    assert!(!world.view().sprites.is_empty());
}
