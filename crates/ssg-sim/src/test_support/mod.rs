//! Headless test harness shared by the integration and characterization tests.
//!
//! This module is self-contained: it depends only on crate items, bevy, avian3d,
//! brahe, nalgebra and serde_json, and every other test module imports only from
//! it (plus crate items). P3 moves it unchanged into the `ssg-sim` crate.
//!
//! Rules for tests that use it:
//! - Advance time only through [`tick`] / [`ticks`]. Never call `fixed_physics_step`
//!   directly (D8, see below).
//! - Never read the wall clock for the sim epoch; use [`fixed_epoch`] (F-06).

// Several helpers are first used by the S2-S4 characterization tests.
#![allow(dead_code)]

use std::time::Duration;

use avian3d::collider_tree::ColliderTreeDiagnostics;
use avian3d::collision::CollisionDiagnostics;
use avian3d::dynamics::solver::SolverDiagnostics;
use avian3d::prelude::*;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::RunSystemOnce;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::{input::InputPlugin, scene::ScenePlugin};
use brahe::utils::DOrbitStateProvider;
use brahe::{AngleFormat, Epoch, KeplerianPropagator, TimeSystem};
use nalgebra::Vector6;

use crate::SimPlugins;
use crate::SimStep;
use crate::capture::plan::{CapturePlan, CapturePlanLibrary, build_capture_component};
use crate::capture::{CaptureSphereRadius, capture_state_machine_update};
use crate::orbit::cache::OrbitalCache;
use crate::project::{AssetsDir, load_plans_from_dir};
use crate::session::{SimState, UiScreen};
use crate::telemetry::{CaptureLog, LogEvent, LogLevel, collect_log_events};
use crate::time::{ManualPhysics, PHYS_DT, SimConfig, WorldTime};

/// Deterministic sim epoch for tests; never read the wall clock in tests (F-06).
///
/// This is the dataset's GP epoch.
pub fn fixed_epoch() -> Epoch {
    Epoch::from_datetime(2026, 4, 3, 16, 3, 47.002464, 0.0, TimeSystem::UTC)
}

/// Workspace root (this crate lives in `crates/ssg-app`).
pub fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Minimal headless app wired like the real one, with a fixed sim epoch.
///
/// Time: `app.update()` advances virtual time by exactly one fixed step
/// (`PHYS_DT`) and so runs `FixedUpdate` (hence `fixed_physics_step`) exactly
/// once, with the generic `Time` being `Time<Fixed>`, as in the app (D8).
/// Avian takes its timestep from the generic `Time` resource, not from
/// `PHYS_DT`, and skips the step when that delta is zero. Calling
/// `fixed_physics_step` from test code (outside `FixedUpdate`) would therefore
/// step avian with the last frame's wall-clock delta, or not at all.
pub fn test_app() -> App {
    // P3: `SimPlugins` replace `create_app()` and provide what the removed lines
    // added (`DataCollection`, `SelectedProject`, the body spawn system); the
    // render-side asset registrations are app-side now.
    let mut app = App::new(); // P3: was `create_app()`
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin, // P3: `SimPlugins` need it first
        AssetPlugin::default(),
        InputPlugin,
        ScenePlugin,
    ))
    .init_asset::<Mesh>()
    .add_plugins(SimPlugins) // P3: the sim plugins
    .init_resource::<CollisionDiagnostics>()
    .init_resource::<SpatialQueryDiagnostics>()
    .init_resource::<SolverDiagnostics>()
    .init_resource::<ColliderTreeDiagnostics>()
    // The sim start epoch (F-06: fixed in P4).
    .insert_resource(SimConfig {
        start_epoch: fixed_epoch(),
    })
    .insert_resource(AssetsDir(repo_root().join("assets"))) // P3: the dataset directory
    // One `app.update()` = one `Time<Fixed>` step (see above).
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        PHYS_DT,
    )));
    app
}

