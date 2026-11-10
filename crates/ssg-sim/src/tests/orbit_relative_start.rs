//! P9 S5: the V-bar/R-bar start (`relative_to_target`, Q10 row 15, F-47). The
//! conversion on the target's LVLH frame, the elements round trip, the CW
//! checks, near-circular and equatorial targets (F-69), the start guard (operator
//! decision M1-a) and the M1 run.
//! Numbers: `plans/stage-9/results/s5-m1.md`.

use avian3d::prelude::*;
use bevy::math::DVec3;
use bevy::prelude::*;
use brahe::Epoch;
use nalgebra::Vector6;

use crate::capture::plan::CapturePlanLibrary;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::kepler::{TwoBody, state_to_elements};
use crate::orbit::lvlh::{LvlhFrame, cw_solution, relative_offset_lvlh, relative_start};
use crate::orbit::selection::{
    EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState,
    RELATIVE_START_TOLERANCE_M, SelectedOrbitalObject, relative_chaser_elements,
};
use crate::orbit::{Orbit, start_state_eci, use_zero_eop};
use crate::physics_bubble::body_eci_state;
use crate::project::SelectedProject;
use crate::project::epoch::format_utc;
use crate::project::manifest::{
    ManifestBody, ManifestOrbit, ProjectManifest, RelativePreset, RelativeToTarget,
};
use crate::test_support::{
    current_step, enter_sim, fixed_epoch, load_example_plans, q13, start_running, test_app, tick,
};
use crate::time::{PHYS_DT, WorldTime};

const PRESETS: [RelativePreset; 2] = [RelativePreset::VBar, RelativePreset::RBar];
const OFFSETS: [f64; 4] = [10.0, -10.0, 1_000.0, -1_000.0];

fn hst() -> SelectedOrbitalObject {
    q13::catalog_pick("HST", q13::HST_NORAD_ID)
}

/// A circular 400 km orbit (e = 0) given as elements at the start epoch.
fn circular() -> SelectedOrbitalObject {
    SelectedOrbitalObject {
        source: OrbitalSelectionSource::Custom {
            label: "circular".to_string(),
        },
        elements: EditableOrbitalElements {
            semi_major_axis_m: 6_778_137.0,
            eccentricity: 0.0,
            inclination_rad: 0.9,
            raan_rad: 1.0,
            arg_perigee_rad: 0.0,
            mean_anomaly_rad: 0.5,
            epoch: fixed_epoch(),
        },
        gp: None,
        mass_kg: None,
        inertia_kg_m2: None,
    }
}

/// The target's start state, as the sim's propagator gives it.
fn start_state(target: &SelectedOrbitalObject) -> Vector6<f64> {
    use_zero_eop();
    start_state_eci(&target.start_orbit(), fixed_epoch()).expect("start state")
}

fn v3(s: &Vector6<f64>, i: usize) -> DVec3 {
    DVec3::new(s[i], s[i + 1], s[i + 2])
}

/// `(LVLH position, LVLH velocity in the rotating frame)` of `chaser` relative to `target`.
fn relative_lvlh(target: &Vector6<f64>, chaser: &Vector6<f64>) -> (DVec3, DVec3) {
    let frame = LvlhFrame::from_eci(target).expect("target frame");
    let (rel_r, rel_v) = (v3(chaser, 0) - v3(target, 0), v3(chaser, 3) - v3(target, 3));
    (
        frame.to_lvlh(rel_r),
        frame.to_lvlh(frame.relative_velocity_in_frame(rel_r, rel_v)),
    )
}

/// The chaser's start state from elements at the start epoch, as the sim gets it.
fn elements_start(elements: Vector6<f64>) -> Vector6<f64> {
    start_state_eci(
        &Orbit::FromElementsAt {
            elements,
            epoch: fixed_epoch(),
        },
        fixed_epoch(),
    )
    .expect("kepler")
}

/// Both start states Kepler-propagated `t` seconds; the chaser relative to the target.
fn propagated_lvlh(target: &Vector6<f64>, chaser: &Vector6<f64>, t: f64) -> (DVec3, DVec3) {
    let at: Epoch = fixed_epoch() + t;
    let state = |s: &Vector6<f64>| {
        TwoBody::from_eci(fixed_epoch(), *s)
            .state_eci(at)
            .expect("kepler")
    };
    relative_lvlh(&state(target), &state(chaser))
}

// (1) The conversion is exact on the LVLH frame.
#[test]
fn relative_start_is_at_rest_on_its_point() {
    for (name, target) in [("HST", hst()), ("circular", circular())] {
        let target = start_state(&target);
        for preset in PRESETS {
            for offset in OFFSETS {
                let chaser = relative_start(&target, preset, offset).expect("frame");
                let (p, v) = relative_lvlh(&target, &chaser);
                let miss = (p - relative_offset_lvlh(preset, offset)).length();
                assert!(
                    miss < 1e-6,
                    "{name} {preset:?} {offset}: position {miss:e} m"
                );
                assert!(
                    v.length() < 1e-9,
                    "{name} {preset:?} {offset}: velocity {:e} m/s",
                    v.length()
                );
            }
        }
    }
}

