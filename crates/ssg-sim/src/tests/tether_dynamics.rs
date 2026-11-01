//! The tether's dynamics on the Q13 scenario (P7.5 S2, F-66): the rope at rest,
//! and two `#[ignore]`d long runs (a 10-minute rest; the run to the capture phase
//! with its contact count).
//!
//! "Relative to the root" is a body's velocity minus the root's. A floating-origin
//! reset shifts every enabled body by the same `(com_r, com_v)`, so the relative
//! speed and the length need nothing.

use avian3d::prelude::*;
use bevy::math::DVec3;
use bevy::prelude::*;

use crate::capture::CaptureComponent;
use crate::orbit::selection::OrbitalSelectionState;
use crate::project::SelectedProject;
use crate::test_support::{enter_sim, load_example_plans, q13, start_running, test_app, tick};
use crate::tether::chain_from_joints;

/// The default tether length, m.
const TETHER_LENGTH_M: f64 = 20.0;
/// 10 s at 64 Hz.
const WINDOW_TICKS: u64 = 640;
/// The window is sampled every this many ticks.
const SAMPLE_EVERY: u64 = 64;

/// The tether's bodies, root first, from the distance joints.
fn chain(app: &mut App, root: Entity) -> Vec<Entity> {
    let world = app.world_mut();
    let joints: Vec<DistanceJoint> = world
        .query::<&DistanceJoint>()
        .iter(world)
        .cloned()
        .collect();
    chain_from_joints(root, joints.iter())
}

fn position(app: &App, entity: Entity) -> DVec3 {
    app.world().get::<Position>(entity).expect("Position").0
}

fn velocity(app: &App, entity: Entity) -> DVec3 {
    app.world()
        .get::<LinearVelocity>(entity)
        .expect("LinearVelocity")
        .0
}

/// One look at the chain.
#[derive(Debug, Default)]
struct Sample {
    /// The largest speed of a tether body relative to the root, m/s.
    max_rel_speed: f64,
    /// Root to tail, m.
    length: f64,
    /// Touching pairs whose two colliders are tether bodies.
    tether_tether: usize,
    /// Touching pairs of a tether body and another collider (the debris is the
    /// only other one).
    tether_debris: usize,
}

/// The touching pairs as `(tether–tether, tether–other)`.
fn touching_pairs(app: &App, chain: &[Entity]) -> (usize, usize) {
    let mut pairs = (0, 0);
    for pair in app
        .world()
        .resource::<ContactGraph>()
        .iter_active_touching()
    {
        match (
            chain.contains(&pair.collider1),
            chain.contains(&pair.collider2),
        ) {
            (true, true) => pairs.0 += 1,
            (true, false) | (false, true) => pairs.1 += 1,
            (false, false) => {}
        }
    }
    pairs
}

fn sample(app: &App, chain: &[Entity]) -> Sample {
    let root_v = velocity(app, chain[0]);
    let max_rel_speed = chain
        .iter()
        .map(|&body| (velocity(app, body) - root_v).length())
        .fold(0.0, f64::max);
    let (tether_tether, tether_debris) = touching_pairs(app, chain);
    Sample {
        max_rel_speed,
        length: (position(app, chain[chain.len() - 1]) - position(app, chain[0])).length(),
        tether_tether,
        tether_debris,
    }
}

/// The relative length error of `length` against the tether length.
fn length_error(length: f64) -> f64 {
    (length / TETHER_LENGTH_M - 1.0).abs()
}

/// The Q13 scenario with no capture: the bodies spawned and the propagators built
/// (step 2), as [`q13::build`] goes up to its capture start.
fn q13_without_a_capture(app: &mut App) -> Entity {
    load_example_plans(app);
    app.world_mut().resource_mut::<SelectedProject>().project_id = Some(q13::PLAN_ID.to_string());
    {
        let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
        selection.target = Some(q13::catalog_pick("HST", q13::HST_NORAD_ID));
        selection.chaser = Some(q13::catalog_pick("P0 HST CHASER", q13::CHASER_NORAD_ID));
    }
    enter_sim(app);
    start_running(app);
    tick(app);
    let plan_tether = app
        .world()
        .resource::<crate::capture::plan::CapturePlanLibrary>()
        .plans[q13::PLAN_ID]
        .tether
        .clone();
    app.world()
        .resource::<crate::orbit::cache::OrbitalCache>()
        .tethers[&plan_tether][0]
}

