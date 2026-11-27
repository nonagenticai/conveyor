//! The Gemba Walk world: a deterministic simulation behind the `WorldView` port.
//!
//! The three state classes are the module boundary (ECS §1): the mirror (`projection`), the
//! session (`session`/`resource`), and the derived (`system`/`view`). A write leaves as an
//! [`ActionCall`]; only a receipt or a delta changes the mirror.

pub mod command;
pub mod component;
pub mod entity;
pub mod message;
pub mod projection;
pub mod replay;
pub mod resource;
pub mod session;
pub mod system;
pub mod view;
pub mod wasm;

use std::time::Duration;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::{LogLevel, ScheduleBuildSettings};
use bevy_time::{Fixed, Time};

pub use command::{ActionCall, CallState, Outcome, Problem};
pub use component::{Avatar, CameraMode, Position, Vec2, Vec3};
pub use entity::{
    CallId, InteractableId, RecordKey, RecordKind, RoomKey, RouteId, SceneKey, SeatId,
};
pub use message::Input;
pub use projection::{Delta, DeltaKind, DeltaOp, Record, Snapshot};
pub use replay::{Entry, ReplayLog, TimedEntry, replay};
pub use resource::{
    ActionManifest, ManifestEntry, RoomConfig, SceneIndex, SessionState, WallConfig, WorldConfig,
};
pub use session::{CameraSnapshot, SessionSnapshot};
pub use view::{ActionInfo, FrameView, SheetContext, hash_view};

use projection::LastSeq;
use resource::{
    CallSeq, CameraRig, FrameViewRes, Intent, LastProblem, Outbox, PromptState, Rand64,
    RecordIndex, SnapshotGeneration, WorldClock,
};

/// The fixed timestep every replay and test uses.
pub const TIMESTEP: Duration = Duration::from_nanos(16_666_667);

/// The portable world handle. Nothing mutates the world outside the schedules.
pub struct WalkWorld {
    app: App,
}

impl WalkWorld {
    pub fn new(config: WorldConfig) -> Self {
        let mut app = App::new();
        app.add_plugins(bevy_time::TimePlugin);
        app.insert_resource(Time::<Fixed>::from_hz(60.0));
        app.add_plugins(WalkWorldPlugin);
        app.insert_resource(config.clone());

        let settings = ScheduleBuildSettings {
            ambiguity_detection: LogLevel::Error,
            shuffle_seed: Some(0),
            ..Default::default()
        };
        let _ = app.edit_schedule(FixedUpdate, |schedule| {
            schedule.set_build_settings(settings.clone());
        });
        let _ = app.edit_schedule(Update, |schedule| {
            schedule.set_build_settings(settings.clone());
        });

        spawn_scene(app.world_mut(), &config);
        Self { app }
    }

    /// Advance the fixed simulation (clamped to four ticks) and then run presentation once.
    pub fn advance(&mut self, dt: Duration) {
        self.app.world_mut().run_schedule(First);
        let timestep = self.app.world().resource::<Time<Fixed>>().timestep();
        let ticks = if dt > Duration::ZERO {
            ((dt.as_secs_f64() / timestep.as_secs_f64()).floor() as u32).max(1)
        } else {
            0
        };
        for _ in 0..ticks.min(4) {
            self.app.world_mut().resource_mut::<WorldClock>().tick += 1;
            self.app.world_mut().run_schedule(FixedUpdate);
            self.app
                .world_mut()
                .resource_mut::<Time<Fixed>>()
                .advance_by(timestep);
        }
        self.app.world_mut().run_schedule(Update);
    }

    pub fn push_snapshot(&mut self, snapshot: &Snapshot) {
        self.app
            .world_mut()
            .resource_mut::<Messages<message::SnapshotIn>>()
            .write(message::SnapshotIn(snapshot.clone()));
    }

    pub fn push_delta(&mut self, delta: &Delta) {
        self.app
            .world_mut()
            .resource_mut::<Messages<message::DeltaIn>>()
            .write(message::DeltaIn(delta.clone()));
    }

    /// The ECS design's name for a streamed delta.
    pub fn push_event(&mut self, delta: &Delta) {
        self.push_delta(delta);
    }

    pub fn push_input(&mut self, input: Input) {
        self.app
            .world_mut()
            .resource_mut::<Messages<message::InputIn>>()
            .write(message::InputIn(input));
    }

    pub fn resolve(&mut self, id: CallId, outcome: Outcome) {
        self.app
            .world_mut()
            .resource_mut::<Messages<message::ReceiptIn>>()
            .write(message::ReceiptIn { id, outcome });
    }

