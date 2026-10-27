//! Characterization of time warp: the N-by-1 invariant, the keyboard ladder
//! (F-12), pause, the catch-up cap, and the single physics-clock advance (F-38).

use crate::anchor::CameraTarget;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::{ISS_ORBIT, Orbital};
use crate::test_support::{
    assert_close, enter_sim, fixed_epoch, fixed_propagator, kepler_eci, start_running, test_app,
    tick, ticks,
};
use crate::tether::{TetherNode, TetherRoot};
use crate::time::{PHYS_DT, WorldTime};
use avian3d::prelude::*;
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
fn time_physics_clock_single_advance() {
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
    // F-38: fixed in P4
    // Only avian's `run_physics_schedule` advances `Time<Physics>`, by the generic
    // `Time` delta (`Time<Fixed>` = `PHYS_DT` inside `FixedUpdate`).
    assert_close(
        physics_advance,
        PHYS_DT,
        1e-9,
        "Time<Physics> advance per tick",
    );
}
