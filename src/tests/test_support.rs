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

/// Q13 Hubble scenario builder: the HST (NORAD 20580) as the capture target and
/// the P0 chaser (NORAD 99901) as the tether root, spawned through the real
/// `setup_orbital_selection` with the `example_plan` capture plan.
///
/// Headless only (A-6): no glTF loader, so the HST has no collider, and no
/// render, camera or GPU systems run. See plans/stage-2/52-p2-testing-baseline.md.
pub(crate) mod q13 {
    use std::f64::consts::PI;
    use std::time::{Duration, Instant};

    use avian3d::prelude::*;
    use bevy::math::DVec3;
    use bevy::prelude::*;
    use brahe::{Epoch, GM_EARTH, TimeSystem};
    use nalgebra::Vector6;

    use super::{
        LogTap, enter_sim, fixed_epoch, fixed_propagator, kepler_eci, load_example_plans,
        parse_transition, start_running, test_app, tick,
    };
    use crate::components::capture_components::CaptureComponent;
    use crate::components::orbit::{JsonOrbital, JsonOrbitalData, Orbit, Orbital};
    use crate::plugins::gpu_compute::GpuOrbitalElements;
    use crate::resources::capture_plans::{CapturePlanLibrary, build_capture_component};
    use crate::resources::orbital_cache::OrbitalCache;
    use crate::resources::space_catalog::{
        EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState,
        SelectedOrbitalObject,
    };
    use crate::resources::world_time::WorldTime;
    use crate::systems::physics::PHYS_DT;
    use crate::ui::state::{SelectedProject, UiScreen};

    /// The HST record (verbatim from `assets/datasets/dataset_2026_04_03.json`)
    /// and the P0 chaser record, as a GP JSON array.
    pub(crate) const FIXTURE: &str = include_str!("fixtures/q13_hubble_pair.json");
    pub(crate) const HST_NORAD_ID: u64 = 20580;
    pub(crate) const CHASER_NORAD_ID: u64 = 99901;
    pub(crate) const PLAN_ID: &str = "example_plan";
    /// [`run`] records a checkpoint every this many ticks.
    pub(crate) const CHECKPOINT_EVERY: u64 = 640;

    /// The fixture record with `NORAD_CAT_ID == norad_id`.
    pub(crate) fn fixture_record(norad_id: u64) -> JsonOrbital {
        let data: JsonOrbitalData =
            serde_json::from_str(FIXTURE).expect("Q13 fixture did not parse");
        data.data
            .into_iter()
            .find(|record| {
                record.norad_cat_id.as_ref().and_then(|id| id.as_u64()) == Some(norad_id)
            })
            .unwrap_or_else(|| panic!("NORAD {norad_id} is not in the Q13 fixture"))
    }

    /// The elements the app stores in `OrbitalSelectionState` when the user picks
    /// `record` from the catalog: `load_dataset_entities` (mean motion → semi-major
    /// axis, degrees → radians, then the f32 `GpuOrbitalElements` buffer), then
    /// back to f64 as `project_detail.rs` does ([`gpu_to_editable`]).
    ///
    /// The epoch offset is taken against [`fixed_epoch`], the dataset reference
    /// epoch in a harness app; the app uses the wall clock there (F-06).
    pub(crate) fn app_elements(record: &JsonOrbital) -> EditableOrbitalElements {
        let value = |field: &Option<serde_json::Value>, name: &str| {
            field
                .as_ref()
                .and_then(|value| value.as_f64())
                .unwrap_or_else(|| panic!("record has no numeric {name}"))
        };
        let mean_motion = value(&record.mean_motion, "MEAN_MOTION");
        let mean_motion_rad_s = mean_motion * 2.0 * PI / 86400.0;
        let semi_major_axis = (GM_EARTH / (mean_motion_rad_s * mean_motion_rad_s)).powf(1.0 / 3.0);
        let object_epoch =
            parse_dataset_epoch(record.epoch.as_deref().expect("record has no EPOCH"));

        let gpu = GpuOrbitalElements {
            id: value(&record.norad_cat_id, "NORAD_CAT_ID") as u32,
            a: semi_major_axis as f32,
            e: value(&record.eccentricity, "ECCENTRICITY") as f32,
            i: value(&record.inclination, "INCLINATION").to_radians() as f32,
            raan: value(&record.ra_of_asc_node, "RA_OF_ASC_NODE").to_radians() as f32,
            argp: value(&record.arg_of_pericenter, "ARG_OF_PERICENTER").to_radians() as f32,
            mean_anomaly: value(&record.mean_anomaly, "MEAN_ANOMALY").to_radians() as f32,
            epoch_offset_seconds: (object_epoch - fixed_epoch()) as f32,
        };
        gpu_to_editable(&gpu)
    }