/// Without a capture the rope stays where it was spawned: no body moves more than
/// 5 mm relative to the root in 10 s (the rod moved 1.8 mm, P7 audit).
#[test]
fn tether_still_without_a_capture() {
    let mut app = test_app();
    let root = q13_without_a_capture(&mut app);
    let chain = chain(&mut app, root);
    let offsets = |app: &App| -> Vec<DVec3> {
        let root_p = position(app, root);
        chain
            .iter()
            .map(|&body| position(app, body) - root_p)
            .collect()
    };
    let before = offsets(&app);
    for _ in 0..WINDOW_TICKS {
        tick(&mut app);
    }
    let moved = offsets(&app)
        .iter()
        .zip(&before)
        .map(|(after, before)| (*after - *before).length())
        .fold(0.0, f64::max);
    println!("tether_still_without_a_capture: largest move relative to the root {moved:.6} m");
    assert!(moved < 0.005, "a body moved {moved} m");
}

/// The rope left alone for 10 minutes of sim time (38,400 ticks): every body
/// under 0.05 m/s relative to the root, root to tail within 2 %, no touching
/// pair (review C10).
#[test]
#[ignore = "P7.5 long rest (~15 s release, longer in debug): cargo test -p ssg-sim --locked tether_rest_ten_minutes -- --ignored --nocapture"]
fn tether_rest_ten_minutes() {
    const TICKS: u64 = 38_400;
    let mut app = test_app();
    let root = q13_without_a_capture(&mut app);
    let chain = chain(&mut app, root);
    let mut peak = Sample::default();
    let mut worst_length_error = 0.0f64;
    for t in 1..=TICKS {
        tick(&mut app);
        if t % SAMPLE_EVERY == 0 {
            let s = sample(&app, &chain);
            peak.max_rel_speed = peak.max_rel_speed.max(s.max_rel_speed);
            peak.tether_tether = peak.tether_tether.max(s.tether_tether);
            peak.tether_debris = peak.tether_debris.max(s.tether_debris);
            worst_length_error = worst_length_error.max(length_error(s.length));
            peak.length = s.length;
        }
    }
    println!(
        "tether_rest_ten_minutes: {TICKS} ticks; peak relative speed {:.6} m/s; root->tail \
         at the end {:.4} m (worst {:+.4} %); touching pairs {} tether-tether, {} tether-debris",
        peak.max_rel_speed,
        peak.length,
        100.0 * worst_length_error,
        peak.tether_tether,
        peak.tether_debris,
    );
    assert!(
        peak.max_rel_speed < 0.05,
        "a tether body reached {} m/s relative to the root",
        peak.max_rel_speed
    );
    assert!(
        worst_length_error < 0.02,
        "root to tail off by {worst_length_error}"
    );
    assert_eq!(
        (peak.tether_tether, peak.tether_debris),
        (0, 0),
        "touching pairs"
    );
}

/// The scenario run to the capture phase, as `baseline_q13_to_capture` runs it:
/// no tether–tether pair ever touches; the tether–debris pairs and the first step
/// one touches are counted (the debris has a collider from S3 on; review C8).
#[test]
#[ignore = "P7.5 contact count to capture (~1 min release, ~4 min debug): cargo test -p ssg-sim --locked tether_to_capture_contacts -- --ignored --nocapture"]
fn tether_to_capture_contacts() {
    const MAX_TICKS: u64 = 250_000;
    let mut app = test_app();
    let handles = q13::build(&mut app);
    let chain = chain(&mut app, handles.root);
    let mut first_debris_contact: Option<u64> = None;
    let mut peak_debris_pairs = 0;
    let mut tether_tether_steps = 0u64;
    let mut ticks = 0;
    let mut state = String::new();
    while ticks < MAX_TICKS {
        tick(&mut app);
        ticks += 1;
        let (tether_tether, tether_debris) = touching_pairs(&app, &chain);
        let step = q13::sample(&app, &handles).step;
        if tether_tether > 0 {
            tether_tether_steps += 1;
        }
        if tether_debris > 0 && first_debris_contact.is_none() {
            first_debris_contact = Some(step);
        }
        peak_debris_pairs = peak_debris_pairs.max(tether_debris);
        state = app
            .world()
            .get::<CaptureComponent>(handles.debris)
            .expect("CaptureComponent")
            .current_state
            .clone();
        if state == "capture" {
            break;
        }
    }
    let end = q13::sample(&app, &handles);
    println!(
        "tether_to_capture_contacts: {ticks} ticks to '{state}' at step {}, range {} m; \
         first tether-debris touching step {first_debris_contact:?}; peak tether-debris \
         pairs {peak_debris_pairs}; steps with a tether-tether pair {tether_tether_steps}",
        end.step, end.range_m,
    );
    assert_eq!(state, "capture", "the run reached the capture phase");
    assert_eq!(tether_tether_steps, 0, "steps with a tether-tether pair");
}
