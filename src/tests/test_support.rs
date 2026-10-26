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
use bevy::time::TimeUpdateStrategy;
use bevy::{input::InputPlugin, scene::ScenePlugin};
use brahe::utils::DOrbitStateProvider;
use brahe::{AngleFormat, Epoch, KeplerianPropagator, TimeSystem};
use nalgebra::Vector6;

use crate::components::capture_components::CapturePlan;
use crate::create_app;
use crate::plugins::gpu_compute::GpuElements;
use crate::plugins::orbital_mechanics::SimState;
use crate::resources::capture_log::{CaptureLog, CaptureLogUiState, LogEvent, LogLevel};
use crate::resources::capture_plans::{
    CapturePlanLibrary, CaptureSphereRadius, build_capture_component, load_plans_from_dir,
};
use crate::resources::data_collection::DataCollection;
use crate::resources::orbital_cache::OrbitalCache;
use crate::resources::world_time::WorldTime;
use crate::systems::capture_algorithms::capture_state_machine_update;
use crate::systems::physics::PHYS_DT;
use crate::systems::setup::setup_orbital_selection;
use crate::ui::plugin::collect_log_events;
use crate::ui::state::{SelectedProject, UiScreen};

/// Deterministic sim epoch for tests; never use `Epoch::now()` in tests (F-06).
///
/// This is the dataset's GP epoch.
pub(crate) fn fixed_epoch() -> Epoch {
    Epoch::from_datetime(2026, 4, 3, 16, 3, 47.002464, 0.0, TimeSystem::UTC)
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
pub(crate) fn test_app() -> App {
    let mut app = create_app();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        InputPlugin,
        ScenePlugin,
    ))
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .init_asset::<GizmoAsset>()
    .init_resource::<CollisionDiagnostics>()
    .init_resource::<SpatialQueryDiagnostics>()
    .init_resource::<SolverDiagnostics>()
    .init_resource::<ColliderTreeDiagnostics>()
    .init_resource::<DataCollection>()
    // `WorldTime::default()` reads the wall clock (F-06), so insert it explicitly.
    .insert_resource(WorldTime {
        multiplier: 1,
        epoch: fixed_epoch(),
        start_epoch: fixed_epoch(),
    })
    // Normally provided by `UiPlugin`; `spawn_tether` needs it.
    .init_resource::<SelectedProject>()
    // Normally provided by `GpuComputePlugin`; `orbital_gizmos` needs it as soon as a
    // `Camera3d` exists.
    .init_resource::<GpuElements>()
    // Mirrors `run()` at `src/lib.rs`.
    .add_systems(OnEnter(SimState::Running), setup_orbital_selection)
    // One `app.update()` = one `Time<Fixed>` step (see above).
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        PHYS_DT,
    )));
    app
}

/// Switch to the sim screen and pin the sim epoch to [`fixed_epoch`].
pub(crate) fn enter_sim(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(UiScreen::Sim);
    app.update();

    // `setup_time` has just set both epochs from the wall clock; overwrite them.
    // F-06: current behaviour, expected to change in P4
    let mut world_time = app.world_mut().resource_mut::<WorldTime>();
    world_time.epoch = fixed_epoch();
    world_time.start_epoch = fixed_epoch();
}

/// Set `SimState::Running` so `FixedUpdate` starts running `fixed_physics_step`.
pub(crate) fn start_running(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<SimState>>()
        .set(SimState::Running);
    app.update();
}

/// One `app.update()`: exactly one fixed step (times the warp multiplier).
pub(crate) fn tick(app: &mut App) {
    app.update();
}

/// `n` calls to [`tick`].
pub(crate) fn ticks(app: &mut App, n: u64) {
    for _ in 0..n {
        tick(app);
    }
}

/// ECI state for Keplerian `elements` at `epoch`: the same two brahe calls
/// `init_orbitals` makes.
pub(crate) fn kepler_eci(elements: Vector6<f64>, epoch: Epoch) -> Vector6<f64> {
    KeplerianPropagator::from_keplerian(epoch, elements, AngleFormat::Radians, 1.0)
        .state_eci(epoch)
        .expect("Keplerian propagation failed")
}

/// A propagator starting from `eci` at [`fixed_epoch`].
pub(crate) fn fixed_propagator(eci: Vector6<f64>) -> KeplerianPropagator {
    KeplerianPropagator::from_eci(fixed_epoch(), eci, 1.0)
}

