//! Direct tests for the small systems of `time.rs`, `physics_bubble.rs` and
//! `floating_origin.rs` that the characterization tests reach only through full
//! scenarios (P5 S6): the warp ladder, the fixed-step loop, the clock reset, the
//! bubble's ECI helper and relative gravity, and the centre-of-mass edge cases.
//!
//! Systems run through `run_system_once`; none of these tests steps avian (D8).

use std::time::Duration;

use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::DVec3;
use bevy::prelude::*;
use nalgebra::Vector6;

use crate::anchor::SimAnchor;
use crate::floating_origin::calculate_com_rv;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::lvlh::differential_gravity;
use crate::physics_bubble::{apply_relative_gravity, body_eci_state};
use crate::telemetry::{LogEvent, LogLevel};
use crate::test_support::{LogTap, assert_close, assert_close_dvec3, fixed_epoch, test_app};
use crate::tether::{TetherNode, TetherRoot};
use crate::time::{
    MAX_STEPS_PER_UPDATE, MAX_TIME_WARP, ManualPhysics, PHYS_DT, SimConfig, WarpChange, WorldTime,
    configure_catch_up_window, fixed_physics_step, setup_time,
};

// ---------------------------------------------------------------------------
// WorldTime::starting_at / setup_time
// ---------------------------------------------------------------------------

#[test]
fn starting_at_is_a_1x_clock_at_the_epoch_with_no_steps_run() {
    let epoch = fixed_epoch();

    let time = WorldTime::starting_at(epoch);

    assert_eq!(time.multiplier, 1);
    assert_eq!(time.effective_multiplier, 0);
    assert_eq!(time.epoch, epoch);
    assert_eq!(time.start_epoch, epoch);
}

#[test]
fn setup_time_resets_the_clock_to_the_configured_epoch_and_logs_it() {
    let start = fixed_epoch() + 86_400.0;
    let mut app = App::new();
    app.add_message::<LogEvent>()
        .insert_resource(SimConfig { start_epoch: start })
        .insert_resource(WorldTime {
            multiplier: 512,
            effective_multiplier: 64,
            epoch: fixed_epoch() + 5.0,
            start_epoch: fixed_epoch(),
        });
    let mut tap = LogTap::new(app.world());

    app.world_mut()
        .run_system_once(setup_time)
        .expect("setup_time failed to run");

    let time = app.world().resource::<WorldTime>();
    assert_eq!(time.epoch, start);
    assert_eq!(time.start_epoch, start);
    assert_eq!(time.multiplier, 1, "the warp is back to 1x");
    assert_eq!(time.effective_multiplier, 0);
    let logs = tap.drain(app.world());
    assert_eq!(
        logs,
        [(LogLevel::Info, "time", format!("Sim start epoch: {start}"))]
    );
}

// ---------------------------------------------------------------------------
// WorldTime::change_warp
// ---------------------------------------------------------------------------

fn clock_at(multiplier: u32) -> WorldTime {
    WorldTime {
        multiplier,
        ..WorldTime::starting_at(fixed_epoch())
    }
}

#[test]
fn change_warp_up_doubles_to_the_top_rung_and_then_stays() {
    let mut time = clock_at(1);
    let mut seen = Vec::new();

    loop {
        match time.change_warp(true, false) {
            WarpChange::Multiplier(m) => seen.push(m),
            other => {
                assert_eq!(
                    other,
                    WarpChange::Unchanged,
                    "past the top: nothing changes"
                );
                break;
            }
        }
    }

    let expected: Vec<u32> = (1..=13).map(|n| 1u32 << n).collect();
    assert_eq!(seen, expected, "2, 4, ..., 8192");
    assert_eq!(time.multiplier, MAX_TIME_WARP);
    assert_eq!(time.change_warp(true, false), WarpChange::Unchanged);
    assert_eq!(
        time.multiplier, MAX_TIME_WARP,
        "the multiplier did not move"
    );
}

