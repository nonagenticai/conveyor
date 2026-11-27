mod common;

use std::time::Duration;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_time::TimePlugin;
use serde_json::json;
use walk_world::{Input, Outcome, Problem, TIMESTEP, WalkWorld};

fn submit(world: &mut WalkWorld, action: &str, input: serde_json::Value, id: &str) {
    world.push_input(Input::Submit {
        action: action.into(),
        input,
        if_match: Some(1),
        approval_id: None,
        subject: Some(common::record(id)),
    });
}

#[test]
fn destructive_requires_approval() {
    let mut world = WalkWorld::new(common::config(1, 0));
    world.set_manifest(common::manifest());
    world.push_snapshot(&common::snapshot(1, &["a"]));
    world.advance(Duration::ZERO);

    submit(&mut world, "cards.archive", json!({"id": "a"}), "a");
    world.advance(TIMESTEP);

    assert!(
        world.drain_calls().is_empty(),
        "a destructive call without approval never leaves"
    );
    let context = world.context(common::record("a"));
    let problem = context.problem.expect("the refusal is surfaced");
    assert_eq!(problem.kind, "approval_required");
}

#[test]
fn refusal_clears_pending() {
    let mut world = WalkWorld::new(common::config(1, 0));
    world.set_manifest(common::manifest());
    world.push_snapshot(&common::snapshot(1, &["a"]));
    world.advance(Duration::ZERO);

    submit(
        &mut world,
        "cards.move",
        json!({"id": "a", "list_id": "b"}),
        "a",
    );
    world.advance(TIMESTEP);

    let calls = world.drain_calls();
    assert_eq!(calls.len(), 1, "a write leaves exactly one call");
    assert_eq!(
        world.context(common::record("a")).pending,
        Some(calls[0].id),
        "the subject carries the pending call"
    );

    world.resolve(
        calls[0].id,
        Outcome::Refused(Problem::new(
            403,
            "approval_required",
            "Forbidden",
            "no approval",
        )),
    );
    world.advance(TIMESTEP);

    let context = world.context(common::record("a"));
    assert_eq!(context.pending, None, "the receipt clears the pending call");
    assert!(context.problem.is_some(), "the refusal is shown verbatim");
}

#[derive(Message, Clone, Copy)]
struct Tick(u32);

#[derive(Resource, Default)]
struct Seen(Vec<u32>);

fn writer(mut writer: MessageWriter<Tick>, mut counter: Local<u32>) {
    *counter += 1;
    writer.write(Tick(*counter));
}

fn reader(mut reader: MessageReader<Tick>, mut seen: ResMut<Seen>) {
    for tick in reader.read() {
        seen.0.push(tick.0);
    }
}

#[test]
fn same_frame_ordering() {
    // Reader before writer, unchained by anything but the schedule: the reader sees the write
    // only on the next frame.
    let mut late = App::new();
    late.add_plugins(TimePlugin);
    late.add_message::<Tick>();
    late.init_resource::<Seen>();
    late.add_systems(Update, (reader, writer).chain());
    late.update();
    late.update();
    assert_eq!(late.world().resource::<Seen>().0, vec![1]);

    // Writer before reader, chained: the reader sees the write in the same frame.
    let mut same = App::new();
    same.add_plugins(TimePlugin);
    same.add_message::<Tick>();
    same.init_resource::<Seen>();
    same.add_systems(Update, (writer, reader).chain());
    same.update();
    assert_eq!(same.world().resource::<Seen>().0, vec![1]);
}
