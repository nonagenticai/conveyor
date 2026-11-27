//! Projection: the only writer of the mirror. Applying the same snapshot twice leaves every
//! component and the key set identical; a missing record survives one generation.

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::component::{
    AbsentSince, Headline, Highlight, Interactable, Position, Title, Vec3, Version,
};
use crate::entity::{InteractableId, RecordKey, RecordKind, RecordRef};
use crate::message::{DeltaIn, ResyncRequested, SnapshotIn};
use crate::resource::{RecordIndex, SessionState, SnapshotGeneration};

/// One mirrored record, as the shell fetched it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub key: RecordKey,
    pub version: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub headline: String,
}

/// A full read-model snapshot with its generation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub generation: u64,
    pub records: Vec<Record>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaOp {
    Upsert,
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaKind {
    Entity,
    Relation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Delta {
    pub seq: u64,
    #[serde(default)]
    pub kind: Option<DeltaKind>,
    pub op: DeltaOp,
    #[serde(default)]
    pub record: Option<Record>,
}

/// The last delta sequence the ECS applied; zero means "no delta yet".
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LastSeq(pub u64);

/// A deterministic slot for a record inside its room; no programme state is implied.
fn record_position(key: &RecordKey) -> Position {
    let kind = match key.kind {
        RecordKind::Project => 1u64,
        RecordKind::Tranche => 2,
        RecordKind::Card => 3,
        RecordKind::RegisterItem => 4,
        RecordKind::Decision => 5,
        RecordKind::Artifact => 6,
        RecordKind::Meeting => 7,
    };
    let mut hash = 0xcbf29ce484222325u64 ^ kind;
    for byte in key.id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let x = ((hash >> 8) % 5) as f32 * 0.25 - 0.5;
    let z = ((hash >> 16) % 5) as f32 * 0.25 - 0.5;
    Position(Vec3::new(x, 0.15, z))
}

fn upsert_components(record: &Record) -> impl Bundle {
    (
        RecordRef(record.key.clone()),
        Title(record.title.clone()),
        Headline(record.headline.clone()),
        Version(record.version),
        Interactable {
            target: InteractableId::Record(record.key.clone()),
        },
        Highlight::default(),
        record_position(&record.key),
    )
}

/// Apply a full snapshot. Upserts by key, marks the unseen with `AbsentSince`; never despawns.
pub fn apply_snapshot(
    mut reader: MessageReader<SnapshotIn>,
    mut commands: Commands,
    mut index: ResMut<RecordIndex>,
    mut generation: ResMut<SnapshotGeneration>,
    existing: Query<(Entity, &RecordRef, Option<&AbsentSince>)>,
) {
    for SnapshotIn(snapshot) in reader.read() {
        let mut seen: BTreeSet<RecordKey> = BTreeSet::new();
        for record in &snapshot.records {
            seen.insert(record.key.clone());
            if let Some(entity) = index.entities.get(&record.key).copied() {
                commands.entity(entity).insert(upsert_components(record));
            } else {
                let entity = commands.spawn(upsert_components(record)).id();
                index.entities.insert(record.key.clone(), entity);
            }
        }
        for (entity, reference, absent) in &existing {
            if !seen.contains(&reference.0) && absent.is_none() {
                commands
                    .entity(entity)
                    .insert(AbsentSince(snapshot.generation));
            }
        }
        generation.generation = snapshot.generation;
    }
}

/// Apply one SSE delta. A sequence gap asks the shell for a fresh snapshot.
pub fn apply_delta(
    mut reader: MessageReader<DeltaIn>,
    mut commands: Commands,
    mut index: ResMut<RecordIndex>,
    generation: Res<SnapshotGeneration>,
    mut last: ResMut<LastSeq>,
    mut resync: MessageWriter<ResyncRequested>,
) {
    for DeltaIn(delta) in reader.read() {
        if last.0 != 0 && delta.seq > last.0.saturating_add(1) {
            resync.write(ResyncRequested);
        }
        last.0 = last.0.max(delta.seq);
        match delta.op {
            DeltaOp::Upsert => {
                if let Some(record) = &delta.record {
                    if let Some(entity) = index.entities.get(&record.key).copied() {
                        commands.entity(entity).insert(upsert_components(record));
                    } else {
                        let entity = commands.spawn(upsert_components(record)).id();
                        index.entities.insert(record.key.clone(), entity);
                    }
                }
            }
            DeltaOp::Remove => {
                if let Some(record) = &delta.record
                    && let Some(entity) = index.entities.remove(&record.key)
                {
                    commands
                        .entity(entity)
                        .insert(AbsentSince(generation.generation.max(1)));
                }
            }
        }
    }
}

/// Despawn records absent for two generations, except the sheet's pinned subject.
pub fn despawn_absent(
    mut commands: Commands,
    mut index: ResMut<RecordIndex>,
    generation: Res<SnapshotGeneration>,
    session: Res<SessionState>,
    absorbed: Query<(Entity, &RecordRef, &AbsentSince)>,
) {
    for (entity, reference, absent) in &absorbed {
        if generation.generation.saturating_sub(absent.0) >= 1 {
            let pinned =
                session.sheet.as_ref() == Some(&InteractableId::Record(reference.0.clone()));
            if pinned {
                continue;
            }
            index.entities.remove(&reference.0);
            commands.entity(entity).despawn();
        }
    }
}

/// The pinned key survives even when the shell never sends another snapshot.
pub fn pinned_keys(session: &SessionState) -> BTreeMap<RecordKey, ()> {
    match &session.sheet {
        Some(InteractableId::Record(key)) => BTreeMap::from([(key.clone(), ())]),
        _ => BTreeMap::new(),
    }
}
