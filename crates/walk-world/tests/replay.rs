mod common;

use walk_world::replay;

#[test]
fn replay_is_deterministic() {
    let config = common::config(1, 1);
    let log = common::reference_log();

    let first = replay(&config, &log);
    let second = replay(&config, &log);
    assert_eq!(
        first, second,
        "the same log must produce the same frame-view hash"
    );
    assert!(!first.is_empty());
    println!("reference frame hash: {first}");
}

#[test]
fn reference_frame_hash_is_stable() {
    let config = common::config(1, 1);
    let hash = replay(&config, &common::reference_log());
    let golden = include_str!("../../../docs/evidence/walk-frame.sha256").trim();
    assert_eq!(
        hash, golden,
        "the native frame-view hash drifted from the golden"
    );
}
