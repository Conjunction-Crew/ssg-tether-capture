//! Characterization of Kepler propagation of the cached ECI state, of the start
//! epoch (F-06, fixed in P4) and of the bodies' initial states (SGP4 for catalog
//! picks, elements at their epoch otherwise; F-10, F-27, fixed in P4).

use crate::anchor::SimAnchor;
use crate::capture::plan::CapturePlanLibrary;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::selection::{OrbitalSelectionState, SelectedOrbitalObject};
use crate::orbit::{ISS_ORBIT, Orbit, Orbital};
use crate::project::SelectedProject;
use crate::test_support::{
    assert_close, assert_close_vec6, enter_sim, fixed_epoch, fixed_propagator, kepler_eci,
    load_example_plans, q13, start_running, test_app, ticks,
};
use crate::time::{SimConfig, WorldTime};
use avian3d::prelude::*;
use bevy::prelude::*;
use brahe::propagators::SGPPropagator;
use brahe::propagators::traits::SStatePropagator;
use brahe::utils::DOrbitStateProvider;
use brahe::{AngleFormat, KeplerianPropagator};
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
            SimAnchor,
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

/// The Q13 target and chaser selections, entered and started as the app does,
/// before `init_orbitals` runs: `(app, debris, root)`.
fn spawn_pair(
    target: SelectedOrbitalObject,
    chaser: SelectedOrbitalObject,
) -> (App, Entity, Entity) {
    let mut app = test_app();
    load_example_plans(&mut app);
    app.world_mut().resource_mut::<SelectedProject>().project_id = Some(q13::PLAN_ID.to_string());
    {
        let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
        selection.target = Some(target);
        selection.chaser = Some(chaser);
    }
    enter_sim(&mut app);
    start_running(&mut app);
    let tether = app.world().resource::<CapturePlanLibrary>().plans[q13::PLAN_ID]
        .tether
        .clone();
    let cache = app.world().resource::<OrbitalCache>();
    let debris = *cache.debris.get("Satellite1").expect("debris not spawned");
    let root = cache.tethers.get(&tether).expect("tether not spawned")[0];
    (app, debris, root)
}

fn q13_picks() -> (SelectedOrbitalObject, SelectedOrbitalObject) {
    (
        q13::catalog_pick("HST", q13::HST_NORAD_ID),
        q13::catalog_pick("P0 HST CHASER", q13::CHASER_NORAD_ID),
    )
}

fn propagator_of(app: &App, entity: Entity) -> &KeplerianPropagator {
    app.world()
        .get::<Orbital>(entity)
        .expect("Orbital missing")
        .propagator
        .as_ref()
        .expect("init_orbitals did not build a propagator")
}

/// `(range m, relative speed m/s)` between two ECI states.
fn pair_geometry(a: &Vector6<f64>, b: &Vector6<f64>) -> (f64, f64) {
    let d = b - a;
    (d.fixed_rows::<3>(0).norm(), d.fixed_rows::<3>(3).norm())
}

#[test]
fn sgp4_state_at_start_epoch_matches_brahe() {
    // F-10: fixed in P4
    let (target, chaser) = q13_picks();
    let (mut app, debris, _) = spawn_pair(target, chaser);
    app.update();
    let propagator = propagator_of(&app, debris);
    assert_eq!(propagator.initial_epoch(), fixed_epoch());

    // The HST record's numbers, straight into brahe (zero EOP, installed by
    // `OrbitPlugin`).
    let direct = SGPPropagator::from_omm_elements(
        "2026-04-03T16:03:47.002464",
        15.29899616,
        0.0002132,
        28.4724,
        229.5454,
        51.9469,
        308.1319,
        20580,
        1.0,
        None,
        None,
        Some('U'),
        Some(0.00040809),
        Some(0.00012406),
        Some(0.0),
        Some(0),
        None,
        None,
    )
    .expect("SGP4 from the HST record")
    .state_eci(fixed_epoch())
    .expect("SGP4 state");

    let state = propagator.state_eci(fixed_epoch()).unwrap();
    // Bit-equal to a two-body propagator seeded with brahe's SGP4 state...
    assert_eq!(
        state.map(f64::to_bits),
        KeplerianPropagator::from_eci(fixed_epoch(), direct, 1.0)
            .state_eci(fixed_epoch())
            .unwrap()
            .map(f64::to_bits),
        "start state vs a Kepler propagator seeded from brahe's SGP4 state"
    );
    // ...which returns that state up to the elements round trip.
    let (dr, dv) = pair_geometry(&state, &direct);
    println!("HST start state vs brahe SGP4: |dr| {dr:e} m, |dv| {dv:e} m/s");
    assert!(dr < 1e-6 && dv < 1e-9, "|dr| {dr:e} m, |dv| {dv:e} m/s");
}

