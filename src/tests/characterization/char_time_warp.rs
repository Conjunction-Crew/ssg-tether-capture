//! Characterization of time warp: the N-by-1 invariant, the keyboard ladder
//! (F-12), pause, the catch-up cap, and the physics-clock double advance (F-38).

use crate::components::orbit::{Orbital, TetherNode, TetherRoot};
use crate::components::orbit_camera::CameraTarget;
use crate::constants::ISS_ORBIT;
use crate::resources::capture_log::LogLevel;
use crate::resources::orbital_cache::OrbitalCache;
use crate::resources::world_time::WorldTime;
use crate::systems::physics::{PHYS_DT, fixed_physics_step};
use crate::systems::user_input::change_time_warp;
use crate::tests::test_support::{
    LogTap, assert_close, enter_sim, fixed_epoch, fixed_propagator, kepler_eci, start_running,
    test_app, tick, ticks,
};
use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::DVec3;
use bevy::prelude::*;
use nalgebra::Vector6;

/// A harness app in `SimState::Running`.
fn running_app() -> App {
    let mut app = test_app();
    enter_sim(&mut app);
    start_running(&mut app);
    app
}

/// The entities of the warp scenario, in spawn order.
struct Scenario {
    anchor: Entity,
    node: Entity,
    far: Entity,
}

/// An ISS anchor (`CameraTarget` + `TetherRoot`, moving), a second dynamic body
/// joined to it like `spawners.rs:186-195`, and a disabled orbital body 300 m away.
fn warp_scenario(app: &mut App) -> Scenario {
    let eci = kepler_eci(ISS_ORBIT, fixed_epoch());
    let anchor = app
        .world_mut()
        .spawn((
            CameraTarget,
            TetherRoot,
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Position::from_xyz(0.0, 0.0, 0.0),
            Transform::from_xyz(0.0, 0.0, 0.0),
            LinearVelocity(DVec3::new(0.3, 0.1, 0.0)),
            Orbital {
                propagator: Some(fixed_propagator(eci)),
                ..default()
            },
        ))
        .id();
    let node = app
        .world_mut()
        .spawn((
            TetherNode { root: anchor },
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Position::from_xyz(0.0, 5.0, 0.0),
            Transform::from_xyz(0.0, 5.0, 0.0),
            Mass::from(2.0),
        ))
        .id();
    app.world_mut().spawn((
        DistanceJoint::new(anchor, node).with_anchor(DVec3::new(0.0, 2.5, 0.0)),
        JointDamping {
            linear: 1.0,
            angular: 1.0,
        },
        JointCollisionDisabled,
    ));
    let far_eci = eci + Vector6::new(300.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let far = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Position::from_xyz(300.0, 0.0, 0.0),
            Transform::from_xyz(300.0, 0.0, 0.0),
            RigidBodyDisabled,
            Orbital {
                propagator: Some(fixed_propagator(far_eci)),
                ..default()
            },
        ))
        .id();
    Scenario { anchor, node, far }
}

/// Every number the invariant compares, as bit patterns.
#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    elapsed: u64,
    eci: Vec<(usize, Vec<u64>)>,
    bodies: Vec<Vec<u64>>,
}

fn snapshot(app: &App, scenario: &Scenario) -> Snapshot {
    let world = app.world();
    let cache = world.resource::<OrbitalCache>();
    let eci = [scenario.anchor, scenario.far]
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            cache
                .eci_states
                .get(e)
                .map(|s| (i, s.iter().map(|x| x.to_bits()).collect()))
        })
        .collect();
    let bodies = [scenario.anchor, scenario.node, scenario.far]
        .iter()
        .map(|&e| {
            let p = world.get::<Position>(e).unwrap().0;
            let v = world
                .get::<LinearVelocity>(e)
                .map(|v| v.0)
                .unwrap_or_default();
            [p.x, p.y, p.z, v.x, v.y, v.z]
                .iter()
                .map(|x| x.to_bits())
                .collect()
        })
        .collect();
    Snapshot {
        elapsed: (world.resource::<WorldTime>().epoch - fixed_epoch()).to_bits(),
        eci,
        bodies,
    }
}

/// `n` ticks at `multiplier` from a fresh scenario.
fn run_warp(multiplier: u32, n: u64) -> Snapshot {
    let mut app = running_app();
    let scenario = warp_scenario(&mut app);
    app.world_mut().resource_mut::<WorldTime>().multiplier = multiplier;
    ticks(&mut app, n);
    snapshot(&app, &scenario)
}

#[test]
fn char_warp_n_by_1_equals_1_by_n() {
    // Precondition: the scenario actually moves, so equal snapshots are not trivial.
    let start = run_warp(1, 0);
    let moved = run_warp(1, 8);
    assert_ne!(start, moved);
    assert!(!moved.eci.is_empty());

    // 8 ticks at x1 vs 1 tick at x8.
    assert_eq!(run_warp(1, 8), run_warp(8, 1));
    // 1024 ticks at x1 vs 128 ticks at x8.
    assert_eq!(run_warp(1, 1024), run_warp(8, 128));
}

/// Press `key` and run `change_time_warp` once; returns the multiplier afterwards
/// and the Info messages it logged.
fn press(app: &mut App, key: KeyCode) -> (u32, Vec<String>) {
    let mut tap = LogTap::new(app.world());
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.world_mut()
        .run_system_once(change_time_warp)
        .expect("change_time_warp failed to run");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    let messages = tap
        .drain(app.world())
        .into_iter()
        .filter(|(level, source, _)| *level == LogLevel::Info && *source == "ui")
        .map(|(_, _, message)| message)
        .collect();
    (app.world().resource::<WorldTime>().multiplier, messages)
}

