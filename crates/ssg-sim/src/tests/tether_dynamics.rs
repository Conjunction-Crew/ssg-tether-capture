//! The tether's dynamics on the Q13 scenario (P7.5 S2, S3; F-66, F-64): the §4
//! acceptance measurements in the 10 s after the capture start, the collider set,
//! the rope at rest, and two `#[ignore]`d long runs (a 10-minute rest; the run to
//! the capture phase with its contact count).
//!
//! "Relative to the root" is a body's velocity minus the root's. A floating-origin
//! reset shifts every enabled body by the same `(com_r, com_v)`, so the relative
//! speed and the length need nothing; the root's velocity change adds the reset's
//! `com_v` back (review C4).

use avian3d::prelude::*;
use bevy::ecs::message::Messages;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

use crate::capture::CaptureForce;
use crate::capture::plan::{CapturePlanDevice, CapturePlanLibrary, SpawnOrientation};
use crate::floating_origin::OriginReset;
use crate::orbit::selection::OrbitalSelectionState;
use crate::project::SelectedProject;
use crate::test_support::{enter_sim, load_example_plans, q13, start_running, test_app, tick};
use crate::tether::{
    DEBRIS_LAYER, DEBRIS_LENGTH_M, DEBRIS_RADIUS_M, TETHER_LAYER, chain_from_joints,
};
use crate::time::PHYS_DT;

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
/// (step 2), as [`q13::build`] goes up to its capture start. `orientation` sets the
/// example plan's `device.spawn_orientation` before the spawn (`None`: the plan's).
fn q13_without_a_capture(app: &mut App, orientation: Option<SpawnOrientation>) -> Entity {
    load_example_plans(app);
    if let Some(orientation) = orientation {
        app.world_mut()
            .resource_mut::<CapturePlanLibrary>()
            .plans
            .get_mut(q13::PLAN_ID)
            .expect("example_plan")
            .device
            .get_or_insert_with(CapturePlanDevice::default)
            .spawn_orientation = Some(orientation);
    }
    app.world_mut().resource_mut::<SelectedProject>().project_id = Some(q13::PLAN_ID.to_string());
    {
        let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
        selection.target = Some(q13::catalog_pick("HST", q13::HST_NORAD_ID));
        selection.chaser = Some(q13::catalog_pick("P0 HST CHASER", q13::CHASER_NORAD_ID));
    }
    enter_sim(app);
    start_running(app);
    tick(app);
    let plan_tether = app.world().resource::<CapturePlanLibrary>().plans[q13::PLAN_ID]
        .tether
        .clone();
    app.world()
        .resource::<crate::orbit::cache::OrbitalCache>()
        .tethers[&plan_tether][0]
}