    /// `project_detail.rs`'s f32 → f64 conversion of a catalog entry.
    /// `epoch_offset_seconds` is kept here but `to_vec6()` drops it.
    // F-27: current behaviour, expected to change in P4
    pub(crate) fn gpu_to_editable(elements: &GpuOrbitalElements) -> EditableOrbitalElements {
        EditableOrbitalElements {
            semi_major_axis_m: elements.a as f64,
            eccentricity: elements.e as f64,
            inclination_rad: elements.i as f64,
            raan_rad: elements.raan as f64,
            arg_perigee_rad: elements.argp as f64,
            mean_anomaly_rad: elements.mean_anomaly as f64,
            epoch_offset_seconds: elements.epoch_offset_seconds as f64,
        }
    }

    /// The fixed-format branch of `parse_dataset_epoch_fast` (`propagation.rs`),
    /// the format every fixture record uses.
    fn parse_dataset_epoch(raw: &str) -> Epoch {
        let field = |range: std::ops::Range<usize>| -> u32 {
            raw.get(range)
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| panic!("bad EPOCH {raw}"))
        };
        assert!(
            raw.len() >= 19 && &raw[4..5] == "-" && &raw[10..11] == "T",
            "EPOCH {raw} is not in the fixed format"
        );
        let whole_seconds = field(17..19) as f64;
        let digits = raw.get(20..).unwrap_or_default();
        let fractional_seconds = if raw.get(19..20) == Some(".") && !digits.is_empty() {
            digits.parse::<u32>().expect("bad EPOCH fraction") as f64
                / 10_f64.powi(digits.len() as i32)
        } else {
            0.0
        };
        Epoch::from_datetime(
            field(0..4),
            field(5..7) as u8,
            field(8..10) as u8,
            field(11..13) as u8,
            field(14..16) as u8,
            whole_seconds + fractional_seconds,
            0.0,
            TimeSystem::UTC,
        )
    }

    /// How the two spacecraft get their initial epoch.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum EpochMode {
        /// As in the app: `setup_time` and `init_orbitals` read the wall clock (F-06).
        AppPath,
        /// Both propagators start at [`fixed_epoch`], and so does `WorldTime`.
        Fixed,
    }

    impl EpochMode {
        pub(crate) fn name(self) -> &'static str {
            match self {
                Self::AppPath => "app_path",
                Self::Fixed => "fixed",
            }
        }
    }

    pub(crate) struct Q13Handles {
        /// The HST: `"Satellite1"`, carries the `CaptureComponent`.
        pub(crate) debris: Entity,
        /// The chaser: the tether root, `CameraTarget`.
        pub(crate) root: Entity,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub(crate) struct Q13Sample {
        /// `round((WorldTime.epoch − WorldTime.start_epoch) / PHYS_DT)`.
        pub(crate) step: u64,
        pub(crate) state: String,
        /// `|Position(debris) − Position(root)|`, the range the transitions use.
        pub(crate) range_m: f64,
    }

    pub(crate) struct Q13Run {
        /// `(step, from, to)` per logged transition; a cascade shares one step.
        pub(crate) transitions: Vec<(u64, String, String)>,
        /// One sample every [`CHECKPOINT_EVERY`] ticks.
        pub(crate) checkpoints: Vec<Q13Sample>,
        pub(crate) final_sample: Q13Sample,
        pub(crate) final_root: DVec3,
        pub(crate) final_debris: DVec3,
        /// Ticks run after [`build`].
        pub(crate) ticks: u64,
        /// Wall time of the whole run, including [`build`].
        pub(crate) wall: Duration,
    }

    fn catalog_pick(label: &str, norad_id: u64) -> SelectedOrbitalObject {
        SelectedOrbitalObject {
            source: OrbitalSelectionSource::Custom {
                label: label.to_string(),
            },
            elements: app_elements(&fixture_record(norad_id)),
        }
    }

    /// Select the HST and the chaser, enter the sim, start it, and mark the HST
    /// for capture with `example_plan`, as the app's UI does.
    pub(crate) fn build(app: &mut App, epoch_mode: EpochMode) -> Q13Handles {
        load_example_plans(app);
        app.world_mut().resource_mut::<SelectedProject>().project_id = Some(PLAN_ID.to_string());
        {
            let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
            selection.target = Some(catalog_pick("HST", HST_NORAD_ID));
            selection.chaser = Some(catalog_pick("P0 HST CHASER", CHASER_NORAD_ID));
        }

        match epoch_mode {
            EpochMode::Fixed => enter_sim(app),
            EpochMode::AppPath => {
                // Keep `setup_time`'s wall-clock epoch, as in the app.
                // F-06: current behaviour, expected to change in P4
                app.world_mut()
                    .resource_mut::<NextState<UiScreen>>()
                    .set(UiScreen::Sim);
                app.update();
            }
        }
        start_running(app);

        // `setup_orbital_selection` has spawned the tether root and the debris with
        // `Orbit::FromElements`; `init_orbitals` turns them into propagators on the
        // next update.
        let pending: Vec<(Entity, Vector6<f64>)> = app
            .world_mut()
            .query::<(Entity, &Orbit)>()
            .iter(app.world())
            .map(|(entity, orbit)| match orbit {
                Orbit::FromElements(elements) => (entity, *elements),
                Orbit::FromTle(_) => panic!("unexpected TLE orbit"),
            })
            .collect();
        assert_eq!(
            pending.len(),
            2,
            "precondition: root and debris await init_orbitals"
        );
        if epoch_mode == EpochMode::Fixed {
            // Replace `init_orbitals`' wall-clock start with the fixed epoch.
            for (entity, elements) in pending {
                let mut entity = app.world_mut().entity_mut(entity);
                entity.remove::<Orbit>();
                entity
                    .get_mut::<Orbital>()
                    .expect("Orbit requires Orbital")
                    .propagator = Some(fixed_propagator(kepler_eci(elements, fixed_epoch())));
            }
        }
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

        // As `ui/plugin.rs` does on "Capture".
        let start_time = app.world().resource::<Time<Physics>>().elapsed_secs_f64();
        let component =
            build_capture_component(PLAN_ID, &plan, start_time).expect("example_plan has states");
        app.world_mut().entity_mut(handles.debris).insert(component);
        tick(app);
        handles
    }

    pub(crate) fn sample(app: &App, handles: &Q13Handles) -> Q13Sample {
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
    pub(crate) fn run(epoch_mode: EpochMode, max_ticks: u64, stop_at_capture: bool) -> Q13Run {
        let started = Instant::now();
        let mut app = test_app();
        let handles = build(&mut app, epoch_mode);
        assert_eq!(
            sample(&app, &handles).state,
            "rendezvous",
            "precondition: no transition before the run starts"
        );

        let mut tap = LogTap::new(app.world());
        let mut transitions = Vec::new();
        let mut checkpoints = Vec::new();
        let mut ticks = 0;
        let mut final_sample = sample(&app, &handles);
        while ticks < max_ticks {
            tick(&mut app);
            ticks += 1;
            final_sample = sample(&app, &handles);
            for (_, source, message) in tap.drain(app.world()) {
                if source == "capture"
                    && let Some((from, to, _)) = parse_transition(&message)
                {
                    transitions.push((final_sample.step, from, to));
                }
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
