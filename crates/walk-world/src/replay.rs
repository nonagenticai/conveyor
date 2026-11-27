//! Replay: the same seed, the same entries at the same ticks, the same frame view hash.

use serde::{Deserialize, Serialize};

use crate::entity::CallId;
use crate::message::Input;
use crate::projection::{Delta, Snapshot};
use crate::resource::WorldConfig;
use crate::view::hash_view;
use crate::{TIMESTEP, WalkWorld};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    Input(Input),
    Snapshot(Snapshot),
    Delta(Delta),
    Receipt {
        id: CallId,
        outcome: crate::command::Outcome,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimedEntry {
    pub tick: u64,
    pub entry: Entry,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplayLog {
    pub seed: u64,
    pub entries: Vec<TimedEntry>,
}

/// Replay a log against a fresh world and return the final view hash.
pub fn replay(config: &WorldConfig, log: &ReplayLog) -> String {
    let mut world = WalkWorld::new(WorldConfig {
        seed: log.seed,
        ..config.clone()
    });
    let last = log
        .entries
        .iter()
        .map(|entry| entry.tick)
        .max()
        .unwrap_or(0);
    for tick in 0..=last {
        for timed in log.entries.iter().filter(|entry| entry.tick == tick) {
            match &timed.entry {
                Entry::Input(input) => world.push_input(input.clone()),
                Entry::Snapshot(snapshot) => world.push_snapshot(snapshot),
                Entry::Delta(delta) => world.push_delta(delta),
                Entry::Receipt { id, outcome } => world.resolve(*id, outcome.clone()),
            }
        }
        world.advance(TIMESTEP);
    }
    hash_view(&world.view())
}
