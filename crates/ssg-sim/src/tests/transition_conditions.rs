//! The P9 S3 transition conditions: `all_of.closing_rate_mps` (Q10 row 6; A-4: the
//! rate the root's range to the target shrinks, positive while closing) and
//! `all_of.tether_straightness` (F-22, E4 (b): the largest distance of a tether
//! body from the root→tail line over the root-to-tail distance; 0 = straight).
//! The measures, the condition checks (alone and AND-ed), the controller reading
//! them on the rig, validation, and one print of the straightness through the
//! example plan's terminal phase (ignored; run locally).

use avian3d::prelude::{Collider, DistanceJoint, LinearVelocity, Mass, Position, RigidBody};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

use crate::capture::plan::{CompiledCaptureTransition, validate_capture_plan};
use crate::capture::{
    CaptureComponent, TransitionInputs, closing_rate, tether_straightness, transition_satisfied,
};
use crate::test_support::{CaptureRig, capture_rig, insert_plan_json, rig_set, rig_step, test_app};

// ---------------------------------------------------------------------------
// The measures
// ---------------------------------------------------------------------------

#[test]
fn closing_rate_is_the_range_rate_positive_while_closing() {
    // `rel_r` from the root to the target; the root moves toward it at 0.2 m/s.
    let rel_r = DVec3::new(30.0, 40.0, 0.0);
    let toward = rel_r.normalize() * 0.2;
    assert!((closing_rate(rel_r, toward).unwrap() - 0.2).abs() < 1e-15);
    assert!((closing_rate(rel_r, -toward).unwrap() + 0.2).abs() < 1e-15);
    // Sideways motion does not change the range.
    let sideways = DVec3::Z * 3.0;
    assert_eq!(closing_rate(rel_r, sideways), Some(0.0));
    // A rotating frame's ω × r adds nothing (A-4).
    let omega = DVec3::new(0.1, -0.2, 0.3);
    let in_frame = closing_rate(rel_r, toward + omega.cross(rel_r)).unwrap();
    assert!((in_frame - 0.2).abs() < 1e-12, "{in_frame}");
    assert_eq!(closing_rate(DVec3::ZERO, toward), None, "undefined at 0 m");
}

/// `n + 1` points on a circular arc with chord `chord` and sagitta `sagitta`
/// (the bow at the middle), turned by `turn` and moved by `origin`.
fn arc(chord: f64, sagitta: f64, n: usize, turn: DQuat, origin: DVec3) -> Vec<DVec3> {
    let radius = (chord * chord / 4.0 + sagitta * sagitta) / (2.0 * sagitta);
    let half = (chord / 2.0 / radius).asin();
    (0..=n)
        .map(|i| {
            let angle = -half + 2.0 * half * i as f64 / n as f64;
            // The centre below the chord's middle; the chord along X from 0.
            let point = DVec3::new(
                chord / 2.0 + radius * angle.sin(),
                radius * angle.cos() - (radius - sagitta),
                0.0,
            );
            origin + turn * point
        })
        .collect()
}

#[test]
fn a_straight_chain_measures_zero() {
    let axis = DVec3::new(1.0, 2.0, -0.5).normalize();
    let chain: Vec<DVec3> = (0..=80)
        .map(|i| DVec3::new(7.0, -3.0, 2.0) + axis * (0.25 * i as f64))
        .collect();
    let bow = tether_straightness(&chain).unwrap();
    assert!(bow.abs() < 1e-12, "{bow}");
}

#[test]
fn a_bowed_chain_measures_its_sagitta_over_its_chord() {
    let turn = DQuat::from_euler(EulerRot::XYZ, 0.3, -1.1, 0.7);
    for (chord, sagitta) in [(10.0, 1.5), (19.0, 0.2), (4.0, 1.9)] {
        let chain = arc(chord, sagitta, 40, turn, DVec3::new(-5.0, 12.0, 3.0));
        let bow = tether_straightness(&chain).unwrap();
        assert!(
            (bow - sagitta / chord).abs() < 1e-12,
            "chord {chord}, sagitta {sagitta}: {bow} != {}",
            sagitta / chord
        );
    }
}

#[test]
fn straightness_is_undefined_when_the_ends_meet() {
    assert_eq!(tether_straightness(&[]), None);
    assert_eq!(tether_straightness(&[DVec3::ONE]), None);
    let loop_ = [DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::new(0.0, 0.0, 5e-7)];
    assert_eq!(tether_straightness(&loop_), None, "ends 5e-7 m apart");
}

// ---------------------------------------------------------------------------
// The condition checks
// ---------------------------------------------------------------------------

fn inputs(closing: Option<f64>, straightness: Option<f64>) -> TransitionInputs {
    TransitionInputs {
        distance_m: 5.0,
        relative_speed_mps: 0.2,
        closing_rate_mps: closing,
        tether_straightness: straightness,
        time_in_phase_s: 10.0,
    }
}

fn closing(less_than: Option<f64>, greater_than: Option<f64>) -> CompiledCaptureTransition {
    CompiledCaptureTransition {
        to: "b".to_string(),
        closing_rate_less_than: less_than,
        closing_rate_greater_than: greater_than,
        ..Default::default()
    }
}

