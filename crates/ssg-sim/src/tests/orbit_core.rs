//! Tests for the rest of `orbit/` (P5 S5): `init_orbitals` (the SGP4 failure path),
//! `cache_eci_states`, the selection types and the LVLH frame's edge cases, plus
//! property-style element-conversion tests (A-4).

use bevy::ecs::system::RunSystemOnce;
use bevy::math::DVec3;
use bevy::prelude::*;
use brahe::eop::{StaticEOPProvider, set_global_eop_provider};
use brahe::utils::DOrbitStateProvider;
use brahe::{AngleFormat, GM_EARTH, KeplerianPropagator};
use nalgebra::Vector6;

use crate::orbit::catalog::{GpElements, gp_record_to_elements};
use crate::orbit::lvlh::LvlhFrame;
use crate::orbit::selection::EditableOrbitalElements;
use crate::orbit::{
    ISS_ORBIT, Orbit, Orbital, cache::OrbitalCache, cache_eci_states, init_orbitals,
};
use crate::recording::inputs::SimInputLog;
use crate::telemetry::{LogEvent, LogLevel};
use crate::test_support::{
    Lcg, LogTap, assert_close, assert_close_vec6, fixed_epoch, kepler_eci, q13, test_app,
};
use crate::time::WorldTime;

// ---------------------------------------------------------------------------
// init_orbitals
// ---------------------------------------------------------------------------

fn init_app() -> App {
    // What `OrbitPlugin` sets up for SGP4's TEME -> GCRF rotation: zero EOP.
    set_global_eop_provider(StaticEOPProvider::from_zero());
    let mut app = App::new();
    app.add_message::<LogEvent>()
        .init_resource::<SimInputLog>()
        .insert_resource(WorldTime::starting_at(fixed_epoch()));
    app
}

fn run_init(app: &mut App) -> Vec<(LogLevel, &'static str, String)> {
    let mut tap = LogTap::new(app.world());
    app.world_mut()
        .run_system_once(init_orbitals)
        .expect("init_orbitals failed to run");
    tap.drain(app.world())
}

fn hst_gp() -> GpElements {
    gp_record_to_elements(&q13::fixture_record(q13::HST_NORAD_ID), fixed_epoch())
        .expect("HST record")
        .1
}

#[test]
fn init_orbitals_builds_a_propagator_and_consumes_the_orbit_component() {
    let mut app = init_app();
    let from_elements = app.world_mut().spawn(Orbit::FromElements(ISS_ORBIT)).id();
    let from_gp = app.world_mut().spawn(Orbit::FromGp(hst_gp())).id();

    let logs = run_init(&mut app);

    assert!(logs.is_empty(), "no log line on success: {logs:?}");
    for entity in [from_elements, from_gp] {
        assert!(
            app.world().get::<Orbit>(entity).is_none(),
            "{entity:?}: the init component is removed"
        );
        assert!(
            app.world()
                .get::<Orbital>(entity)
                .is_some_and(|o| o.propagator.is_some()),
            "{entity:?} has a propagator"
        );
    }
    // The Kepler body starts at the start epoch with the elements' state.
    let state = app
        .world()
        .get::<Orbital>(from_elements)
        .and_then(|o| o.propagator.as_ref())
        .unwrap()
        .state_eci(fixed_epoch())
        .unwrap();
    assert_close_vec6(
        &state,
        &kepler_eci(ISS_ORBIT, fixed_epoch()),
        1e-6,
        "ISS state at the start epoch",
    );
    // The catalog body is a low-Earth-orbit state (SGP4 at the start epoch).
    let hst = app
        .world()
        .get::<Orbital>(from_gp)
        .and_then(|o| o.propagator.as_ref())
        .unwrap()
        .state_eci(fixed_epoch())
        .unwrap();
    let radius = DVec3::new(hst[0], hst[1], hst[2]).length();
    assert!((6.7e6..7.1e6).contains(&radius), "HST radius {radius} m");
}

