//! Per-phase controller gains (P9 S4, Q10 row 17, E2 (a)): `phases[].controller`
//! parameterises the P7.5 law's eight constants. Absent or at the defaults it
//! compiles to the constants (the goldens are the CLI's `controller_defaults`
//! test); each gain moves the term it names; validation; and one harness check on
//! the example plan's terminal phase (ignored; run locally).

use avian3d::prelude::{LinearVelocity, Position};
use bevy::math::DVec3;
use bevy::prelude::*;
use serde_json::json;

use crate::capture::gains::{ControllerGains, PhaseController};
use crate::capture::plan::{CapturePlan, compile_capture_plan, validate_capture_plan};
use crate::capture::{
    BRANCH_DEAD_BAND, CaptureForce, CaptureSphereRadius, FORCE_RAMP_S, INNER_EDGE_FRACTION,
    PLANE_HOLD_BAND_M, RADIAL_RATE_BAND_MPS, RADIUS_HOLD_BAND_M, SPACING_BAND_M,
    SPACING_RATE_BAND_MPS, circle_law, tail_lag_weight,
};
use crate::test_support::{capture_rig, insert_plan_json, repo_root, rig_step, test_app};
use crate::time::PHYS_DT;

/// The eight keys at the constants they default to.
pub(crate) fn explicit_defaults() -> serde_json::Value {
    json!({
        "radius_hold_band_m": RADIUS_HOLD_BAND_M,
        "radial_rate_band_mps": RADIAL_RATE_BAND_MPS,
        "spacing_band_m": SPACING_BAND_M,
        "spacing_rate_band_mps": SPACING_RATE_BAND_MPS,
        "plane_hold_band_m": PLANE_HOLD_BAND_M,
        "inner_edge_fraction": INNER_EDGE_FRACTION,
        "dead_band_fraction": BRANCH_DEAD_BAND,
        "force_ramp_s": FORCE_RAMP_S,
    })
}