/// Switch to the sim screen; `setup_time` starts the clock at the app's
/// [`SimConfig`] epoch ([`fixed_epoch`] in [`test_app`]).
pub fn enter_sim(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(UiScreen::Sim);
    app.update();
}

/// Set `SimState::Running` so `FixedUpdate` starts running `fixed_physics_step`.
pub fn start_running(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<SimState>>()
        .set(SimState::Running);
    app.update();
}

/// One `app.update()`: exactly one fixed step (times the warp multiplier).
pub fn tick(app: &mut App) {
    app.update();
}

/// Turns the relative gravity (`SimStep::RelativeDynamics`, F-09) off in `app`.
/// For the floating-origin and camera tests that pin positions to 1e-9 m with
/// bodies 50-60 m from the anchor, where one step of differential gravity moves
/// them by about 1.5e-8 m (operator decision, P4 Run 3).
pub fn disable_relative_gravity(app: &mut App) {
    app.configure_sets(ManualPhysics, SimStep::RelativeDynamics.run_if(|| false));
}

/// `n` calls to [`tick`].
pub fn ticks(app: &mut App, n: u64) {
    for _ in 0..n {
        tick(app);
    }
}

/// ECI state for Keplerian `elements` at `epoch`: the same two brahe calls
/// `init_orbitals` makes.
pub fn kepler_eci(elements: Vector6<f64>, epoch: Epoch) -> Vector6<f64> {
    KeplerianPropagator::from_keplerian(epoch, elements, AngleFormat::Radians, 1.0)
        .state_eci(epoch)
        .expect("Keplerian propagation failed")
}

/// A propagator starting from `eci` at [`fixed_epoch`].
pub fn fixed_propagator(eci: Vector6<f64>) -> KeplerianPropagator {
    KeplerianPropagator::from_eci(fixed_epoch(), eci, 1.0)
}

/// Assert `|actual - expected| <= tol`; the message names `what` and both values.
#[track_caller]
pub fn assert_close(actual: f64, expected: f64, tol: f64, what: &str) {
    let diff = (actual - expected).abs();
    assert!(
        diff <= tol,
        "{what}: actual {actual:e} vs expected {expected:e} (|diff| {diff:e} > tol {tol:e})"
    );
}

/// Component-wise [`assert_close`] for [`DVec3`].
#[track_caller]
pub fn assert_close_dvec3(actual: DVec3, expected: DVec3, tol: f64, what: &str) {
    for (axis, (a, e)) in ["x", "y", "z"]
        .iter()
        .zip(actual.to_array().into_iter().zip(expected.to_array()))
    {
        assert_close(a, e, tol, &format!("{what}.{axis}"));
    }
}

/// Component-wise [`assert_close`] for [`Vector6`].
#[track_caller]
pub fn assert_close_vec6(actual: &Vector6<f64>, expected: &Vector6<f64>, tol: f64, what: &str) {
    for i in 0..6 {
        assert_close(actual[i], expected[i], tol, &format!("{what}[{i}]"));
    }
}

/// Reads [`LogEvent`] messages from a cursor.
///
/// `app.update()` swaps message buffers each frame, so drain after **every**
/// tick.
pub struct LogTap {
    cursor: MessageCursor<LogEvent>,
}

impl LogTap {
    /// A tap that sees messages written from now on.
    pub fn new(world: &World) -> Self {
        Self {
            cursor: world.resource::<Messages<LogEvent>>().get_cursor_current(),
        }
    }

    pub fn drain(&mut self, world: &World) -> Vec<(LogLevel, &'static str, String)> {
        let messages = world.resource::<Messages<LogEvent>>();
        self.cursor
            .read(messages)
            .map(|event| (event.level, event.source, event.message.clone()))
            .collect()
    }
}

/// Build a minimal app wired up to test `collect_log_events`.
pub fn log_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<LogEvent>()
        .init_resource::<CaptureLog>()
        .insert_resource(WorldTime::starting_at(fixed_epoch()))
        .add_systems(Update, collect_log_events);
    app
}