#[test]
fn init_orbitals_logs_an_error_and_leaves_no_propagator_when_sgp4_fails() {
    let mut app = init_app();
    // A record whose epoch string brahe cannot parse.
    let mut gp = hst_gp();
    gp.epoch_str = "not an epoch".to_string();
    let body = app.world_mut().spawn(Orbit::FromGp(gp)).id();

    let logs = run_init(&mut app);

    assert_eq!(logs.len(), 1, "one log line: {logs:?}");
    assert_eq!(logs[0].0, LogLevel::Error);
    assert_eq!(logs[0].1, "propagation");
    assert!(
        logs[0].2.starts_with("SGP4 failed for NORAD 20580: "),
        "message: {}",
        logs[0].2
    );
    assert!(
        logs[0].2.len() > "SGP4 failed for NORAD 20580: ".len(),
        "the message carries brahe's error"
    );
    assert!(app.world().get::<Orbit>(body).is_none(), "still consumed");
    assert!(
        app.world()
            .get::<Orbital>(body)
            .is_some_and(|o| o.propagator.is_none()),
        "no propagator for the failed body"
    );

    // A record without an epoch has an empty epoch string and fails the same way.
    let mut gp = hst_gp();
    gp.epoch_str = String::new();
    let body = app.world_mut().spawn(Orbit::FromGp(gp)).id();
    let logs = run_init(&mut app);
    assert_eq!(logs.len(), 1, "{logs:?}");
    assert!(logs[0].2.starts_with("SGP4 failed for NORAD 20580: "));
    assert!(
        app.world()
            .get::<Orbital>(body)
            .unwrap()
            .propagator
            .is_none()
    );
}

#[test]
fn init_orbitals_only_initialises_newly_added_orbits() {
    let mut app = init_app();
    let first = app.world_mut().spawn(Orbit::FromElements(ISS_ORBIT)).id();
    run_init(&mut app);
    let before = app
        .world()
        .get::<Orbital>(first)
        .unwrap()
        .propagator
        .as_ref()
        .unwrap()
        .state_eci(fixed_epoch())
        .unwrap();

    // A second body, later: the first keeps its propagator untouched.
    let second = app.world_mut().spawn(Orbit::FromElements(ISS_ORBIT)).id();
    run_init(&mut app);
    assert!(
        app.world()
            .get::<Orbital>(second)
            .is_some_and(|o| o.propagator.is_some())
    );
    let after = app
        .world()
        .get::<Orbital>(first)
        .unwrap()
        .propagator
        .as_ref()
        .unwrap()
        .state_eci(fixed_epoch())
        .unwrap();
    assert_eq!(before, after);
}

// ---------------------------------------------------------------------------
// cache_eci_states
// ---------------------------------------------------------------------------

#[test]
fn cache_eci_states_stores_each_propagators_state_at_the_sim_epoch() {
    let mut app = test_app();
    app.init_resource::<OrbitalCache>();
    let propagator =
        KeplerianPropagator::from_keplerian(fixed_epoch(), ISS_ORBIT, AngleFormat::Radians, 1.0);
    let moving = app
        .world_mut()
        .spawn(Orbital {
            propagator: Some(propagator.clone()),
            ..default()
        })
        .id();
    let without = app.world_mut().spawn(Orbital::default()).id();
    // A stale entry for an entity that no longer has an orbital.
    let stale = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<OrbitalCache>()
        .eci_states
        .insert(stale, Vector6::repeat(1.0));
    app.world_mut().insert_resource(WorldTime {
        epoch: fixed_epoch() + 600.0,
        ..WorldTime::starting_at(fixed_epoch())
    });

    app.world_mut()
        .run_system_once(cache_eci_states)
        .expect("cache_eci_states failed to run");

    let cache = app.world().resource::<OrbitalCache>();
    let expected = propagator.state_eci(fixed_epoch() + 600.0).unwrap();
    assert_eq!(cache.eci_states.get(&moving), Some(&expected));
    assert!(
        !cache.eci_states.contains_key(&without),
        "no propagator, no state"
    );
    assert!(
        !cache.eci_states.contains_key(&stale),
        "the map is rebuilt, not merged"
    );
    assert_eq!(cache.eci_states.len(), 1);
    // The state moved from the start epoch's.
    assert!(
        (expected - kepler_eci(ISS_ORBIT, fixed_epoch())).norm() > 1_000.0,
        "600 s of orbit is kilometres"
    );
}