    pub fn set_manifest(&mut self, manifest: ActionManifest) {
        self.app.insert_resource(manifest);
    }

    pub fn drain_calls(&mut self) -> Vec<ActionCall> {
        let mut outbox = self.app.world_mut().resource_mut::<Outbox>();
        std::mem::take(&mut outbox.queue)
    }

    pub fn view(&self) -> FrameView {
        self.app.world().resource::<FrameViewRes>().0.clone()
    }

    pub fn context(&self, target: InteractableId) -> SheetContext {
        let world = self.app.world();
        let mut context = SheetContext {
            interactable: Some(target.clone()),
            ..Default::default()
        };
        match &target {
            InteractableId::Record(key) => {
                if let Some(entity) = world.resource::<RecordIndex>().entities.get(key).copied() {
                    if let Some(title) = world.get::<component::Title>(entity) {
                        context.title = title.0.clone();
                    }
                    if let Some(headline) = world.get::<component::Headline>(entity) {
                        context.headline = headline.0.clone();
                    }
                    if let Some(version) = world.get::<component::Version>(entity) {
                        context.version = Some(version.0);
                    }
                    if let Some(pending) = world.get::<component::PendingCall>(entity) {
                        context.pending = Some(pending.0);
                    }
                }
            }
            InteractableId::Wall(route) => {
                if let Some(entity) = world.resource::<SceneIndex>().walls.get(route).copied()
                    && let Some(wall) = world.get::<component::Wall>(entity)
                {
                    context.title = wall.label.clone();
                }
            }
            InteractableId::Room(key) => {
                if let Some(entity) = world.resource::<SceneIndex>().rooms.get(key).copied()
                    && let Some(room) = world.get::<component::Room>(entity)
                {
                    context.title = room.label.clone();
                }
            }
        }
        let manifest = world.resource::<ActionManifest>();
        context.actions = manifest
            .entries
            .iter()
            .map(|(name, entry)| ActionInfo {
                name: name.clone(),
                effect: entry.effect.clone(),
                confirm: entry.confirm.clone(),
                scopes: entry.scopes.clone(),
                idempotent: entry.idempotent,
            })
            .collect();
        if let Some(problem) = &world.resource::<LastProblem>().problem {
            context.problem = Some(problem.clone());
        }
        context
    }

    pub fn save(&self) -> SessionSnapshot {
        let session = self.app.world().resource::<SessionState>();
        let rig = self.app.world().resource::<CameraRig>();
        SessionSnapshot {
            seat: session.seat,
            sheet: session.sheet.clone(),
            selection: session.selection.clone(),
            camera: CameraSnapshot {
                mode: rig.mode,
                distance: rig.distance,
            },
        }
    }

    pub fn load(&mut self, snapshot: &SessionSnapshot) {
        let world = self.app.world_mut();
        {
            let mut session = world.resource_mut::<SessionState>();
            session.seat = snapshot.seat;
            session.sheet = snapshot.sheet.clone();
            session.selection = snapshot.selection.clone();
        }
        {
            let mut rig = world.resource_mut::<CameraRig>();
            rig.mode = snapshot.camera.mode;
            rig.distance = snapshot.camera.distance;
        }
    }
}

/// The plugin: resources, messages, sets and systems, in the ECS design's order.
pub struct WalkWorldPlugin;

