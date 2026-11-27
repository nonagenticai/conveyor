mod common;

use std::time::Duration;

use walk_world::wasm;
use walk_world::{Input, WalkWorld};

#[test]
fn wasm_json_roundtrip_matches_native() {
    let config = common::config(1, 1);
    let config_json = serde_json::to_string(&config).expect("config json");
    let snapshot = common::snapshot(1, &["a", "b"]);
    let snapshot_json = serde_json::to_string(&snapshot).expect("snapshot json");
    let input = Input::Move { x: 0.0, z: -1.0 };
    let input_json = serde_json::to_string(&input).expect("input json");

    let mut native = WalkWorld::new(config);
    native.push_snapshot(&snapshot);
    native.advance(Duration::ZERO);
    native.push_input(input);
    native.advance(Duration::from_nanos(16_666_667));

    let mut edge = wasm::new_json(1, &config_json).expect("wasm new");
    wasm::push_snapshot_json(&mut edge, &snapshot_json).expect("wasm snapshot");
    wasm::advance_json(&mut edge, 0.0);
    wasm::push_input_json(&mut edge, &input_json).expect("wasm input");
    wasm::advance_json(&mut edge, 16.666667);

    assert_eq!(
        wasm::view_json(&edge),
        serde_json::to_string(&native.view()).expect("native view json"),
        "the wasm JSON surface and the native API produce the same frame view"
    );
}