#[test]
fn char_warp_keyboard_ladder() {
    let mut app = running_app();
    assert_eq!(app.world().resource::<WorldTime>().multiplier, 1);

    // Period doubles from 1 up to 8192, and then stays: 8192 * 2 > 10000
    // (`user_input.rs:56`, `:70`).
    let mut up = Vec::new();
    for _ in 0..13 {
        let (multiplier, messages) = press(&mut app, KeyCode::Period);
        assert_eq!(messages, vec![format!("Time warp → {multiplier}x")]);
        up.push(multiplier);
    }
    // F-12: current behaviour, expected to change in P4
    assert_eq!(
        up,
        [2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192]
    );
    let (multiplier, messages) = press(&mut app, KeyCode::Period);
    // F-12: current behaviour, expected to change in P4
    assert_eq!(multiplier, 8192);
    assert!(messages.is_empty());

    // Comma halves down to 1 and then 0.
    let mut down = Vec::new();
    for _ in 0..14 {
        let (multiplier, messages) = press(&mut app, KeyCode::Comma);
        assert_eq!(messages, vec![format!("Time warp → {multiplier}x")]);
        down.push(multiplier);
    }
    // F-12: current behaviour, expected to change in P4
    assert_eq!(
        down,
        [4096, 2048, 1024, 512, 256, 128, 64, 32, 16, 8, 4, 2, 1, 0]
    );

    // Period from 0 goes to 1.
    let (multiplier, messages) = press(&mut app, KeyCode::Period);
    // F-12: current behaviour, expected to change in P4
    assert_eq!(multiplier, 1);
    assert_eq!(messages, vec!["Time warp → 1x".to_string()]);
}

#[test]
fn char_warp_pause_is_zero_multiplier() {
    let mut app = running_app();
    let scenario = warp_scenario(&mut app);
    // One moving tick so the cache holds the states.
    tick(&mut app);
    let before = snapshot(&app, &scenario);

    app.world_mut().resource_mut::<WorldTime>().multiplier = 0;
    ticks(&mut app, 5);
    // F-12: current behaviour, expected to change in P4
    assert_eq!(snapshot(&app, &scenario), before);

    // Control: at x1 the same scenario moves.
    app.world_mut().resource_mut::<WorldTime>().multiplier = 1;
    tick(&mut app);
    assert_ne!(snapshot(&app, &scenario), before);
}

#[test]
fn char_warp_catch_up_cap() {
    let app = running_app();
    // F-12: current behaviour, expected to change in P4
    // A slow frame is clamped to 250 ms of virtual time, which is 16 fixed steps,
    // so a x8192 frame on a slow machine silently falls behind the readout.
    assert_eq!(
        app.world().resource::<Time<Virtual>>().max_delta(),
        std::time::Duration::from_millis(250)
    );
    assert_close(
        app.world()
            .resource::<Time<Fixed>>()
            .timestep()
            .as_secs_f64(),
        PHYS_DT,
        1e-15,
        "fixed timestep",
    );
}

#[test]
fn char_time_physics_clock_double_advance() {
    let mut app = running_app();
    let physics_secs = |app: &App| {
        app.world()
            .resource::<Time<Physics>>()
            .elapsed()
            .as_secs_f64()
    };

    let epoch_before = app.world().resource::<WorldTime>().epoch;
    let physics_before = physics_secs(&app);
    tick(&mut app);
    let epoch_advance = app.world().resource::<WorldTime>().epoch - epoch_before;
    let physics_advance = physics_secs(&app) - physics_before;

    assert_close(epoch_advance, PHYS_DT, 1e-12, "sim epoch advance per tick");
    // F-38 (proposed in P2): current behaviour, expected to change in P4
    // `physics.rs:17` advances `Time<Physics>` once and avian's
    // `run_physics_schedule` advances it again by the generic `Time` delta, so the
    // physics clock runs at twice the sim clock.
    assert_close(
        physics_advance,
        2.0 * PHYS_DT,
        1e-9,
        "Time<Physics> advance per tick",
    );

    // The trap behind `tick`: calling `fixed_physics_step` from test code, outside
    // `FixedUpdate`, leaves the generic `Time` at whatever the last frame's delta
    // was. This is the only place a test may do it.
    let mut app = running_app();
    let epoch_before = app.world().resource::<WorldTime>().epoch;
    let physics_before = physics_secs(&app);
    fixed_physics_step(app.world_mut());
    let epoch_advance = app.world().resource::<WorldTime>().epoch - epoch_before;
    let physics_advance = physics_secs(&app) - physics_before;
    assert_close(
        epoch_advance,
        PHYS_DT,
        1e-12,
        "sim epoch advance, direct call",
    );
    // Observed: the generic `Time` delta is still 1/64 s at that moment, so the
    // physics clock advances by `PHYS_DT` plus 1/64 s. With a different last-frame
    // delta (or none) the advance, and avian's step, would differ: that is why
    // tests use `tick`.
    // F-38 (proposed in P2): current behaviour, expected to change in P4
    assert_close(
        physics_advance,
        2.0 * PHYS_DT,
        1e-9,
        "Time<Physics> advance, direct call",
    );
}