// ---------------------------------------------------------------------------
// EditableOrbitalElements
// ---------------------------------------------------------------------------

#[test]
fn editable_elements_from_gp_are_the_records_kepler_elements_at_its_epoch() {
    let gp = hst_gp();
    let elements = EditableOrbitalElements::from_gp(&gp);
    let kepler = gp.kepler_elements_rad();
    assert_eq!(elements.semi_major_axis_m, kepler[0]);
    assert_eq!(elements.eccentricity, kepler[1]);
    assert_eq!(elements.inclination_rad, kepler[2]);
    assert_eq!(elements.raan_rad, kepler[3]);
    assert_eq!(elements.arg_perigee_rad, kepler[4]);
    assert_eq!(elements.mean_anomaly_rad, kepler[5]);
    assert_eq!(elements.epoch, gp.epoch);
}

#[test]
fn editable_elements_to_vec6_orders_the_six_elements() {
    let elements = EditableOrbitalElements {
        semi_major_axis_m: 1.0,
        eccentricity: 2.0,
        inclination_rad: 3.0,
        raan_rad: 4.0,
        arg_perigee_rad: 5.0,
        mean_anomaly_rad: 6.0,
        epoch: fixed_epoch(),
    };
    assert_eq!(
        elements.to_vec6(),
        Vector6::new(1.0, 2.0, 3.0, 4.0, 5.0, 6.0)
    );
}

fn random_gp(rng: &mut Lcg) -> GpElements {
    let json = format!(
        r#"{{"EPOCH": "2026-04-03T16:03:47.002464", "MEAN_MOTION": {}, "ECCENTRICITY": {},
 "INCLINATION": {}, "RA_OF_ASC_NODE": {}, "ARG_OF_PERICENTER": {}, "MEAN_ANOMALY": {},
 "NORAD_CAT_ID": 1}}"#,
        rng.range(0.5, 16.5),
        rng.range(0.0, 0.9),
        rng.range(0.0, 180.0),
        rng.range(0.0, 360.0),
        rng.range(0.0, 360.0),
        rng.range(0.0, 360.0),
    );
    let record = serde_json::from_str(&json).expect("record");
    gp_record_to_elements(&record, fixed_epoch())
        .expect("record converts")
        .1
}

#[test]
fn element_conversions_round_trip_for_generated_records() {
    // Property-style (A-4): a record's elements through `from_gp` and `to_vec6`
    // are the Kepler elements, and give back the record's mean motion and angles.
    const SEED: u64 = 0xE1E_0001;
    let mut rng = Lcg::new(SEED);
    for case in 0..300 {
        let gp = random_gp(&mut rng);
        let ctx = format!("seed {SEED:#x} case {case}: {gp:?}");
        let vec6 = EditableOrbitalElements::from_gp(&gp).to_vec6();
        assert_eq!(vec6, gp.kepler_elements_rad(), "{ctx}");

        let n = (GM_EARTH / vec6[0].powi(3)).sqrt() * 86_400.0 / std::f64::consts::TAU;
        assert!(
            (n / gp.mean_motion_rev_per_day - 1.0).abs() < 1e-12,
            "mean motion {n} vs {}: {ctx}",
            gp.mean_motion_rev_per_day
        );
        for (radians, degrees, name) in [
            (vec6[2], gp.inclination_deg, "inclination"),
            (vec6[3], gp.raan_deg, "raan"),
            (vec6[4], gp.arg_perigee_deg, "argument of perigee"),
            (vec6[5], gp.mean_anomaly_deg, "mean anomaly"),
        ] {
            assert_close(
                radians.to_degrees(),
                degrees,
                1e-12 * degrees.abs().max(1.0),
                &format!("{name}: {ctx}"),
            );
        }
        assert_eq!(vec6[1], gp.eccentricity, "{ctx}");
    }
}

// ---------------------------------------------------------------------------
// LvlhFrame
// ---------------------------------------------------------------------------