/// §4 P7.5 acceptance (F-66): in the 10 s after the capture start no tether body
/// exceeds 1 m/s relative to the root, no tether–tether pair touches, the root
/// stays within 2 % of 20 m from the tail, and the root's velocity change matches
/// the controller's impulse over the chain's mass to 1 % (reset-aware, C4).
#[test]
fn tether_steady_at_capture_start() {
    let mut app = test_app();
    let handles = q13::build(&mut app);
    let chain = chain(&mut app, handles.root);
    assert_eq!(chain.len(), 77, "root + 75 nodes + tail");
    let mass: f64 = chain
        .iter()
        .map(|&body| f64::from(app.world().get::<Mass>(body).expect("Mass").0))
        .sum();

    let start_v = velocity(&app, handles.root);
    let mut resets = app
        .world()
        .resource::<Messages<OriginReset>>()
        .get_cursor_current();
    let mut impulse = 0.0;
    let mut reset_v = DVec3::ZERO;
    let mut reset_count = 0;
    let mut peak = Sample::default();
    let mut worst_length_error = 0.0f64;
    for t in 1..=WINDOW_TICKS {
        tick(&mut app);
        impulse += chain
            .iter()
            .filter_map(|&body| app.world().get::<CaptureForce>(body))
            .map(|force| force.applied.length())
            .sum::<f64>()
            * PHYS_DT;
        for reset in resets.read(app.world().resource::<Messages<OriginReset>>()) {
            reset_v += reset.com_v;
            reset_count += 1;
        }
        if t % SAMPLE_EVERY == 0 {
            let s = sample(&app, &chain);
            peak.max_rel_speed = peak.max_rel_speed.max(s.max_rel_speed);
            peak.tether_tether = peak.tether_tether.max(s.tether_tether);
            peak.tether_debris = peak.tether_debris.max(s.tether_debris);
            worst_length_error = worst_length_error.max(length_error(s.length));
            peak.length = s.length;
        }
    }
    let delta_v = (velocity(&app, handles.root) - start_v + reset_v).length();
    let expected = impulse / mass;
    let f_over_m_error = delta_v / expected - 1.0;
    println!(
        "tether_steady_at_capture_start: chain {} bodies, {mass:.3} kg; peak relative \
         speed {:.4} m/s; root->tail at 10 s {:.4} m (worst {:+.3} %); tether-tether \
         pairs {}; tether-debris pairs {}; root dv {delta_v:.5} m/s vs impulse/mass \
         {expected:.5} m/s ({:+.3} %); origin resets {reset_count}",
        chain.len(),
        peak.max_rel_speed,
        peak.length,
        100.0 * worst_length_error,
        peak.tether_tether,
        peak.tether_debris,
        100.0 * f_over_m_error,
    );

    assert!(
        peak.max_rel_speed < 1.0,
        "a tether body reached {} m/s relative to the root",
        peak.max_rel_speed
    );
    assert_eq!(peak.tether_tether, 0, "tether-tether touching pairs");
    assert!(
        worst_length_error < 0.02,
        "root to tail off by {:.3} % of 20 m",
        100.0 * worst_length_error
    );
    assert!(
        f_over_m_error.abs() < 0.01,
        "root dv {delta_v} m/s against impulse/mass {expected} m/s"
    );
    // A documented fact of the default plan, not a dependency: the chain's centre
    // stays inside the 50 m reset threshold for these 10 s.
    assert_eq!(reset_count, 0, "origin resets in the window");
}

/// F-64: headless, the colliders are the rope's 77 bodies and the debris's
/// primitive cylinder, each on its layer, and no collider is built from a mesh. The
/// cylinder's long axis is the debris body's X (D11): its AABB at the identity pose
/// is 6.6 m along X and 2.1 m along Y and Z, and the explicit 2,500 kg replaces its
/// density mass and scales its inertia (axial on X, transverse on Y and Z).
#[test]
fn the_headless_collider_set_is_the_rope_and_the_debris_cylinder() {
    let mut app = test_app();
    let handles = q13::build(&mut app);
    let chain = chain(&mut app, handles.root);
    let world = app.world_mut();
    let colliders: Vec<Entity> = world
        .query_filtered::<Entity, With<Collider>>()
        .iter(world)
        .collect();
    assert_eq!(colliders.len(), 78, "77 tether bodies + the debris");
    assert!(chain.iter().all(|body| colliders.contains(body)));
    assert!(colliders.contains(&handles.debris));
    assert_eq!(
        world
            .query::<&ColliderConstructorHierarchy>()
            .iter(world)
            .count(),
        0
    );
    for &body in &chain {
        assert_eq!(
            world.get::<CollisionLayers>(body),
            Some(&CollisionLayers::new(TETHER_LAYER, DEBRIS_LAYER))
        );
    }
    assert_eq!(
        world.get::<CollisionLayers>(handles.debris),
        Some(&CollisionLayers::new(DEBRIS_LAYER, TETHER_LAYER))
    );
    let aabb = world
        .get::<Collider>(handles.debris)
        .expect("debris Collider")
        .aabb(DVec3::ZERO, DQuat::IDENTITY);
    let half_extents = (aabb.max - aabb.min) / 2.0;
    println!("debris collider AABB half-extents {half_extents} m");
    assert!(
        (half_extents - DVec3::new(6.6, 2.1, 2.1))
            .abs()
            .max_element()
            < 1e-9,
        "the long axis is X: {half_extents}"
    );
    let mass = world
        .get::<ComputedMass>(handles.debris)
        .expect("ComputedMass")
        .value();
    let inertia = world
        .get::<ComputedAngularInertia>(handles.debris)
        .expect("ComputedAngularInertia")
        .tensor();
    println!(
        "debris: ComputedMass {mass} kg; ComputedAngularInertia diag ({}, {}, {}) kg m2",
        inertia.m00, inertia.m11, inertia.m22
    );
    let (r, l) = (2.1f64, 13.2f64);
    let axial = 2500.0 * r * r / 2.0;
    let transverse = 2500.0 * (3.0 * r * r + l * l) / 12.0;
    for (actual, expected, what) in [
        (mass, 2500.0, "mass"),
        (inertia.m00, axial, "axial inertia (X)"),
        (inertia.m11, transverse, "transverse inertia (Y)"),
        (inertia.m22, transverse, "transverse inertia (Z)"),
    ] {
        assert!(
            (actual / expected - 1.0).abs() < 1e-4,
            "{what}: {actual} vs {expected}"
        );
    }
}