#[test]
fn from_elements_at_honours_epoch() {
    // F-27: fixed in P4
    let mut app = test_app();
    enter_sim(&mut app);
    start_running(&mut app);
    let early = fixed_epoch() - 600.0;
    let at_early = app
        .world_mut()
        .spawn(Orbit::FromElementsAt {
            elements: ISS_ORBIT,
            epoch: early,
        })
        .id();
    let at_start = app
        .world_mut()
        .spawn(Orbit::FromElementsAt {
            elements: ISS_ORBIT,
            epoch: fixed_epoch(),
        })
        .id();
    let plain = app.world_mut().spawn(Orbit::FromElements(ISS_ORBIT)).id();
    app.update();

    let state = |entity| {
        propagator_of(&app, entity)
            .state_eci(fixed_epoch())
            .unwrap()
    };
    let (s_early, s_start) = (state(at_early), state(at_start));
    for entity in [at_early, at_start] {
        assert_eq!(propagator_of(&app, entity).initial_epoch(), fixed_epoch());
    }
    // Elements at the start epoch are what `FromElements` means.
    assert_eq!(
        s_start.map(f64::to_bits),
        state(plain).map(f64::to_bits),
        "FromElementsAt at the start epoch vs FromElements"
    );
    assert_close_vec6(
        &s_start,
        &kepler_eci(ISS_ORBIT, fixed_epoch()),
        1e-6,
        "at start",
    );
    // Elements valid 600 s earlier have moved on by 600 s of mean motion.
    let expected_early =
        KeplerianPropagator::from_keplerian(early, ISS_ORBIT, AngleFormat::Radians, 1.0)
            .state_eci(fixed_epoch())
            .unwrap();
    assert_close_vec6(&s_early, &expected_early, 1e-6, "600 s earlier");
    let (apart, _) = pair_geometry(&s_early, &s_start);
    println!("start states 600 s of mean motion apart: {apart:.1} m");
    assert!(apart > 4_000.0, "{apart} m");
}

#[test]
fn sgp4_pair_geometry_close_to_kepler() {
    // F-10: fixed in P4. The planner measured 0.76 m and 0.0038 m/s; a TEME/GCRF
    // or units mistake shows up as kilometres.
    let (target, chaser) = q13_picks();
    let kepler = |pick: &SelectedOrbitalObject| {
        kepler_eci(
            pick.gp.as_ref().unwrap().kepler_elements_rad(),
            fixed_epoch(),
        )
    };
    let (kepler_target, kepler_chaser) = (kepler(&target), kepler(&chaser));
    let (mut app, debris, root) = spawn_pair(target, chaser);
    app.update();
    let state = |entity| {
        propagator_of(&app, entity)
            .state_eci(fixed_epoch())
            .unwrap()
    };

    let (range_sgp4, speed_sgp4) = pair_geometry(&state(debris), &state(root));
    let (range_kepler, speed_kepler) = pair_geometry(&kepler_target, &kepler_chaser);
    println!(
        "Q13 pair at the fixed epoch: SGP4 range {range_sgp4} m speed {speed_sgp4} m/s | \
         Kepler range {range_kepler} m speed {speed_kepler} m/s | \
         |d range| {} m |d speed| {} m/s",
        (range_sgp4 - range_kepler).abs(),
        (speed_sgp4 - speed_kepler).abs()
    );
    assert_close(range_sgp4, range_kepler, 5.0, "pair range, SGP4 vs Kepler");
    assert_close(speed_sgp4, speed_kepler, 0.05, "pair speed, SGP4 vs Kepler");
}

#[test]
fn catalog_pick_edited_falls_back_to_kepler() {
    // F-27: fixed in P4. A slider edit clears `gp` (app side); the sim then
    // starts the body from the edited elements at their epoch.
    let (mut target, chaser) = q13_picks();
    target.elements.mean_anomaly_rad += 1e-4;
    target.gp = None;
    let edited = target.elements;
    let chaser_gp = chaser.gp.clone().unwrap();
    let (mut app, debris, root) = spawn_pair(target, chaser);

    match app.world().get::<Orbit>(debris) {
        Some(Orbit::FromElementsAt { elements, epoch }) => {
            assert_eq!(*elements, edited.to_vec6());
            assert_eq!(*epoch, edited.epoch);
        }
        other => panic!("edited pick: expected FromElementsAt, got {other:?}"),
    }
    match app.world().get::<Orbit>(root) {
        Some(Orbit::FromGp(gp)) => assert_eq!(*gp, chaser_gp),
        other => panic!("unedited pick: expected FromGp, got {other:?}"),
    }

    app.update();
    // The edited elements at their epoch (the start epoch here), seeded as
    // `init_orbitals` seeds every two-body start.
    let expected = KeplerianPropagator::from_eci(
        fixed_epoch(),
        kepler_eci(edited.to_vec6(), edited.epoch),
        1.0,
    )
    .state_eci(fixed_epoch())
    .unwrap();
    assert_eq!(
        propagator_of(&app, debris)
            .state_eci(fixed_epoch())
            .unwrap()
            .map(f64::to_bits),
        expected.map(f64::to_bits),
        "edited pick start state"
    );
}