impl Plugin for WalkWorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WorldClock::default())
            .insert_resource(ActionManifest::default())
            .insert_resource(Outbox::default())
            .insert_resource(SnapshotGeneration::default())
            .insert_resource(SessionState::default())
            .insert_resource(CameraRig::default())
            .insert_resource(FrameViewRes::default())
            .insert_resource(Rand64::new(1))
            .insert_resource(PromptState::default())
            .insert_resource(LastProblem::default())
            .insert_resource(Intent::default())
            .insert_resource(CallSeq::default())
            .insert_resource(RecordIndex::default())
            .insert_resource(SceneIndex::default())
            .insert_resource(LastSeq::default())
            .add_message::<message::InputIn>()
            .add_message::<message::SnapshotIn>()
            .add_message::<message::DeltaIn>()
            .add_message::<message::ReceiptIn>()
            .add_message::<message::ActionRequested>()
            .add_message::<message::ValidatedRequested>()
            .add_message::<message::ActionResolved>()
            .add_message::<message::PromptChanged>()
            .add_message::<message::ResyncRequested>()
            .configure_sets(
                FixedUpdate,
                (
                    system::IntentSet,
                    system::LocomotionSet,
                    system::CollisionSet,
                    system::InteractionSet,
                    system::CameraSet,
                    system::AnimationSet,
                    system::MoodSet,
                    system::OutboxSet,
                )
                    .chain(),
            )
            .configure_sets(
                Update,
                (system::ProjectionSet, system::RenderPrepareSet).chain(),
            )
            .add_systems(
                FixedUpdate,
                (
                    system::intent::read_input.in_set(system::IntentSet),
                    system::intent::validate_against_manifest.in_set(system::IntentSet),
                    system::locomotion::apply_intent.in_set(system::LocomotionSet),
                    system::collision::integrate.in_set(system::CollisionSet),
                    system::collision::resolve_walls.in_set(system::CollisionSet),
                    system::interaction::resolve_proximity.in_set(system::InteractionSet),
                    system::interaction::resolve_pick.in_set(system::InteractionSet),
                    system::camera::follow.in_set(system::CameraSet),
                    system::camera::boom_collision.in_set(system::CameraSet),
                    system::animation::advance_phase.in_set(system::AnimationSet),
                    system::animation::apply_moodlets_expiry.in_set(system::AnimationSet),
                    system::mood::derive_mood.in_set(system::MoodSet),
                    system::mood::derive_badges.in_set(system::MoodSet),
                    system::outbox::drain_requests.in_set(system::OutboxSet),
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    projection::apply_snapshot,
                    projection::apply_delta,
                    system::outbox::apply_receipt,
                    projection::despawn_absent,
                )
                    .chain()
                    .in_set(system::ProjectionSet),
            )
            .add_systems(
                Update,
                view::build_frame_view.in_set(system::RenderPrepareSet),
            );
    }
}

/// Spawn the rooms, the walls and the one avatar. The layout is deterministic from the config
/// order; the census supplies the ids and labels, never the geometry.
fn spawn_scene(world: &mut World, config: &WorldConfig) {
    let layout = |index: usize| {
        let column = (index % 4) as f32;
        let row = (index / 4) as f32;
        Vec3::new(column * 4.0 - 6.0, 0.0, row * 4.0 - 2.0)
    };

    let mut scene = SceneIndex::default();
    for (index, room) in config.rooms.iter().enumerate() {
        let key = RoomKey(index as u8);
        let entity = world
            .spawn((
                entity::SceneRef(SceneKey(index as u64 + 1)),
                component::Room {
                    key,
                    label: room.label.clone(),
                    size: Vec2::new(3.0, 3.0),
                },
                component::Position(layout(index)),
                component::Interactable {
                    target: InteractableId::Room(key),
                },
                component::Highlight::default(),
            ))
            .id();
        scene.rooms.insert(key, entity);
    }

    let mut per_room: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (index, wall) in config.walls.iter().enumerate() {
        let room_index = config
            .rooms
            .iter()
            .position(|room| room.id == wall.room)
            .unwrap_or(0);
        let center = if config.rooms.is_empty() {
            Vec3::default()
        } else {
            layout(room_index)
        };
        let within = {
            let counter = per_room.entry(wall.room.clone()).or_insert(0);
            let value = *counter;
            *counter += 1;
            value
        };
        let route = RouteId(index as u16);
        let label = if wall.label.is_empty() {
            wall.view.clone()
        } else {
            wall.label.clone()
        };
        let entity = world
            .spawn((
                entity::SceneRef(SceneKey(1000 + index as u64)),
                component::Wall { route, label },
                component::Position(Vec3::new(
                    center.x + (within as f32 - 1.0) * 0.6,
                    0.4,
                    center.z - 1.45,
                )),
                component::Collider {
                    half_extents: Vec2::new(0.28, 0.08),
                },
                component::Interactable {
                    target: InteractableId::Wall(route),
                },
                component::Highlight::default(),
            ))
            .id();
        scene.walls.insert(route, entity);
    }

    let start = if config.rooms.is_empty() {
        Vec3::default()
    } else {
        layout(0)
    };
    world.spawn((
        entity::SceneRef(SceneKey(5000)),
        Avatar,
        component::Position(start),
        component::Velocity::default(),
        component::Facing::default(),
        component::Collider {
            half_extents: Vec2::new(0.3, 0.3),
        },
        component::InRoom(if config.rooms.is_empty() {
            None
        } else {
            Some(RoomKey(0))
        }),
        component::AnimPhase::default(),
        component::Mood::default(),
        component::Badge::default(),
        component::SeatPreset(SeatId(0)),
        component::Highlight::default(),
    ));
    world.insert_resource(CameraRig {
        mode: CameraMode::Third,
        focus: start,
        position: start + Vec3::new(0.0, 3.0, 4.0),
        distance: 4.0,
    });
    world.insert_resource(scene);
}