#[test]
fn closing_at_0_2_fires_less_than_0_3_and_not_greater_than_0_3() {
    let measured = inputs(Some(0.2), None);
    assert_eq!(
        transition_satisfied(&closing(Some(0.3), None), &measured).as_deref(),
        Some("closing rate 0.20 m/s < 0.30 m/s")
    );
    assert_eq!(
        transition_satisfied(&closing(None, Some(0.3)), &measured),
        None
    );
    assert_eq!(
        transition_satisfied(&closing(None, Some(0.1)), &measured).as_deref(),
        Some("closing rate 0.20 m/s > 0.10 m/s")
    );
}

#[test]
fn an_opening_range_never_fires_greater_than() {
    // Bounds are not negative, so `greater_than` asks for a closing range.
    for bound in [0.0, 0.1, 5.0] {
        let transition = closing(None, Some(bound));
        assert_eq!(
            transition_satisfied(&transition, &inputs(Some(-0.2), None)),
            None
        );
    }
    // Undefined (range 0): neither bound holds.
    assert_eq!(
        transition_satisfied(&closing(Some(1.0), None), &inputs(None, None)),
        None
    );
    assert_eq!(
        transition_satisfied(&closing(None, Some(0.0)), &inputs(None, None)),
        None
    );
}

#[test]
fn straightness_and_distance_combine_as_and() {
    let transition = CompiledCaptureTransition {
        to: "b".to_string(),
        distance_less_than: Some(10.0),
        tether_straightness_less_than: Some(0.15),
        ..Default::default()
    };
    assert_eq!(
        transition_satisfied(&transition, &inputs(None, Some(0.1))).as_deref(),
        Some("distance 5.0 m < 10.0 m, tether straightness 0.100 < 0.150")
    );
    assert_eq!(
        transition_satisfied(&transition, &inputs(None, Some(0.2))),
        None,
        "bowed"
    );
    assert_eq!(
        transition_satisfied(&transition, &inputs(None, None)),
        None,
        "not measured"
    );
    let far = TransitionInputs {
        distance_m: 12.0,
        ..inputs(None, Some(0.1))
    };
    assert_eq!(transition_satisfied(&transition, &far), None, "too far");
    let bowed = CompiledCaptureTransition {
        to: "b".to_string(),
        tether_straightness_greater_than: Some(0.15),
        ..Default::default()
    };
    assert_eq!(
        transition_satisfied(&bowed, &inputs(None, Some(0.2))).as_deref(),
        Some("tether straightness 0.200 > 0.150")
    );
}

// ---------------------------------------------------------------------------
// The controller reads them (the rig)
// ---------------------------------------------------------------------------

const CLOSING_PLAN: &str = r#"{"schema_version": 1, "name": "closing", "tether": "T", "phases": [
  {"id": "a", "max_velocity_mps": 1.0, "max_force_n": 10.0,
   "transitions": [{"to": "b", "all_of": {"closing_rate_mps": {"greater_than": 0.1}}}]},
  {"id": "b", "max_velocity_mps": 1.0, "max_force_n": 10.0}
]}"#;

#[test]
fn the_controller_fires_on_the_roots_closing_rate() {
    let mut app = test_app();
    insert_plan_json(&mut app, "closing", CLOSING_PLAN);
    let rig = capture_rig(&mut app, "closing");
    // `rig_set`: the nodes at the origin moving +X, the target at +X: closing.
    rig_set(&mut app, &rig, 50.0, -0.2);
    assert!(rig_step(&mut app, &rig).is_empty(), "opening at 0.2 m/s");
    rig_set(&mut app, &rig, 50.0, 0.05);
    assert!(rig_step(&mut app, &rig).is_empty(), "closing at 0.05 m/s");
    rig_set(&mut app, &rig, 50.0, 0.2);
    assert_eq!(
        rig_step(&mut app, &rig),
        vec![(
            "a".to_string(),
            "b".to_string(),
            "closing rate 0.20 m/s > 0.10 m/s".to_string()
        )]
    );
}

const STRAIGHT_PLAN: &str = r#"{"schema_version": 1, "name": "straight", "tether": "T", "phases": [
  {"id": "a", "max_velocity_mps": 1.0, "max_force_n": 10.0,
   "transitions": [{"to": "b", "all_of": {"tether_straightness": {"less_than": 0.15}}}]},
  {"id": "b", "max_velocity_mps": 1.0, "max_force_n": 10.0}
]}"#;

/// The rig with a middle body joined root → middle → tail, as the spawner joins
/// a tether.
fn three_body_rig() -> (App, CaptureRig, Entity) {
    let mut app = test_app();
    insert_plan_json(&mut app, "straight", STRAIGHT_PLAN);
    let rig = capture_rig(&mut app, "straight");
    let middle = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Mass::from(2.0),
            Position::default(),
        ))
        .id();
    for (a, b) in [(rig.root, middle), (middle, rig.tail)] {
        app.world_mut().spawn(DistanceJoint::new(a, b));
    }
    app.update();
    (app, rig, middle)
}