#[test]
fn change_warp_down_halves_to_1x_and_then_pauses() {
    let mut time = clock_at(MAX_TIME_WARP);
    let mut seen = Vec::new();

    while let WarpChange::Multiplier(m) = time.change_warp(false, false) {
        seen.push(m);
    }

    let expected: Vec<u32> = (0..13).rev().map(|n| 1u32 << n).collect();
    assert_eq!(seen, expected, "4096, 2048, ..., 1");
    assert_eq!(time.multiplier, 1);
    assert_eq!(
        time.change_warp(false, false),
        WarpChange::Paused,
        "down at 1x pauses"
    );
    assert_eq!(
        time.multiplier, 1,
        "pausing leaves the multiplier at 1, never 0"
    );
}

#[test]
fn change_warp_up_while_paused_resumes_at_the_current_multiplier() {
    let mut time = clock_at(8);

    assert_eq!(time.change_warp(true, true), WarpChange::Resumed);
    assert_eq!(time.multiplier, 8, "resuming does not double");
}

#[test]
fn change_warp_down_while_paused_changes_nothing() {
    let mut time = clock_at(8);

    assert_eq!(time.change_warp(false, true), WarpChange::Unchanged);
    assert_eq!(time.multiplier, 8);
}

#[test]
fn change_warp_from_an_off_ladder_multiplier_halves_with_integer_division() {
    let mut time = clock_at(3);
    assert_eq!(time.change_warp(false, false), WarpChange::Multiplier(1));

    // Above half the top rung the double would pass 8192: no change.
    let mut time = clock_at(5000);
    assert_eq!(time.change_warp(true, false), WarpChange::Unchanged);
    assert_eq!(time.multiplier, 5000);

    // 4096 doubled is exactly the top rung, which is allowed.
    let mut time = clock_at(4096);
    assert_eq!(
        time.change_warp(true, false),
        WarpChange::Multiplier(MAX_TIME_WARP)
    );
}

// ---------------------------------------------------------------------------
// fixed_physics_step / configure_catch_up_window
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct PhysicsRuns(u32);

/// A world with only the clock and a `ManualPhysics` schedule that counts its runs:
/// the loop in `fixed_physics_step`, without avian.
fn counting_world(multiplier: u32) -> World {
    let mut world = World::new();
    world.insert_resource(clock_at(multiplier));
    world.init_resource::<PhysicsRuns>();
    let mut schedule = Schedule::new(ManualPhysics);
    schedule.add_systems(|mut runs: ResMut<PhysicsRuns>| runs.0 += 1);
    world.add_schedule(schedule);
    world
}

#[test]
fn fixed_physics_step_runs_the_multiplier_in_steps_and_advances_the_clock() {
    let mut world = counting_world(5);

    world
        .run_system_once(fixed_physics_step)
        .expect("fixed_physics_step failed to run");

    assert_eq!(world.resource::<PhysicsRuns>().0, 5);
    let time = world.resource::<WorldTime>();
    assert_eq!(time.effective_multiplier, 5);
    assert_eq!(time.multiplier, 5, "the requested warp is untouched");
    assert_close(
        time.epoch - time.start_epoch,
        5.0 * PHYS_DT,
        1e-12,
        "elapsed sim time",
    );
}

#[test]
fn fixed_physics_step_caps_one_run_at_max_steps_per_update() {
    let mut world = counting_world(MAX_TIME_WARP);

    world
        .run_system_once(fixed_physics_step)
        .expect("fixed_physics_step failed to run");

    assert_eq!(world.resource::<PhysicsRuns>().0, MAX_STEPS_PER_UPDATE);
    let time = world.resource::<WorldTime>();
    assert_eq!(time.effective_multiplier, MAX_STEPS_PER_UPDATE);
    assert_eq!(time.multiplier, MAX_TIME_WARP, "the request stays 8192x");
    assert_close(
        time.epoch - time.start_epoch,
        f64::from(MAX_STEPS_PER_UPDATE) * PHYS_DT,
        1e-12,
        "elapsed sim time: no step debt is carried over",
    );
}

#[test]
fn fixed_physics_step_accumulates_over_runs() {
    let mut world = counting_world(2);

    for _ in 0..3 {
        world.run_system_once(fixed_physics_step).unwrap();
    }

    assert_eq!(world.resource::<PhysicsRuns>().0, 6);
    let time = world.resource::<WorldTime>();
    assert_close(
        time.epoch - time.start_epoch,
        6.0 * PHYS_DT,
        1e-12,
        "elapsed",
    );
}