/// Load every plan in `assets/example_capture_plans` into the app's library.
pub fn load_example_plans(app: &mut App) {
    let example_plans = load_plans_from_dir(&repo_root().join("assets/example_capture_plans"));
    let mut capture_plan_lib = app.world_mut().resource_mut::<CapturePlanLibrary>();
    capture_plan_lib.plans = example_plans;

    // Compile capture plans
    for (name, plan) in capture_plan_lib.plans.clone() {
        capture_plan_lib.insert_plan(name.clone(), plan.clone());
    }
}

/// Parse `json` as a [`CapturePlan`] and insert it under `id` **without**
/// validation: some characterization plans are invalid on purpose.
pub fn insert_plan_json(app: &mut App, id: &str, json: &str) {
    let plan: CapturePlan = serde_json::from_str(json).expect("plan JSON did not parse");
    app.world_mut()
        .resource_mut::<CapturePlanLibrary>()
        .insert_plan(id.to_string(), plan);
}

/// Three rigid bodies driving `capture_state_machine_update` with scripted
/// ranges and speeds (no physics advance between script rows).
pub struct CaptureRig {
    pub capture: Entity,
    pub root: Entity,
    pub tail: Entity,
}

/// Spawn the capture entity (radius 1.0) and a two-node tether (radius 0.5,
/// mass 2.0), register the tether under the plan's name and attach the capture
/// component for plan `plan_id`. Runs one `app.update()` so avian initialises the
/// rigid bodies.
pub fn capture_rig(app: &mut App, plan_id: &str) -> CaptureRig {
    let plan = app
        .world()
        .resource::<CapturePlanLibrary>()
        .plans
        .get(plan_id)
        .unwrap_or_else(|| panic!("plan '{plan_id}' is not in the library"))
        .clone();

    if app.world().get_resource::<OrbitalCache>().is_none() {
        app.world_mut().insert_resource(OrbitalCache::default());
    }
    if app.world().get_resource::<WorldTime>().is_none() {
        // The clock `init_sim_resources` inserts; the rig never advances it.
        app.world_mut()
            .insert_resource(WorldTime::starting_at(fixed_epoch()));
    }
    if app.world().get_resource::<CaptureSphereRadius>().is_none() {
        // The value `init_sim_resources` inserts.
        app.world_mut()
            .insert_resource(CaptureSphereRadius { radius: 25.0 });
    }

    let capture = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(1.0),
            Position::default(),
        ))
        .id();
    let root = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Mass::from(2.0),
            Position::default(),
        ))
        .id();
    let tail = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Mass::from(2.0),
            Position::default(),
        ))
        .id();

    app.world_mut()
        .resource_mut::<OrbitalCache>()
        .tethers
        .insert(plan.tether.clone(), vec![root, tail]);

    let component = build_capture_component(plan_id, &plan, 0.0)
        .unwrap_or_else(|| panic!("plan '{plan_id}' has no states"));
    app.world_mut().entity_mut(capture).insert(component);

    app.update();

    CaptureRig {
        capture,
        root,
        tail,
    }
}

/// Place the rig so the capture entity is `range_m` from both tether nodes and
/// `rel_speed_mps` faster relative to them: capture at `(range, 0, 0)` at rest,
/// root and tail at the origin moving `(rel_speed, 0, 0)`. Then `|rel_r| = range`
/// and `|rel_v| = rel_speed` exactly as `capture_state_machine_update` computes
/// them.
pub fn rig_set(app: &mut App, rig: &CaptureRig, range_m: f64, rel_speed_mps: f64) {
    let world = app.world_mut();
    world
        .entity_mut(rig.capture)
        .insert((Position::from_xyz(range_m, 0.0, 0.0), LinearVelocity::ZERO));
    for node in [rig.root, rig.tail] {
        world.entity_mut(node).insert((
            Position::from_xyz(0.0, 0.0, 0.0),
            LinearVelocity(DVec3::new(rel_speed_mps, 0.0, 0.0)),
        ));
    }
}