fn example_plan_json() -> serde_json::Value {
    let text =
        std::fs::read_to_string(repo_root().join("assets/example_capture_plans/example_plan.json"))
            .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn plan(value: &serde_json::Value) -> CapturePlan {
    serde_json::from_value(value.clone()).expect("a v1 plan")
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

#[test]
fn the_default_gains_are_the_controllers_constants() {
    let gains = ControllerGains::default();
    assert_eq!(gains, PhaseController::default().gains());
    assert_eq!(gains.radius_hold_band_m, 2.0);
    assert_eq!(gains.radial_rate_band_mps, 0.1);
    assert_eq!(gains.spacing_band_m, 3.0);
    assert_eq!(gains.spacing_rate_band_mps, 0.3);
    assert_eq!(gains.plane_hold_band_m, 1.0);
    assert_eq!(gains.inner_edge_fraction, 0.8);
    assert_eq!(gains.dead_band_fraction, 0.02);
    assert_eq!(gains.force_ramp_s, 1.0);
}

/// The example plan with every phase's `controller` set to the defaults compiles
/// to the same parameters, phase by phase, as without the key; and the key round
/// trips through the writer only when present.
#[test]
fn explicit_defaults_compile_to_the_same_parameters_as_no_key() {
    let without = example_plan_json();
    let mut with = without.clone();
    for phase in with["phases"].as_array_mut().unwrap() {
        phase["controller"] = explicit_defaults();
    }
    let (a, b) = (
        compile_capture_plan(&plan(&without)),
        compile_capture_plan(&plan(&with)),
    );
    assert_eq!(a.states.len(), 6);
    for (x, y) in a.states.iter().zip(&b.states) {
        assert_eq!(x.parameters, y.parameters, "{}", x.id);
    }
    assert!(validate_capture_plan("d", &plan(&with)).is_empty());
    let written = crate::project::plan_files::plan_to_json(&plan(&with));
    assert_eq!(written.matches("\"controller\"").count(), 6);
    assert!(!crate::project::plan_files::plan_to_json(&plan(&without)).contains("controller"));
}

// ---------------------------------------------------------------------------
// Each gain moves its term
// ---------------------------------------------------------------------------

fn gains(edit: impl FnOnce(&mut ControllerGains)) -> ControllerGains {
    let mut gains = ControllerGains::default();
    edit(&mut gains);
    gains
}

/// The circle law's radial term inside the band: `w_r = error / band`. A body
/// 0.5 m outside its 25 m sphere, at rest, the sphere not shrinking (`w_v` = 0).
#[test]
fn halving_radius_hold_band_m_doubles_the_radial_weight_inside_the_band() {
    let rel_r = DVec3::X * 25.5;
    let radial = |g: &ControllerGains| {
        circle_law(rel_r, DVec3::ZERO, 25.0, 0.0, DVec3::ZERO, false, g).dot(DVec3::X)
    };
    let default = radial(&ControllerGains::default());
    let halved = radial(&gains(|g| g.radius_hold_band_m = 1.0));
    assert_eq!(default, 0.25);
    assert_eq!(halved, 0.5);
}

/// `w_v = v_out / band`: 0.02 m/s outward on the sphere.
#[test]
fn halving_radial_rate_band_mps_doubles_the_rate_weight() {
    let rel_r = DVec3::X * 25.0;
    let outward = -DVec3::X * 0.02;
    let radial = |g: &ControllerGains| {
        circle_law(rel_r, outward, 25.0, 0.0, DVec3::ZERO, false, g).dot(DVec3::X)
    };
    assert!((radial(&ControllerGains::default()) - 0.2).abs() < 1e-12);
    assert!((radial(&gains(|g| g.radial_rate_band_mps = 0.05)) - 0.4).abs() < 1e-12);
}

/// The lag law: the tail 1 m of chord behind its held spacing, both ends at rest
/// on a 50 m sphere: `w = 1 / spacing_band_m`.
#[test]
fn spacing_band_m_scales_the_tail_lag_weight() {
    let (radius, phi) = (50.0, 0.3_f64);
    let root = (-DVec3::X * radius, DVec3::ZERO);
    let tail_out = DVec3::new(phi.cos(), phi.sin(), 0.0) * radius;
    let tail = (-tail_out, DVec3::ZERO);
    let chord = 2.0 * radius * (phi / 2.0).sin();
    let weight =
        |g: &ControllerGains| tail_lag_weight(root, tail, DVec3::Z, radius, chord - 1.0, g);
    assert!((weight(&ControllerGains::default()) - 1.0 / 3.0).abs() < 1e-9);
    assert!((weight(&gains(|g| g.spacing_band_m = 1.5)) - 2.0 / 3.0).abs() < 1e-9);
    // The rate term: the tail 0.03 m/s faster along its tangent, chord at its hold.
    let fast_tail = (-tail_out, DVec3::Z.cross(-tail_out).normalize() * 0.03);
    let rate = |g: &ControllerGains| tail_lag_weight(root, fast_tail, DVec3::Z, radius, chord, g);
    assert!((rate(&ControllerGains::default()) + 0.1).abs() < 1e-9);
    assert!((rate(&gains(|g| g.spacing_rate_band_mps = 0.15)) + 0.2).abs() < 1e-9);
}

/// The branch law's inner edge is `inner_edge_fraction × R`, entered below it by
/// the dead band.
#[test]
fn inner_edge_fraction_moves_the_inner_edge() {
    let inner_at = |range: f64, g: &ControllerGains| {
        let mut state = CaptureForce::default();
        state.update_branches(range, 0.0, 10.0, 1.0, 0.02, g);
        state.inner
    };
    let default = ControllerGains::default();
    let half = gains(|g| g.inner_edge_fraction = 0.5);
    // Default edge 8 m (entered below 7.84 m); at 0.5, 5 m (below 4.9 m).
    assert!(inner_at(7.8, &default));
    assert!(!inner_at(7.8, &half));
    assert!(inner_at(4.8, &half));
    assert!(!inner_at(7.9, &default));
}

/// With `dead_band_fraction` 0 a branch is left as soon as the range crosses its
/// edge; with the default 2 % band it is held.
#[test]
fn dead_band_fraction_zero_removes_the_hysteresis() {
    let after = |g: &ControllerGains, ranges: &[f64]| {
        let mut state = CaptureForce::default();
        for &range in ranges {
            state.update_branches(range, 0.0, 10.0, 1.0, 0.0, g);
        }
        (state.inner, state.outer)
    };
    let default = ControllerGains::default();
    let none = gains(|g| g.dead_band_fraction = 0.0);
    // In at 7.0 m, then 8.1 m: inside 8 × 1.02 = 8.16 m the default holds it.
    assert_eq!(after(&default, &[7.0, 8.1]), (true, false));
    assert_eq!(after(&none, &[7.0, 8.1]), (false, false));
    // Out at 10.5 m, then 9.9 m: above 10 × 0.98 the default holds it.
    assert_eq!(after(&default, &[10.5, 9.9]), (false, true));
    assert_eq!(after(&none, &[10.5, 9.9]), (false, false));
    // And the edges themselves: 10.1 m enters the outer branch only without a band.
    assert_eq!(after(&default, &[10.1]), (false, false));
    assert_eq!(after(&none, &[10.1]), (false, true));
}

/// The ramp reaches `max_force_n` after `force_ramp_s`: 32 steps at 0.5 s, 64 by
/// default.
#[test]
fn the_ramp_reaches_max_force_after_force_ramp_s() {
    for (ramp_s, steps) in [(0.5, 32), (1.0, 64), (2.0, 128)] {
        let plan = json!({"schema_version": 1, "name": "ramp", "tether": "T", "phases": [
            {"id": "a", "max_velocity_mps": 1.0, "max_force_n": 10.0,
             "controller": {"force_ramp_s": ramp_s}}]});
        let mut app = test_app();
        insert_plan_json(&mut app, "ramp", &plan.to_string());
        let rig = capture_rig(&mut app, "ramp");
        let ramp = |app: &App| app.world().get::<CaptureForce>(rig.root).unwrap().ramp_n;
        for step in 1..=steps {
            app.world_mut()
                .entity_mut(rig.capture)
                .insert((Position(DVec3::X * 50.0), LinearVelocity::ZERO));
            rig_step(&mut app, &rig);
            if step == steps - 1 {
                assert!(ramp(&app) < 10.0, "{ramp_s} s: {} at {step}", ramp(&app));
            }
        }
        assert_eq!(ramp(&app), 10.0, "{ramp_s} s after {steps} steps");
        assert_eq!(steps as f64 * PHYS_DT, ramp_s);
    }
}

/// `hold_plane`: the component toward the plane is `h / plane_hold_band_m`, so a
/// wider band gives a smaller pull at the same height (the root 0.5 m above).
#[test]
fn plane_hold_band_m_scales_the_pull_toward_the_plane() {
    let pull = |band: f64| {
        let plan = json!({"schema_version": 1, "name": "plane", "tether": "T", "phases": [
            {"id": "a", "max_velocity_mps": 1.0, "max_force_n": 10.0, "hold_plane": true,
             "actuation": "hold", "controller": {"plane_hold_band_m": band}}]});
        let mut app = test_app();
        insert_plan_json(&mut app, "plane", &plan.to_string());
        let rig = capture_rig(&mut app, "plane");
        app.world_mut()
            .insert_resource(CaptureSphereRadius { radius: 25.0 });
        // The target 0.5 m below the root: the root is 0.5 m above the plane.
        app.world_mut()
            .entity_mut(rig.capture)
            .insert((Position(DVec3::new(10.0, 0.0, -0.5)), LinearVelocity::ZERO));
        app.world_mut()
            .entity_mut(rig.root)
            .insert((Position(DVec3::ZERO), LinearVelocity::ZERO));
        rig_step(&mut app, &rig);
        let state = app.world().get::<CaptureForce>(rig.root).unwrap();
        -state.applied.z / state.ramp_n
    };
    assert!((pull(1.0) - 0.5).abs() < 1e-12, "{}", pull(1.0));
    assert!((pull(2.0) - 0.25).abs() < 1e-12, "{}", pull(2.0));
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn controller_errors_of(controller: serde_json::Value) -> Vec<String> {
    let value = json!({"schema_version": 1, "name": "v", "tether": "T", "phases": [
        {"id": "a", "max_velocity_mps": 1.0, "max_force_n": 1.0, "controller": controller}]});
    validate_capture_plan("v", &plan(&value))
}

#[test]
fn controller_gains_are_validated_with_their_pointers() {
    assert!(
        controller_errors_of(json!({})).is_empty(),
        "every key optional"
    );
    assert!(controller_errors_of(explicit_defaults()).is_empty());
    let at = |key: &str, message: &str| format!("[v] /phases/0/controller/{key}: {message}");
    let positive = "must be a finite number greater than 0";
    for key in [
        "radius_hold_band_m",
        "radial_rate_band_mps",
        "spacing_band_m",
        "spacing_rate_band_mps",
        "plane_hold_band_m",
        "force_ramp_s",
    ] {
        for bad in [0.0, -1.0] {
            assert_eq!(
                controller_errors_of(json!({ key: bad })),
                [at(key, positive)],
                "{key} {bad}"
            );
        }
    }
    for bad in [0.0, 1.0, 1.5, -0.1] {
        assert_eq!(
            controller_errors_of(json!({ "inner_edge_fraction": bad })),
            [at(
                "inner_edge_fraction",
                "must be greater than 0 and less than 1"
            )]
        );
    }
    assert!(controller_errors_of(json!({ "dead_band_fraction": 0.0 })).is_empty());
    for bad in [0.5, 0.7, -0.01] {
        assert_eq!(
            controller_errors_of(json!({ "dead_band_fraction": bad })),
            [at(
                "dead_band_fraction",
                "must be at least 0 and less than 0.5"
            )]
        );
    }
    // Field order, several at once.
    assert_eq!(
        controller_errors_of(json!({"force_ramp_s": 0.0, "radius_hold_band_m": -2.0})),
        [
            at("radius_hold_band_m", positive),
            at("force_ramp_s", positive)
        ]
    );
    // An unknown key is refused when the file is read.
    let value = json!({"schema_version": 1, "name": "v", "tether": "T", "phases": [
        {"id": "a", "max_velocity_mps": 1.0, "max_force_n": 1.0,
         "controller": {"gain": 1.0}}]});
    assert!(serde_json::from_value::<CapturePlan>(value).is_err());
}

// ---------------------------------------------------------------------------
// The harness check (ignored: two runs to the terminal phase)
// ---------------------------------------------------------------------------

/// In the example plan's terminal phase, 60 s of sim time with the terminal
/// phase's `radius_hold_band_m` at 1.0 against the default 2.0, from the first step
/// the root is within 2 m of its sphere (before it both bands saturate and the runs
/// are the same; from the terminal entry the root is ~16 m off): the mean |radius
/// error| of the root (its range less the sphere radius, every tick) and the
/// root's force reversals. Asserts the direction only: the tighter band holds the
/// radius closer, and neither run reverses as often as `root_orbits_cleanly`
/// allows in a 6,400-tick window (250).
#[test]
#[ignore = "P9 S4 harness check (~2 min): cargo test -p ssg-sim --locked -- --ignored --nocapture radius_hold_band_tightens_the_roots_orbit"]
fn radius_hold_band_tightens_the_roots_orbit() {
    use crate::capture::plan::CapturePlanLibrary;
    use crate::test_support::{q13, tick};
    use crate::tests::tether_dynamics::position;

    let run = |band: f64| -> (f64, u64, (u64, u64)) {
        let mut app = test_app();
        let handles = q13::build(&mut app);
        // Only the terminal phase's gain differs: the runs are identical until its entry.
        let mut value = example_plan_json();
        value["phases"][3]["controller"] = json!({ "radius_hold_band_m": band });
        assert_eq!(value["phases"][3]["id"], "terminal");
        app.world_mut()
            .resource_mut::<CapturePlanLibrary>()
            .insert_plan("example_plan".to_string(), plan(&value));
        let mut ticks = 0u64;
        while q13::sample(&app, &handles).state != "terminal" {
            tick(&mut app);
            ticks += 1;
            assert!(ticks < 260_000, "no terminal phase");
        }
        let entry = q13::sample(&app, &handles).step;
        let radius_error = |app: &App| {
            let range = (position(app, handles.debris) - position(app, handles.root)).length();
            range - app.world().resource::<CaptureSphereRadius>().radius
        };
        // Both bands saturate while the root is 2 m or more off its sphere: the
        // runs are the same until it first comes within 2 m.
        while radius_error(&app).abs() >= RADIUS_HOLD_BAND_M {
            tick(&mut app);
            ticks += 1;
            assert!(ticks < 260_000, "the root never nears its sphere");
        }
        let start = q13::sample(&app, &handles).step;
        let (mut error_sum, mut reversals, mut previous) = (0.0, 0u64, None::<DVec3>);
        let steps = (60.0 / PHYS_DT) as u64;
        for _ in 0..steps {
            tick(&mut app);
            error_sum += radius_error(&app).abs();
            let applied = app
                .world()
                .get::<CaptureForce>(handles.root)
                .unwrap()
                .applied;
            if applied != DVec3::ZERO {
                if previous.is_some_and(|p| applied.dot(p) < 0.0) {
                    reversals += 1;
                }
                previous = Some(applied);
            }
        }
        assert_eq!(
            q13::sample(&app, &handles).state,
            "terminal",
            "60 s stay in terminal"
        );
        (error_sum / steps as f64, reversals, (entry, start))
    };
    let (default_error, default_reversals, steps) = run(2.0);
    let (tight_error, tight_reversals, tight_steps) = run(1.0);
    assert_eq!(
        steps, tight_steps,
        "the same terminal entry and window start"
    );
    let (entry, start) = steps;
    println!(
        "terminal entry at step {entry}; root first within 2 m of its sphere at step \
         {start} ({:.1} s in terminal); window: 60 s = {} ticks from there",
        (start - entry) as f64 * PHYS_DT,
        (60.0 / PHYS_DT) as u64
    );
    println!("| radius_hold_band_m | mean root radius error (m) | root force reversals |");
    println!("|---|---|---|");
    println!("| 2.0 (default) | {default_error:.4} | {default_reversals} |");
    println!("| 1.0 | {tight_error:.4} | {tight_reversals} |");
    assert!(
        tight_error < default_error,
        "{tight_error} >= {default_error}"
    );
    assert!(default_reversals < 250 && tight_reversals < 250);
}