#[test]
fn configure_catch_up_window_limits_virtual_time_to_three_fixed_steps() {
    let mut world = World::new();
    world.insert_resource(Time::<Virtual>::default());
    assert_eq!(
        world.resource::<Time<Virtual>>().max_delta(),
        Duration::from_millis(250),
        "precondition: Bevy's default window"
    );

    world
        .run_system_once(configure_catch_up_window)
        .expect("configure_catch_up_window failed to run");

    assert_eq!(
        world.resource::<Time<Virtual>>().max_delta(),
        Duration::from_secs_f64(3.0 * PHYS_DT)
    );
}

// ---------------------------------------------------------------------------
// physics_bubble::body_eci_state
// ---------------------------------------------------------------------------

fn state(a: f64) -> Vector6<f64> {
    Vector6::new(a, a + 1.0, a + 2.0, a + 3.0, a + 4.0, a + 5.0)
}

#[test]
fn body_eci_state_of_an_enabled_body_adds_its_bubble_state_to_the_cached_one() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let mut cache = OrbitalCache::default();
    cache.eci_states.insert(entity, state(100.0));

    let eci = body_eci_state(
        &cache,
        entity,
        DVec3::new(1.0, 2.0, 3.0),
        DVec3::new(0.1, 0.2, 0.3),
        false,
    )
    .expect("the entity has a cached state");

    assert_eq!(
        eci,
        Vector6::new(101.0, 103.0, 105.0, 103.1, 104.2, 105.3),
        "position goes to components 0-2, velocity to 3-5"
    );
}

#[test]
fn body_eci_state_of_a_disabled_body_is_its_cached_state() {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let mut cache = OrbitalCache::default();
    cache.eci_states.insert(entity, state(100.0));

    let eci = body_eci_state(&cache, entity, DVec3::splat(50.0), DVec3::splat(5.0), true);

    assert_eq!(eci, Some(state(100.0)), "the bubble state is ignored");
}

#[test]
fn body_eci_state_is_none_without_a_cached_state() {
    let mut world = World::new();
    let known = world.spawn_empty().id();
    let unknown = world.spawn_empty().id();
    let mut cache = OrbitalCache::default();
    cache.eci_states.insert(known, state(1.0));

    for disabled in [false, true] {
        assert_eq!(
            body_eci_state(&cache, unknown, DVec3::ZERO, DVec3::ZERO, disabled),
            None,
            "disabled = {disabled}"
        );
    }
}

// ---------------------------------------------------------------------------
// physics_bubble::apply_relative_gravity
// ---------------------------------------------------------------------------

/// An app (no sim screen) with an anchor whose cached ECI state is a point on the
/// ISS orbit's radius, and `OrbitalCache` holding it. Returns the app, the anchor
/// and the frame origin `r_o`.
fn gravity_app() -> (App, Entity, DVec3) {
    let mut app = test_app();
    let anchor = app
        .world_mut()
        .spawn((SimAnchor, RigidBody::Dynamic, Position::default()))
        .id();
    let r_o = DVec3::new(6.8e6, 0.0, 0.0);
    let mut cache = OrbitalCache::default();
    cache
        .eci_states
        .insert(anchor, Vector6::new(r_o.x, r_o.y, r_o.z, 0.0, 7.5e3, 0.0));
    app.insert_resource(cache);
    (app, anchor, r_o)
}

fn accumulated_acceleration(app: &mut App, entity: Entity) -> DVec3 {
    app.world_mut()
        .run_system_once(move |forces: Query<Forces>| {
            forces
                .get(entity)
                .expect("the body has Forces")
                .accumulated_linear_acceleration()
        })
        .expect("force query failed to run")
}

fn run_gravity(app: &mut App) {
    app.world_mut()
        .run_system_once(apply_relative_gravity)
        .expect("apply_relative_gravity failed to run");
}