// (2) The elements round trip, every preset and offset on both targets (F-69: the
// conversion does not round a small eccentricity).
#[test]
fn elements_round_trip_within_1e6_m() {
    let hst_state = start_state(&hst());
    let circular_state = start_state(&circular());
    let mut cases: Vec<(&str, &Vector6<f64>, RelativePreset, f64)> = Vec::new();
    for preset in PRESETS {
        for offset in OFFSETS {
            cases.push(("HST", &hst_state, preset, offset));
        }
    }
    for preset in PRESETS {
        for offset in OFFSETS {
            cases.push(("circular", &circular_state, preset, offset));
        }
    }
    for (name, target, preset, offset) in cases {
        let chaser = relative_start(target, preset, offset).expect("frame");
        let back = elements_start(state_to_elements(chaser));
        let (dp, dv) = (
            (v3(&back, 0) - v3(&chaser, 0)).length(),
            (v3(&back, 3) - v3(&chaser, 3)).length(),
        );
        assert!(dp < 1e-6, "{name} {preset:?} {offset}: position {dp:e} m");
        assert!(dv < 1e-8, "{name} {preset:?} {offset}: velocity {dv:e} m/s");
    }
}

// (3) V-bar on a circular orbit is a CW equilibrium.
#[test]
fn v_bar_on_a_circular_orbit_stays_put_for_one_orbit() {
    let target = circular();
    let target_state = start_state(&target);
    let offset = 30.0;
    let elements =
        relative_chaser_elements(&target, fixed_epoch(), RelativePreset::VBar, offset).unwrap();
    let chaser = elements_start(elements.to_vec6());
    let period = std::f64::consts::TAU / LvlhFrame::from_eci(&target_state).unwrap().mean_motion();
    let (p, _) = propagated_lvlh(&target_state, &chaser, period);
    let drift = (p - relative_offset_lvlh(RelativePreset::VBar, offset)).length();
    assert!(
        drift < 1e-3 * offset,
        "drift over one orbit {drift:e} m ({p:?})"
    );
}

// (4) R-bar drifts along-track as CW says.
#[test]
fn r_bar_drift_matches_cw_over_a_quarter_orbit() {
    let target = circular();
    let target_state = start_state(&target);
    let offset = 1_000.0;
    let elements =
        relative_chaser_elements(&target, fixed_epoch(), RelativePreset::RBar, offset).unwrap();
    let chaser = elements_start(elements.to_vec6());
    let n = LvlhFrame::from_eci(&target_state).unwrap().mean_motion();
    let t = std::f64::consts::FRAC_PI_2 / n;
    let (p, _) = propagated_lvlh(&target_state, &chaser, t);
    let (cw, _) = cw_solution(n, DVec3::new(offset, 0.0, 0.0), DVec3::ZERO, t);
    assert!(cw.y < -3_000.0, "CW drifts along-track: {cw:?}");
    let miss = (p - cw).length();
    assert!(
        miss < 1e-2 * offset,
        "quarter orbit: sim {p:?}, CW {cw:?}, {miss:e} m"
    );
}

/// A target given as elements at the start epoch: a = 6,918 km, eccentricity `e`,
/// inclination `inclination_rad` (the F-69 cases).
fn target_with(e: f64, inclination_rad: f64) -> SelectedOrbitalObject {
    SelectedOrbitalObject {
        source: OrbitalSelectionSource::Custom {
            label: format!("e {e:e}, i {inclination_rad}"),
        },
        elements: EditableOrbitalElements {
            semi_major_axis_m: 6_918_000.0,
            eccentricity: e,
            inclination_rad,
            raan_rad: 4.0063,
            arg_perigee_rad: 0.9066,
            mean_anomaly_rad: 0.5,
            epoch: fixed_epoch(),
        },
        gp: None,
        mass_kg: None,
        inertia_kg_m2: None,
    }
}

// F-69: before P11, brahe 1.3.4 rounded an eccentricity below about 1e-4 to 0, and
// an R-bar 30 m start from an e = 0 target was refused (90 m off). Now every start
// from a near-circular or equatorial target lands on its point.
#[test]
fn starts_from_near_circular_and_equatorial_targets_land_on_their_point() {
    let mut targets: Vec<SelectedOrbitalObject> = [0.0, 1e-9, 1e-5, 9e-5]
        .into_iter()
        .map(|e| target_with(e, 0.4969))
        .collect();
    targets.push(target_with(1e-5, 0.0));
    for target in &targets {
        let name = match &target.source {
            OrbitalSelectionSource::Custom { label } => label.clone(),
            other => panic!("{other:?}"),
        };
        let target_state = start_state(target);
        for preset in PRESETS {
            for offset in [30.0, 1_000.0] {
                let elements = relative_chaser_elements(target, fixed_epoch(), preset, offset)
                    .unwrap_or_else(|e| panic!("{name} {preset:?} {offset}: {e}"));
                let chaser = elements_start(elements.to_vec6());
                let (p, _) = relative_lvlh(&target_state, &chaser);
                let miss = (p - relative_offset_lvlh(preset, offset)).length();
                assert!(miss <= 1e-6, "{name} {preset:?} {offset}: {miss:e} m");
            }
        }
    }
}