/// Assert `|actual - expected| <= tol`; the message names `what` and both values.
#[track_caller]
pub(crate) fn assert_close(actual: f64, expected: f64, tol: f64, what: &str) {
    let diff = (actual - expected).abs();
    assert!(
        diff <= tol,
        "{what}: actual {actual:e} vs expected {expected:e} (|diff| {diff:e} > tol {tol:e})"
    );
}

/// Component-wise [`assert_close`] for [`DVec3`].
#[track_caller]
pub(crate) fn assert_close_dvec3(actual: DVec3, expected: DVec3, tol: f64, what: &str) {
    for (axis, (a, e)) in ["x", "y", "z"]
        .iter()
        .zip(actual.to_array().into_iter().zip(expected.to_array()))
    {
        assert_close(a, e, tol, &format!("{what}.{axis}"));
    }
}

/// Component-wise [`assert_close`] for [`Vector6`].
#[track_caller]
pub(crate) fn assert_close_vec6(
    actual: &Vector6<f64>,
    expected: &Vector6<f64>,
    tol: f64,
    what: &str,
) {
    for i in 0..6 {
        assert_close(actual[i], expected[i], tol, &format!("{what}[{i}]"));
    }
}

/// Reads [`LogEvent`] messages from a cursor.
///
/// `app.update()` swaps message buffers each frame, so drain after **every**
/// tick.
pub(crate) struct LogTap {
    cursor: MessageCursor<LogEvent>,
}

impl LogTap {
    /// A tap that sees messages written from now on.
    pub(crate) fn new(world: &World) -> Self {
        Self {
            cursor: world.resource::<Messages<LogEvent>>().get_cursor_current(),
        }
    }

    pub(crate) fn drain(&mut self, world: &World) -> Vec<(LogLevel, &'static str, String)> {
        let messages = world.resource::<Messages<LogEvent>>();
        self.cursor
            .read(messages)
            .map(|event| (event.level, event.source, event.message.clone()))
            .collect()
    }
}

/// Build a minimal app wired up to test `collect_log_events`.
pub(crate) fn log_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<LogEvent>()
        .init_resource::<CaptureLog>()
        .init_resource::<CaptureLogUiState>()
        .init_resource::<WorldTime>()
        .add_systems(Update, collect_log_events);
    app
}

/// Load every plan in `assets/example_capture_plans` into the app's library.
pub(crate) fn load_example_plans(app: &mut App) {
    let example_plans = load_plans_from_dir(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/example_capture_plans"),
    );
    let mut capture_plan_lib = app.world_mut().resource_mut::<CapturePlanLibrary>();
    capture_plan_lib.plans = example_plans;

    // Compile capture plans
    for (name, plan) in capture_plan_lib.plans.clone() {
        capture_plan_lib.insert_plan(name.clone(), plan.clone());
    }
}

/// Parse `json` as a [`CapturePlan`] and insert it under `id` **without**
/// validation: some characterization plans are invalid on purpose.
pub(crate) fn insert_plan_json(app: &mut App, id: &str, json: &str) {
    let plan: CapturePlan = serde_json::from_str(json).expect("plan JSON did not parse");
    app.world_mut()
        .resource_mut::<CapturePlanLibrary>()
        .insert_plan(id.to_string(), plan);
}

/// Three rigid bodies driving `capture_state_machine_update` with scripted
/// ranges and speeds (no physics advance between script rows).
pub(crate) struct CaptureRig {
    pub(crate) capture: Entity,
    pub(crate) root: Entity,
    pub(crate) tail: Entity,
}

/// Spawn the capture entity (radius 1.0) and a two-node tether (radius 0.5,
/// mass 2.0), register the tether under the plan's name and attach the capture
/// component for plan `plan_id`. Runs one `app.update()` so avian initialises the
/// rigid bodies.
pub(crate) fn capture_rig(app: &mut App, plan_id: &str) -> CaptureRig {
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
pub(crate) fn rig_set(app: &mut App, rig: &CaptureRig, range_m: f64, rel_speed_mps: f64) {
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
pub(crate) fn rig_step(app: &mut App, _rig: &CaptureRig) -> Vec<(String, String, String)> {
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

/// Q13 Hubble scenario builder: filled in by S4.
pub(crate) mod q13 {}

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
        {
            let world_time = app.world().resource::<WorldTime>();
            assert_eq!(world_time.epoch, fixed_epoch());
            assert_eq!(world_time.start_epoch, fixed_epoch());
        }

        enter_sim(&mut app);
        let world_time = app.world().resource::<WorldTime>();
        assert_eq!(world_time.epoch, fixed_epoch());
        assert_eq!(world_time.start_epoch, fixed_epoch());
    }
}