fn place_chain(app: &mut App, bodies: [(Entity, DVec3); 3]) {
    for (body, at) in bodies {
        app.world_mut()
            .entity_mut(body)
            .insert((Position(at), LinearVelocity::ZERO));
    }
}

#[test]
fn the_controller_fires_on_the_tethers_straightness_from_the_joint_chain() {
    let (mut app, rig, middle) = three_body_rig();
    rig_set(&mut app, &rig, 50.0, 0.0);
    // Root and tail both at the origin: undefined, no transition.
    assert!(rig_step(&mut app, &rig).is_empty(), "ends together");

    // Bowed 3 m over a 10 m chord: 0.3.
    let ends = |middle_y: f64| {
        [
            (rig.root, DVec3::ZERO),
            (middle, DVec3::new(5.0, middle_y, 0.0)),
            (rig.tail, DVec3::new(10.0, 0.0, 0.0)),
        ]
    };
    place_chain(&mut app, ends(3.0));
    assert!(rig_step(&mut app, &rig).is_empty(), "0.3 is not below 0.15");
    place_chain(&mut app, ends(1.0));
    assert_eq!(
        rig_step(&mut app, &rig),
        vec![(
            "a".to_string(),
            "b".to_string(),
            "tether straightness 0.100 < 0.150".to_string()
        )]
    );
    assert_eq!(
        app.world()
            .get::<CaptureComponent>(rig.capture)
            .unwrap()
            .current_state,
        "b"
    );
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn errors_for(all_of: &str) -> Vec<String> {
    let json = format!(
        r#"{{"schema_version": 1, "name": "v", "tether": "T", "phases": [
          {{"id": "a", "max_velocity_mps": 1.0, "max_force_n": 1.0,
            "transitions": [{{"to": "a", "all_of": {all_of}}}]}}]}}"#
    );
    let plan = serde_json::from_str(&json).expect("a v1 plan");
    validate_capture_plan("v", &plan)
}

#[test]
fn the_new_conditions_validate_as_the_others() {
    for key in ["closing_rate_mps", "tether_straightness"] {
        assert_eq!(
            errors_for(&format!(r#"{{"{key}": {{"less_than": 0.3}}}}"#)),
            Vec::<String>::new(),
            "{key} alone is a condition"
        );
        assert_eq!(
            errors_for(&format!(r#"{{"{key}": {{}}}}"#)),
            [format!(
                "[v] /phases/0/transitions/0/all_of/{key}: at least one of less_than, \
                 greater_than is required"
            )]
        );
        assert_eq!(
            errors_for(&format!(r#"{{"{key}": {{"greater_than": -0.1}}}}"#)),
            [format!(
                "[v] /phases/0/transitions/0/all_of/{key}/greater_than: must not be negative"
            )]
        );
    }
    assert_eq!(
        errors_for("{}"),
        ["[v] /phases/0/transitions/0/all_of: at least one condition is required"]
    );
}

// ---------------------------------------------------------------------------
// Through the example plan (ignored: ~minutes; the PR's harness print)
// ---------------------------------------------------------------------------

/// The straightness of the Q13 tether through the first 60 s of the example
/// plan's terminal phase, sampled every second: finite and in [0, 1).
#[test]
#[ignore = "P9 S3 print (minutes): cargo test -p ssg-sim --locked -- --ignored --nocapture straightness_through_the_example_plans_terminal_phase"]
fn straightness_through_the_example_plans_terminal_phase() {
    use crate::test_support::{q13, tick};
    use crate::tests::tether_dynamics::{chain, position};

    let mut app = test_app();
    let handles = q13::build(&mut app);
    let bodies = chain(&mut app, handles.root);
    let mut ticks = 0u64;
    while q13::sample(&app, &handles).state != "terminal" {
        tick(&mut app);
        ticks += 1;
        assert!(ticks < 260_000, "no terminal phase");
    }
    let entry = q13::sample(&app, &handles).step;
    let mut samples = Vec::new();
    for second in 0..=60u64 {
        let points: Vec<DVec3> = bodies.iter().map(|&b| position(&app, b)).collect();
        let bow = tether_straightness(&points).expect("the ends apart");
        assert!(bow.is_finite() && (0.0..1.0).contains(&bow), "{bow}");
        samples.push((second, bow));
        for _ in 0..64 {
            tick(&mut app);
        }
    }
    let (min, max) = samples
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &(_, b)| {
            (lo.min(b), hi.max(b))
        });
    let mean = samples.iter().map(|&(_, b)| b).sum::<f64>() / samples.len() as f64;
    println!("terminal entry at step {entry}; {} bodies", bodies.len());
    println!("| t in terminal (s) | tether_straightness |");
    println!("|---|---|");
    for (second, bow) in samples.iter().filter(|(s, _)| s % 10 == 0) {
        println!("| {second} | {bow:.4} |");
    }
    println!("min {min:.4} · mean {mean:.4} · max {max:.4} over 61 samples");
}