#[test]
fn lvlh_frame_does_not_exist_without_a_position_or_an_angular_momentum() {
    let at_origin = Vector6::new(0.0, 0.0, 0.0, 1.0, 2.0, 3.0);
    assert!(LvlhFrame::from_eci(&at_origin).is_none(), "r = 0");
    let at_rest = Vector6::new(7.0e6, 0.0, 0.0, 0.0, 0.0, 0.0);
    assert!(LvlhFrame::from_eci(&at_rest).is_none(), "v = 0");
    let radial_fall = Vector6::new(7.0e6, 0.0, 0.0, -100.0, 0.0, 0.0);
    assert!(
        LvlhFrame::from_eci(&radial_fall).is_none(),
        "r parallel to v"
    );
    let circular = Vector6::new(7.0e6, 0.0, 0.0, 0.0, 7_500.0, 0.0);
    assert!(LvlhFrame::from_eci(&circular).is_some());
}

#[test]
fn lvlh_mean_motion_of_a_circular_orbit_is_sqrt_mu_over_a_cubed() {
    let a = 7.0e6;
    let v = (GM_EARTH / a).sqrt();
    let frame = LvlhFrame::from_eci(&Vector6::new(a, 0.0, 0.0, 0.0, v, 0.0)).unwrap();
    assert_close(
        frame.mean_motion(),
        (GM_EARTH / (a * a * a)).sqrt(),
        1e-15,
        "mean motion",
    );
    assert_eq!(frame.mean_motion(), frame.omega.length());
    // Axes of this particular orbit: r along +X, v along +Y, normal +Z.
    assert!((frame.radial - DVec3::X).length() < 1e-15);
    assert!((frame.along - DVec3::Y).length() < 1e-15);
    assert!((frame.cross - DVec3::Z).length() < 1e-15);
}

#[test]
fn lvlh_frames_of_generated_orbits_are_consistent() {
    // Property-style (A-4): for random orbits and vectors, `to_eci` inverts
    // `to_lvlh` (1e-12 relative), a body co-rotating with the frame has zero
    // relative velocity, and the mean motion is |omega|.
    const SEED: u64 = 0x17A_0001;
    let mut rng = Lcg::new(SEED);
    for case in 0..300 {
        let elements = Vector6::new(
            rng.range(6.6e6, 4.0e7),
            rng.range(0.0, 0.8),
            rng.range(0.0, 3.1),
            rng.range(0.0, std::f64::consts::TAU),
            rng.range(0.0, std::f64::consts::TAU),
            rng.range(0.0, std::f64::consts::TAU),
        );
        let state = kepler_eci(elements, fixed_epoch());
        let ctx = format!("seed {SEED:#x} case {case}: {elements:?}");
        let frame = LvlhFrame::from_eci(&state).unwrap_or_else(|| panic!("no frame: {ctx}"));

        let eci = DVec3::new(
            rng.range(-1e4, 1e4),
            rng.range(-1e4, 1e4),
            rng.range(-1e4, 1e4),
        );
        let back = frame.to_eci(frame.to_lvlh(eci));
        assert!(
            (back - eci).length() <= 1e-12 * eci.length(),
            "to_eci(to_lvlh(v)) = {back:?} vs {eci:?}: {ctx}"
        );
        let lvlh = DVec3::new(
            rng.range(-1e4, 1e4),
            rng.range(-1e4, 1e4),
            rng.range(-1e4, 1e4),
        );
        let round = frame.to_lvlh(frame.to_eci(lvlh));
        assert!(
            (round - lvlh).length() <= 1e-12 * lvlh.length(),
            "to_lvlh(to_eci(w)) = {round:?} vs {lvlh:?}: {ctx}"
        );

        let rel_r = eci;
        let corotating = frame.omega.cross(rel_r);
        let in_frame = frame.relative_velocity_in_frame(rel_r, corotating);
        assert!(
            in_frame.length() <= 1e-9 * corotating.length().max(1e-3),
            "co-rotating body moves {in_frame:?} in the frame: {ctx}"
        );
        assert_eq!(frame.mean_motion(), frame.omega.length(), "{ctx}");
        assert!(
            (frame.omega.dot(frame.cross) - frame.omega.length()).abs()
                <= 1e-12 * frame.omega.length(),
            "omega is along the orbit normal: {ctx}"
        );
    }
}