#[test]
fn relative_gravity_gives_each_dynamic_body_the_differential_acceleration() {
    let (mut app, _anchor, r_o) = gravity_app();
    let near = DVec3::new(100.0, 0.0, 0.0);
    let off_axis = DVec3::new(-30.0, 40.0, 50.0);
    let a = app
        .world_mut()
        .spawn((RigidBody::Dynamic, Position(near)))
        .id();
    let b = app
        .world_mut()
        .spawn((RigidBody::Dynamic, Position(off_axis)))
        .id();

    run_gravity(&mut app);

    let expected_a = differential_gravity(r_o, near);
    assert!(
        expected_a.length() > 1e-6,
        "precondition: a visible tidal term"
    );
    assert_close_dvec3(
        accumulated_acceleration(&mut app, a),
        expected_a,
        1e-15,
        "radial body",
    );
    assert_close_dvec3(
        accumulated_acceleration(&mut app, b),
        differential_gravity(r_o, off_axis),
        1e-15,
        "off-axis body",
    );
}

/// Runs the system with `body` as spawned, then turns it into an enabled dynamic
/// body (avian's `Forces` query does not match disabled or non-dynamic bodies) and
/// returns what the system accumulated on it.
fn gravity_left_on(
    body_components: impl Bundle,
    make_dynamic: impl FnOnce(&mut EntityWorldMut),
) -> DVec3 {
    let (mut app, _anchor, _r_o) = gravity_app();
    let body = app.world_mut().spawn(body_components).id();

    run_gravity(&mut app);

    make_dynamic(&mut app.world_mut().entity_mut(body));
    accumulated_acceleration(&mut app, body)
}

#[test]
fn relative_gravity_skips_static_and_kinematic_bodies() {
    let p = Position(DVec3::new(100.0, 0.0, 0.0));
    for (label, body) in [
        ("static", RigidBody::Static),
        ("kinematic", RigidBody::Kinematic),
    ] {
        let left = gravity_left_on((body, p), |e| {
            e.insert(RigidBody::Dynamic);
        });
        assert_eq!(left, DVec3::ZERO, "{label} body");
    }
}

#[test]
fn relative_gravity_without_a_cached_anchor_state_applies_nothing() {
    let (mut app, anchor, _r_o) = gravity_app();
    app.world_mut()
        .resource_mut::<OrbitalCache>()
        .eci_states
        .remove(&anchor);
    let body = app
        .world_mut()
        .spawn((RigidBody::Dynamic, Position(DVec3::new(100.0, 0.0, 0.0))))
        .id();

    run_gravity(&mut app);

    assert_eq!(accumulated_acceleration(&mut app, body), DVec3::ZERO);
}

// ---------------------------------------------------------------------------
// floating_origin::calculate_com_rv
// ---------------------------------------------------------------------------

fn com_app() -> App {
    let mut app = test_app();
    app.insert_resource(OrbitalCache::default());
    app
}

fn spawn_massive(app: &mut App, mass: f32, position: DVec3, velocity: DVec3) -> Entity {
    app.world_mut()
        .spawn((
            RigidBody::Dynamic,
            Mass(mass),
            Position(position),
            LinearVelocity(velocity),
        ))
        .id()
}

fn run_com(app: &mut App) {
    app.world_mut()
        .run_system_once(calculate_com_rv)
        .expect("calculate_com_rv failed to run");
}

fn com_of(app: &App, root: Entity) -> Option<(DVec3, DVec3)> {
    app.world()
        .resource::<OrbitalCache>()
        .com_rv
        .get(&root)
        .copied()
}

#[test]
fn com_rv_is_the_mass_weighted_mean_of_the_root_and_its_nodes() {
    let mut app = com_app();
    let root = spawn_massive(&mut app, 2.0, DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0));
    let node = spawn_massive(
        &mut app,
        1.0,
        DVec3::new(9.0, 0.0, 0.0),
        DVec3::new(0.0, 3.0, 0.0),
    );
    app.world_mut().entity_mut(root).insert(TetherRoot);
    app.world_mut().entity_mut(node).insert(TetherNode { root });

    run_com(&mut app);

    let (r, v) = com_of(&app, root).expect("the root has a centre of mass");
    assert_close_dvec3(r, DVec3::new(3.0, 0.0, 0.0), 1e-12, "com position");
    assert_close_dvec3(v, DVec3::new(2.0 / 3.0, 1.0, 0.0), 1e-12, "com velocity");
}

