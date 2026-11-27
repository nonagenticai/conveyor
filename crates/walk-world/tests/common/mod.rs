#![allow(dead_code)]
//! Shared fixtures for the walk-world integration tests. Everything is created through the
//! public API; no test reaches into a private field.

use walk_world::{
    ActionManifest, Entry, Input, InteractableId, ManifestEntry, Record, RecordKey, RecordKind,
    ReplayLog, RoomConfig, Snapshot, TimedEntry, WalkWorld, WallConfig, WorldConfig,
};

pub fn config(rooms: usize, walls: usize) -> WorldConfig {
    let room_count = rooms;
    let rooms: Vec<RoomConfig> = (0..rooms)
        .map(|index| RoomConfig {
            id: format!("room-{index}"),
            label: format!("Room {index}"),
        })
        .collect();
    let walls: Vec<WallConfig> = (0..walls)
        .map(|index| WallConfig {
            route: format!("/route-{index}"),
            room: format!("room-{}", index % room_count.max(1)),
            label: format!("Route {index}"),
            view: String::new(),
        })
        .collect();
    WorldConfig {
        seed: 1,
        rooms,
        walls,
    }
}

pub fn snapshot(generation: u64, ids: &[&str]) -> Snapshot {
    Snapshot {
        generation,
        records: ids
            .iter()
            .map(|id| Record {
                key: RecordKey::new(RecordKind::Card, *id),
                version: 1,
                title: format!("Card {id}"),
                headline: String::new(),
            })
            .collect(),
    }
}

pub fn manifest() -> ActionManifest {
    let mut manifest = ActionManifest::default();
    for (name, effect) in [
        ("programmes.list", "read"),
        ("cards.move", "write"),
        ("cards.archive", "destructive"),
    ] {
        manifest.insert(
            name,
            ManifestEntry {
                effect: effect.into(),
                confirm: "none".into(),
                scopes: vec!["cards:write".into()],
                idempotent: true,
                spec_hash: format!("hash-{name}"),
            },
        );
    }
    manifest
}

pub fn record(id: &str) -> InteractableId {
    InteractableId::Record(RecordKey::new(RecordKind::Card, id))
}

pub fn has_record(world: &WalkWorld, id: &str) -> bool {
    let target = record(id);
    world
        .view()
        .sprites
        .iter()
        .any(|sprite| sprite.id.as_ref() == Some(&target))
}

/// The golden scenario: one room, one wall, one snapshot and two inputs.
pub fn reference_log() -> ReplayLog {
    ReplayLog {
        seed: 7,
        entries: vec![
            TimedEntry {
                tick: 0,
                entry: Entry::Snapshot(snapshot(1, &["a", "b"])),
            },
            TimedEntry {
                tick: 1,
                entry: Entry::Input(Input::Move { x: 0.0, z: -1.0 }),
            },
            TimedEntry {
                tick: 20,
                entry: Entry::Input(Input::Stop),
            },
        ],
    }
}