// M1-a: the 1 mm guard stays, now against a degenerate target: a target whose
// state is not a number gives the guard's error.
#[test]
fn a_degenerate_target_is_refused_by_the_start_guard() {
    let mut target = circular();
    target.elements.semi_major_axis_m = f64::NAN;
    let refused =
        relative_chaser_elements(&target, fixed_epoch(), RelativePreset::RBar, 30.0).unwrap_err();
    assert_eq!(
        refused,
        "the chaser would start NaN m from its point; choose another offset or target"
    );
    assert_eq!(RELATIVE_START_TOLERANCE_M, 1e-3);
}

/// The M1 project: the HST record as the target, the chaser 30 m ahead on V-bar.
fn m1_manifest() -> ProjectManifest {
    let start = format_utc(fixed_epoch());
    ProjectManifest {
        schema_version: 1,
        name: "M1 V-bar 30 m".to_string(),
        start_epoch_utc: start,
        active_plan: Some(q13::PLAN_ID.to_string()),
        target: ManifestBody {
            label: "HST".to_string(),
            orbit: ManifestOrbit::Gp(Box::new(q13::fixture_record(q13::HST_NORAD_ID))),
            source: None,
            mass_kg: None,
            inertia_kg_m2: None,
        },
        chaser: ManifestBody {
            label: "Chaser".to_string(),
            orbit: ManifestOrbit::RelativeToTarget(RelativeToTarget {
                preset: RelativePreset::VBar,
                offset_m: 30.0,
            }),
            source: None,
            mass_kg: None,
            inertia_kg_m2: None,
        },
        built_with: None,
        recording: None,
    }
}

// (6) M1 as a test: the sim, no capture, 60 s; bounds of M1-b.
#[test]
fn m1_v_bar_30_m_starts_on_its_point_and_follows_cw() {
    let inputs = m1_manifest().sim_inputs().expect("valid");
    assert_eq!(inputs.start_epoch, fixed_epoch());
    let mut app = test_app();
    load_example_plans(&mut app);
    app.world_mut().resource_mut::<SelectedProject>().project_id = Some(q13::PLAN_ID.into());
    *app.world_mut().resource_mut::<OrbitalSelectionState>() = OrbitalSelectionState {
        target: Some(inputs.target),
        chaser: Some(inputs.chaser),
        chaser_relative: inputs.chaser_relative,
    };
    enter_sim(&mut app);
    start_running(&mut app);
    let tether = app.world().resource::<CapturePlanLibrary>().plans[q13::PLAN_ID]
        .tether
        .clone();
    let (debris, root) = {
        let cache = app.world().resource::<OrbitalCache>();
        (cache.debris["Satellite1"], cache.tethers[&tether][0])
    };
    let lvlh = |app: &App| {
        let world = app.world();
        let cache = world.resource::<OrbitalCache>();
        let eci = |entity: Entity| {
            body_eci_state(
                cache,
                entity,
                world.get::<Position>(entity)?.0,
                world.get::<LinearVelocity>(entity)?.0,
                world.get::<RigidBodyDisabled>(entity).is_some(),
            )
        };
        Some(relative_lvlh(&eci(debris)?, &eci(root)?))
    };

    let mut first = None;
    while first.is_none() {
        tick(&mut app);
        first = lvlh(&app).map(|(p, _)| (current_step(&app), p));
    }
    let (step, p) = first.unwrap();
    assert!(step <= 3, "first sample at step {step}");
    assert!(
        (p.length() - 30.0).abs() <= 1e-6,
        "step {step}: range {} m",
        p.length()
    );

    let n = LvlhFrame::from_eci(&start_state(&hst()))
        .unwrap()
        .mean_motion();
    let end = (60.0 / PHYS_DT).round() as u64;
    while current_step(&app) < end {
        tick(&mut app);
    }
    let world_time = app.world().resource::<WorldTime>();
    let t = world_time.epoch - world_time.start_epoch;
    let (p, _) = lvlh(&app).unwrap();
    let (cw, _) = cw_solution(n, DVec3::new(0.0, 30.0, 0.0), DVec3::ZERO, t);
    assert!((p - cw).length() <= 2e-3, "t = {t} s: sim {p:?}, CW {cw:?}");
}