/// Run `capture_state_machine_update` once and return the `(from, to, reason)`
/// triples it logged, in order. Does **not** call `app.update()`: that would let
/// avian move the bodies and the scripted range would be lost.
pub fn rig_step(app: &mut App, _rig: &CaptureRig) -> Vec<(String, String, String)> {
    let mut tap = LogTap::new(app.world());
    app.world_mut()
        .run_system_once(capture_state_machine_update)
        .expect("capture_state_machine_update failed to run");
    tap.drain(app.world())
        .into_iter()
        .filter(|(_, source, _)| *source == "capture")
        .filter_map(|(_, _, message)| parse_transition(&message))
        .collect()
}

/// Parse `State: {from} → {to} ({reason})`.
fn parse_transition(message: &str) -> Option<(String, String, String)> {
    let rest = message.strip_prefix("State: ")?;
    let (from, rest) = rest.split_once(" → ")?;
    let (to, reason) = rest.split_once(" (")?;
    let reason = reason.strip_suffix(')')?;
    Some((from.to_string(), to.to_string(), reason.to_string()))
}

/// Q13 Hubble scenario builder: the HST (NORAD 20580) as the capture target and
/// the P0 chaser (NORAD 99901) as the tether root, spawned through the real
/// `spawn_selected_bodies` with the `example_plan` capture plan.
///
/// Headless only (A-6): no glTF loader, so the HST has no collider, and no
/// render, camera or GPU systems run. See plans/stage-2/52-p2-testing-baseline.md.
pub mod q13 {
    use std::time::{Duration, Instant};

    use avian3d::prelude::*;
    use bevy::ecs::message::Messages;
    use bevy::math::DVec3;
    use bevy::prelude::*;

    use super::{enter_sim, fixed_epoch, load_example_plans, start_running, test_app, tick};
    use crate::capture::plan::{CapturePlanLibrary, build_capture_component};
    use crate::capture::{CaptureComponent, CaptureTransition};
    use crate::orbit::cache::OrbitalCache;
    use crate::orbit::catalog::{JsonOrbital, JsonOrbitalData, gp_record_to_elements};
    use crate::orbit::selection::{
        EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState,
        SelectedOrbitalObject,
    };
    use crate::orbit::{Orbit, Orbital};
    use crate::project::SelectedProject;
    use crate::time::{PHYS_DT, WorldTime};

    /// The HST record (verbatim from `assets/datasets/dataset_2026_04_03.json`)
    /// and the P0 chaser record, as a GP JSON array.
    pub const FIXTURE: &str = include_str!("fixtures/q13_hubble_pair.json");
    pub const HST_NORAD_ID: u64 = 20580;
    pub const CHASER_NORAD_ID: u64 = 99901;
    pub const PLAN_ID: &str = "example_plan";
    /// [`run`] records a checkpoint every this many ticks.
    pub const CHECKPOINT_EVERY: u64 = 640;

    /// The fixture record with `NORAD_CAT_ID == norad_id`.
    pub fn fixture_record(norad_id: u64) -> JsonOrbital {
        let data: JsonOrbitalData =
            serde_json::from_str(FIXTURE).expect("Q13 fixture did not parse");
        data.data
            .into_iter()
            .find(|record| {
                record.norad_cat_id.as_ref().and_then(|id| id.as_u64()) == Some(norad_id)
            })
            .unwrap_or_else(|| panic!("NORAD {norad_id} is not in the Q13 fixture"))
    }

    pub struct Q13Handles {
        /// The HST: `"Satellite1"`, carries the `CaptureComponent`.
        pub debris: Entity,
        /// The chaser: the tether root, `SimAnchor`.
        pub root: Entity,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct Q13Sample {
        /// `round((WorldTime.epoch − WorldTime.start_epoch) / PHYS_DT)`.
        pub step: u64,
        pub state: String,
        /// `|Position(debris) − Position(root)|`, the range the transitions use.
        pub range_m: f64,
    }