/// Without a capture the rope stays where it was spawned: no body moves more than
/// 5 mm relative to the root in 10 s (the rod moved 1.8 mm, P7 audit).
#[test]
fn tether_still_without_a_capture() {
    let mut app = test_app();
    let root = q13_without_a_capture(&mut app, None);
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

/// The rope left alone for 10 minutes of sim time (38,400 ticks), laid along −R-bar
/// (`spawn_orientation: r_bar`, decision (b) of 2026-11-01: of the three options
/// only R-bar is a gravity-gradient equilibrium): every body under 0.05 m/s
/// relative to the root, root to tail within 2 %, no touching pair (review C10).
/// The default `toward_target` axis is not an equilibrium (the line of sight
/// rotates ≈ 3.8°/min, D9); its drift is a recorded fact, not asserted here.
#[test]
#[ignore = "P7.5 long rest (~15 s release, longer in debug): cargo test -p ssg-sim --locked tether_rest_ten_minutes -- --ignored --nocapture"]
fn tether_rest_ten_minutes() {
    const TICKS: u64 = 38_400;
    let mut app = test_app();
    let root = q13_without_a_capture(&mut app, Some(SpawnOrientation::RBar));
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

/// The touching pairs counted by the tether body's role: the root, the tail or an
/// interior node with the debris (the only other collider), and tether–tether.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct RolePairs {
    root_debris: usize,
    tail_debris: usize,
    node_debris: usize,
    tether_tether: usize,
}

fn role_pairs(app: &App, chain: &[Entity]) -> RolePairs {
    let (root, tail) = (chain[0], chain[chain.len() - 1]);
    let mut pairs = RolePairs::default();
    for pair in app
        .world()
        .resource::<ContactGraph>()
        .iter_active_touching()
    {
        let tether_body = match (
            chain.contains(&pair.collider1),
            chain.contains(&pair.collider2),
        ) {
            (true, true) => {
                pairs.tether_tether += 1;
                continue;
            }
            (true, false) => pair.collider1,
            (false, true) => pair.collider2,
            (false, false) => continue,
        };
        if tether_body == root {
            pairs.root_debris += 1;
        } else if tether_body == tail {
            pairs.tail_debris += 1;
        } else {
            pairs.node_debris += 1;
        }
    }
    pairs
}

/// The clearance of a sphere tether body from the debris cylinder's surface, m:
/// the distance from its centre to the cylinder (long axis the debris's X, D11),
/// in the debris's axes, minus the sphere's radius. Negative inside or touching.
fn clearance_from_debris(app: &App, body: Entity, debris: Entity) -> f64 {
    let rotation = app.world().get::<Rotation>(debris).expect("Rotation").0;
    let local = rotation.inverse() * (position(app, body) - position(app, debris));
    let along = local.x.abs() - DEBRIS_LENGTH_M / 2.0;
    let across = (local.y * local.y + local.z * local.z).sqrt() - DEBRIS_RADIUS_M;
    let to_surface = if along <= 0.0 && across <= 0.0 {
        along.max(across)
    } else {
        (along.max(0.0).powi(2) + across.max(0.0).powi(2)).sqrt()
    };
    let collider = app.world().get::<Collider>(body).expect("Collider");
    let aabb = collider.aabb(DVec3::ZERO, DQuat::IDENTITY);
    to_surface - (aabb.max.x - aabb.min.x) / 2.0
}

/// The scenario run to the capture phase, as `baseline_q13_to_capture` runs it
/// (review C8; the debris has its collider from S3 on). Decision (a) of
/// 2026-11-01 as an asserted outcome: in a successful capture neither the root nor
/// the tail touches the debris, only the interior nodes wrap, so up to the capture
/// transition no root–debris, tail–debris or tether–tether pair touches, and at the
/// transition the root and the tail are clear of the cylinder (design D11). The
/// first node–debris touching step and the peak node–debris pairs are printed.
#[test]
#[ignore = "P7.5 contact count to capture (~1.5 min release): cargo test -p ssg-sim --locked tether_to_capture_contacts -- --ignored --nocapture"]
fn tether_to_capture_contacts() {
    const MAX_TICKS: u64 = 250_000;
    let started = std::time::Instant::now();
    let mut app = test_app();
    let handles = q13::build(&mut app);
    let chain = chain(&mut app, handles.root);
    let tail = chain[chain.len() - 1];
    let mut first_node_contact: Option<u64> = None;
    let mut first_tether_debris_contact: Option<u64> = None;
    let mut totals = RolePairs::default();
    let mut peak = RolePairs::default();
    let mut tether_tether_steps = 0u64;
    let mut transitions = Vec::new();
    let mut ticks = 0;
    let mut state = String::new();
    while ticks < MAX_TICKS {
        tick(&mut app);
        ticks += 1;
        let pairs = role_pairs(&app, &chain);
        let sample = q13::sample(&app, &handles);
        if pairs.tether_tether > 0 {
            tether_tether_steps += 1;
        }
        if pairs.node_debris > 0 && first_node_contact.is_none() {
            first_node_contact = Some(sample.step);
        }
        if pairs.root_debris + pairs.tail_debris + pairs.node_debris > 0
            && first_tether_debris_contact.is_none()
        {
            first_tether_debris_contact = Some(sample.step);
        }
        totals.root_debris += pairs.root_debris;
        totals.tail_debris += pairs.tail_debris;
        totals.node_debris += pairs.node_debris;
        totals.tether_tether += pairs.tether_tether;
        peak.root_debris = peak.root_debris.max(pairs.root_debris);
        peak.tail_debris = peak.tail_debris.max(pairs.tail_debris);
        peak.node_debris = peak.node_debris.max(pairs.node_debris);
        peak.tether_tether = peak.tether_tether.max(pairs.tether_tether);
        if sample.state != state {
            if !state.is_empty() {
                transitions.push((sample.step, sample.state.clone()));
            }
            state = sample.state;
        }
        if state == "capture" {
            break;
        }
    }
    let end = q13::sample(&app, &handles);
    let root_clearance = clearance_from_debris(&app, handles.root, handles.debris);
    let tail_clearance = clearance_from_debris(&app, tail, handles.debris);
    let wall = started.elapsed().as_secs_f64();
    println!(
        "tether_to_capture_contacts: {ticks} ticks to '{state}' at step {}, range {} m; \
         transitions {transitions:?}; first tether-debris touching step \
         {first_tether_debris_contact:?}; first node-debris touching step \
         {first_node_contact:?}; peak pairs {peak:?}; pair-steps summed {totals:?}; \
         steps with a tether-tether pair {tether_tether_steps}; at the capture \
         transition the root is {root_clearance:.4} m and the tail {tail_clearance:.4} m \
         clear of the debris; wall {wall:.1} s ({:.3} ms/step)",
        end.step,
        end.range_m,
        1000.0 * wall / ticks as f64,
    );
    assert_eq!(state, "capture", "the run reached the capture phase");
    assert_eq!(tether_tether_steps, 0, "steps with a tether-tether pair");
    assert_eq!(
        (totals.root_debris, totals.tail_debris),
        (0, 0),
        "root-debris and tail-debris pairs up to the capture (decision (a))"
    );
    assert!(
        root_clearance > 0.0,
        "the root is {root_clearance} m from the debris at the capture"
    );
    assert!(
        tail_clearance > 0.0,
        "the tail is {tail_clearance} m from the debris at the capture"
    );
}
