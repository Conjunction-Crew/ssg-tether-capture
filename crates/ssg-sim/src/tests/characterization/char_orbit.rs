//! Characterization of Kepler propagation of the cached ECI state and of the
//! start epoch (F-06, fixed in P4).

use crate::anchor::CameraTarget;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::{ISS_ORBIT, Orbit, Orbital};
use crate::test_support::{
    assert_close, assert_close_vec6, enter_sim, fixed_epoch, fixed_propagator, kepler_eci,
    start_running, test_app, ticks,
};
use crate::time::{SimConfig, WorldTime};
use avian3d::prelude::*;
use bevy::prelude::*;
use brahe::propagators::traits::SStatePropagator;
use brahe::utils::DOrbitStateProvider;
use nalgebra::Vector6;

fn eci_of(app: &App, entity: Entity) -> Vector6<f64> {
    *app.world()
        .resource::<OrbitalCache>()
        .eci_states
        .get(&entity)
        .expect("no cached ECI state for the entity")
}

/// ECI position within 1e-3 m and velocity within 1e-6 m/s (cross-platform libm
/// tolerance, A-3).
#[track_caller]
fn assert_eci_close(actual: &Vector6<f64>, golden: &[f64; 6], what: &str) {
    for i in 0..6 {
        let tol = if i < 3 { 1e-3 } else { 1e-6 };
        assert_close(actual[i], golden[i], tol, &format!("{what}[{i}]"));
    }
}

#[test]
fn char_orbit_iss_kepler_steps() {
    let mut app = test_app();
    enter_sim(&mut app);
    start_running(&mut app);

    // Anchor body on the ISS orbit, with no `Orbit` so `init_orbitals` (which reads
    // the wall clock, F-06) never sees it. It is not a `TetherRoot`, so the floating
    // origin never resets and its propagator is never rebuilt.
    let anchor = app
        .world_mut()
        .spawn((
            CameraTarget,
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Position::from_xyz(0.0, 0.0, 0.0),
            Transform::from_xyz(0.0, 0.0, 0.0),
            Orbital {
                propagator: Some(fixed_propagator(kepler_eci(ISS_ORBIT, fixed_epoch()))),
                ..default()
            },
        ))
        .id();
    let propagator = app
        .world()
        .get::<Orbital>(anchor)
        .unwrap()
        .propagator
        .clone()
        .unwrap();

    // Golden values captured at c4b8cca on macos/aarch64 (see plans/stage-2/52-p2-testing-baseline.md)
    // (ticks after `start_running`, elapsed sim seconds since `fixed_epoch`, ECI state).
    // The elapsed time includes the one step `start_running` itself takes.
    let goldens: [(u64, f64, [f64; 6]); 2] = [
        (
            3_840,
            60.015625,
            [
                -6112984.669576882,
                -2952811.5361521603,
                357044.3910664104,
                2361.676567330897,
                -4145.316872591764,
                5990.037347745259,
            ],
        ),
        (
            38_400,
            600.015625,
            [
                -3817931.627955201,
                -4525762.093674809,
                3331617.9669669215,
                5875.654511043081,
                -1498.7250693180072,
                4684.419590231248,
            ],
        ),
    ];

    let mut done = 0;
    for (n, golden_elapsed, golden_eci) in goldens {
        ticks(&mut app, n - done);
        done = n;

        // Precondition: the physics step ran (the cache holds the anchor) and no
        // centre of mass exists, i.e. the origin cannot reset.
        assert!(app.world().resource::<OrbitalCache>().com_rv.is_empty());
        let eci = eci_of(&app, anchor);
        let world_time = app.world().resource::<WorldTime>();
        assert_close(
            world_time.epoch - fixed_epoch(),
            golden_elapsed,
            1e-9,
            &format!("elapsed sim time after {n} ticks"),
        );
        assert_eci_close(&eci, &golden_eci, &format!("ECI state after {n} ticks"));

        // Cross-check (not golden): the cached state is the propagator's state at the
        // current epoch.
        let direct = propagator.state_eci(world_time.epoch).unwrap();
        assert_close_vec6(&eci, &direct, 1e-6, "cached vs direct propagation");
    }
}

#[test]
fn time_start_epoch_from_config() {
    // F-06: fixed in P4
    // `setup_time` takes both epochs from `SimConfig`, never from the wall clock.
    let start = fixed_epoch() + 1000.0;
    let mut app = test_app();
    app.insert_resource(SimConfig { start_epoch: start });
    enter_sim(&mut app);

    let world_time = app.world().resource::<WorldTime>();
    assert_eq!(world_time.epoch, start);
    assert_eq!(world_time.start_epoch, start);
    assert_eq!(world_time.multiplier, 1);
}

#[test]
fn time_init_orbitals_at_start_epoch() {
    // F-06: fixed in P4
    let mut app = test_app();
    enter_sim(&mut app);
    start_running(&mut app);
    // Precondition: the clock has moved past the start epoch.
    assert!(app.world().resource::<WorldTime>().epoch > fixed_epoch());

    let entity = app.world_mut().spawn(Orbit::FromElements(ISS_ORBIT)).id();
    app.update();

    // `init_orbitals` ran: `Orbit` was consumed and a propagator exists.
    assert!(app.world().get::<Orbit>(entity).is_none());
    let propagator = app
        .world()
        .get::<Orbital>(entity)
        .expect("Orbital missing")
        .propagator
        .as_ref()
        .expect("init_orbitals did not build a propagator");

    // It starts at the start epoch, with the elements' state at that epoch.
    assert_eq!(propagator.initial_epoch(), fixed_epoch());
    assert_close_vec6(
        &propagator.state_eci(fixed_epoch()).unwrap(),
        &kepler_eci(ISS_ORBIT, fixed_epoch()),
        1e-6,
        "propagator state at the start epoch",
    );
}