    pub struct Q13Run {
        /// `(step, from, to)` per [`CaptureTransition`], `step` being the fixed step
        /// it fired in.
        pub transitions: Vec<(u64, String, String)>,
        /// One sample every [`CHECKPOINT_EVERY`] ticks.
        pub checkpoints: Vec<Q13Sample>,
        pub final_sample: Q13Sample,
        pub final_root: DVec3,
        pub final_debris: DVec3,
        /// Ticks run after [`build`].
        pub ticks: u64,
        /// Wall time of the whole run, including [`build`].
        pub wall: Duration,
    }

    /// The selection the app makes when the user picks `norad_id` from the catalog
    /// (F-27: fixed in P4): the record's full-precision elements at its own epoch,
    /// and the record itself, so the body starts from SGP4.
    pub fn catalog_pick(label: &str, norad_id: u64) -> SelectedOrbitalObject {
        let (_, gp, _) = gp_record_to_elements(&fixture_record(norad_id), fixed_epoch())
            .unwrap_or_else(|| panic!("NORAD {norad_id}: the fixture record is incomplete"));
        SelectedOrbitalObject {
            source: OrbitalSelectionSource::Custom {
                label: label.to_string(),
            },
            elements: EditableOrbitalElements::from_gp(&gp),
            gp: Some(gp),
        }
    }

    /// Select the HST and the chaser, enter the sim, start it, and mark the HST
    /// for capture with `example_plan`, as the app's UI does. The start epoch is
    /// the app's [`SimConfig`](crate::time::SimConfig) ([`fixed_epoch`] in [`test_app`]); the wall-clock
    /// epoch mode was retired in P4 S6 (the sim has no wall-clock path, Q41).
    pub fn build(app: &mut App) -> Q13Handles {
        load_example_plans(app);
        app.world_mut().resource_mut::<SelectedProject>().project_id = Some(PLAN_ID.to_string());
        {
            let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
            selection.target = Some(catalog_pick("HST", HST_NORAD_ID));
            selection.chaser = Some(catalog_pick("P0 HST CHASER", CHASER_NORAD_ID));
        }

        enter_sim(app);
        start_running(app);

        // `spawn_selected_bodies` has spawned the tether root and the debris with
        // `Orbit::FromGp`; `init_orbitals` turns them into propagators at the start
        // epoch (SGP4 state there) on the next update.
        let pending = app.world_mut().query::<&Orbit>().iter(app.world()).count();
        assert_eq!(
            pending, 2,
            "precondition: root and debris await init_orbitals"
        );
        tick(app);

        let plan = app
            .world()
            .resource::<CapturePlanLibrary>()
            .plans
            .get(PLAN_ID)
            .expect("example_plan is not loaded")
            .clone();
        let cache = app.world().resource::<OrbitalCache>();
        let handles = Q13Handles {
            debris: *cache.debris.get("Satellite1").expect("debris not spawned"),
            root: cache.tethers.get(&plan.tether).expect("tether not spawned")[0],
        };
        for entity in [handles.debris, handles.root] {
            assert!(
                app.world().get::<Orbit>(entity).is_none()
                    && app
                        .world()
                        .get::<Orbital>(entity)
                        .is_some_and(|o| o.propagator.is_some()),
                "precondition: {entity:?} has a propagator"
            );
        }

        // As `apply_sim_commands` does on "Capture": sim seconds since the start.
        let start_time = {
            let world_time = app.world().resource::<WorldTime>();
            world_time.epoch - world_time.start_epoch
        };
        let component =
            build_capture_component(PLAN_ID, &plan, start_time).expect("example_plan has states");
        app.world_mut().entity_mut(handles.debris).insert(component);
        tick(app);
        handles
    }