#[test]
fn com_rv_of_a_lone_root_is_its_own_state() {
    let mut app = com_app();
    let root = spawn_massive(
        &mut app,
        5.0,
        DVec3::new(1.0, 2.0, 3.0),
        DVec3::new(0.5, 0.0, -0.5),
    );
    app.world_mut().entity_mut(root).insert(TetherRoot);

    run_com(&mut app);

    let (r, v) = com_of(&app, root).unwrap();
    assert_close_dvec3(r, DVec3::new(1.0, 2.0, 3.0), 1e-12, "com position");
    assert_close_dvec3(v, DVec3::new(0.5, 0.0, -0.5), 1e-12, "com velocity");
}

#[test]
fn com_rv_counts_a_body_at_its_centre_of_mass_offset() {
    let mut app = com_app();
    // Rotated 90 degrees about +Z, a centre of mass at local +X sits at global +Y.
    let root = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Mass(1.0),
            CenterOfMass(Vec3::new(2.0, 0.0, 0.0)),
            Position(DVec3::new(10.0, 0.0, 0.0)),
            Rotation::from(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            TetherRoot,
        ))
        .id();

    run_com(&mut app);

    let (r, _) = com_of(&app, root).unwrap();
    assert_close_dvec3(r, DVec3::new(10.0, 2.0, 0.0), 1e-6, "com position");
}

#[test]
fn com_rv_ignores_nodes_of_other_roots() {
    let mut app = com_app();
    let root_a = spawn_massive(&mut app, 1.0, DVec3::ZERO, DVec3::ZERO);
    let root_b = spawn_massive(&mut app, 1.0, DVec3::new(100.0, 0.0, 0.0), DVec3::ZERO);
    let node_a = spawn_massive(&mut app, 1.0, DVec3::new(2.0, 0.0, 0.0), DVec3::ZERO);
    let node_b = spawn_massive(&mut app, 3.0, DVec3::new(104.0, 0.0, 0.0), DVec3::ZERO);
    app.world_mut().entity_mut(root_a).insert(TetherRoot);
    app.world_mut().entity_mut(root_b).insert(TetherRoot);
    app.world_mut()
        .entity_mut(node_a)
        .insert(TetherNode { root: root_a });
    app.world_mut()
        .entity_mut(node_b)
        .insert(TetherNode { root: root_b });

    run_com(&mut app);

    assert_close_dvec3(
        com_of(&app, root_a).unwrap().0,
        DVec3::new(1.0, 0.0, 0.0),
        1e-12,
        "A",
    );
    assert_close_dvec3(
        com_of(&app, root_b).unwrap().0,
        DVec3::new(103.0, 0.0, 0.0),
        1e-12,
        "B",
    );
}

#[test]
fn com_rv_leaves_out_a_disabled_node() {
    let mut app = com_app();
    let root = spawn_massive(&mut app, 1.0, DVec3::ZERO, DVec3::ZERO);
    let node = spawn_massive(&mut app, 1.0, DVec3::new(50.0, 0.0, 0.0), DVec3::ZERO);
    app.world_mut().entity_mut(root).insert(TetherRoot);
    app.world_mut()
        .entity_mut(node)
        .insert((TetherNode { root }, RigidBodyDisabled));

    run_com(&mut app);

    assert_close_dvec3(
        com_of(&app, root).unwrap().0,
        DVec3::ZERO,
        1e-12,
        "the disabled node does not pull the centre of mass",
    );
}

#[test]
fn com_rv_has_no_entry_for_a_disabled_root() {
    let mut app = com_app();
    let root = spawn_massive(&mut app, 1.0, DVec3::new(5.0, 0.0, 0.0), DVec3::ZERO);
    app.world_mut()
        .entity_mut(root)
        .insert((TetherRoot, RigidBodyDisabled));

    run_com(&mut app);

    assert_eq!(com_of(&app, root), None);
    assert!(app.world().resource::<OrbitalCache>().com_rv.is_empty());
}

#[test]
fn com_rv_has_no_entry_when_the_total_mass_is_zero() {
    let mut app = com_app();
    let root = spawn_massive(&mut app, 0.0, DVec3::new(5.0, 0.0, 0.0), DVec3::ZERO);
    app.world_mut().entity_mut(root).insert(TetherRoot);

    run_com(&mut app);

    assert_eq!(com_of(&app, root), None, "no division by a zero mass");
}