    pub fn sample(app: &App, handles: &Q13Handles) -> Q13Sample {
        let world = app.world();
        let world_time = world.resource::<WorldTime>();
        let step = ((world_time.epoch - world_time.start_epoch) / PHYS_DT).round() as u64;
        let state = world
            .get::<CaptureComponent>(handles.debris)
            .expect("debris has no CaptureComponent")
            .current_state
            .clone();
        let debris = world
            .get::<Position>(handles.debris)
            .expect("debris Position")
            .0;
        let root = world
            .get::<Position>(handles.root)
            .expect("root Position")
            .0;
        Q13Sample {
            step,
            state,
            range_m: (debris - root).length(),
        }
    }

    /// Build the scenario in a fresh harness app and tick it up to `max_ticks`
    /// times (stopping once the state is `capture` if `stop_at_capture`).
    pub fn run(max_ticks: u64, stop_at_capture: bool) -> Q13Run {
        let started = Instant::now();
        let mut app = test_app();
        let handles = build(&mut app);
        assert_eq!(
            sample(&app, &handles).state,
            "rendezvous",
            "precondition: no transition before the run starts"
        );

        let mut cursor = app
            .world()
            .resource::<Messages<CaptureTransition>>()
            .get_cursor_current();
        let mut transitions = Vec::new();
        let mut checkpoints = Vec::new();
        let mut ticks = 0;
        let mut final_sample = sample(&app, &handles);
        while ticks < max_ticks {
            tick(&mut app);
            ticks += 1;
            final_sample = sample(&app, &handles);
            let start_epoch = app.world().resource::<WorldTime>().start_epoch;
            for transition in cursor.read(app.world().resource::<Messages<CaptureTransition>>()) {
                let step = ((transition.epoch - start_epoch) / PHYS_DT).round() as u64;
                transitions.push((step, transition.from.clone(), transition.to.clone()));
            }
            if ticks % CHECKPOINT_EVERY == 0 {
                checkpoints.push(final_sample.clone());
            }
            if stop_at_capture && final_sample.state == "capture" {
                break;
            }
        }

        let world = app.world();
        Q13Run {
            transitions,
            checkpoints,
            final_sample,
            final_root: world
                .get::<Position>(handles.root)
                .expect("root Position")
                .0,
            final_debris: world
                .get::<Position>(handles.debris)
                .expect("debris Position")
                .0,
            ticks,
            wall: started.elapsed(),
        }
    }
}

#[cfg(test)]
mod self_tests {
    use super::*;

    #[test]
    fn harness_tick_is_one_fixed_step() {
        let mut app = test_app();
        enter_sim(&mut app);
        start_running(&mut app);

        let before = app.world().resource::<WorldTime>().epoch;
        tick(&mut app);
        let after = app.world().resource::<WorldTime>().epoch;
        assert_close(after - before, PHYS_DT, 1e-12, "one tick at x1");
        assert_close(
            app.world()
                .resource::<Time<Physics>>()
                .delta()
                .as_secs_f64(),
            PHYS_DT,
            1e-12,
            "Time<Physics> delta after one tick",
        );

        app.world_mut().resource_mut::<WorldTime>().multiplier = 8;
        let before = app.world().resource::<WorldTime>().epoch;
        tick(&mut app);
        let after = app.world().resource::<WorldTime>().epoch;
        assert_close(after - before, 8.0 * PHYS_DT, 1e-12, "one tick at x8");
    }

    #[test]
    fn harness_epoch_is_fixed() {
        let mut app = test_app();
        assert_eq!(
            app.world().resource::<SimConfig>().start_epoch,
            fixed_epoch()
        );

        enter_sim(&mut app);
        let world_time = app.world().resource::<WorldTime>();
        assert_eq!(world_time.epoch, fixed_epoch());
        assert_eq!(world_time.start_epoch, fixed_epoch());
    }
}
