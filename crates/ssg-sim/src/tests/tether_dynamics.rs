//! The tether's dynamics on the Q13 scenario (P7.5 S2, S3, S6; F-66, F-64): the §4
//! acceptance measurements in the 10 s after the capture start, the collider set
//! and the debris's mass properties, the rope at rest, and two `#[ignore]`d long
//! runs (a 10-minute rest; the run to the capture phase with its contact count).
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
    DEBRIS_ARRAY_A, DEBRIS_ARRAY_B, DEBRIS_BUS, DEBRIS_LAYER, DebrisBox, TETHER_FILTERS,
    TETHER_LAYER, chain_from_joints, debris_angular_inertia,
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

/// The debris's two array children (P7.5 S6): every collider entity whose parent
/// is the debris, as `(array A, array B)` (A on +Z, B on −Z of the body).
fn debris_arrays(app: &mut App, debris: Entity) -> (Entity, Entity) {
    let world = app.world_mut();
    let children: Vec<(Entity, f32)> = world
        .query_filtered::<(Entity, &ChildOf, &Transform), With<Collider>>()
        .iter(world)
        .filter(|(_, child_of, _)| child_of.parent() == debris)
        .map(|(entity, _, transform)| (entity, transform.translation.z))
        .collect();
    assert_eq!(children.len(), 2, "two array children: {children:?}");
    let a = children.iter().find(|(_, z)| *z > 0.0).expect("array A").0;
    let b = children.iter().find(|(_, z)| *z < 0.0).expect("array B").0;
    (a, b)
}

/// The world AABB of a box rotated about X, as avian's `Collider::aabb` gives it:
/// the half-extents `(hx, hy cos θ + hz |sin θ|, hy |sin θ| + hz cos θ)`.
fn rotated_half_extents(b: &DebrisBox) -> DVec3 {
    let (sin, cos) = b.rotation_about_x_rad.sin_cos();
    let h = b.half_extents;
    DVec3::new(
        h.x,
        h.y * cos + h.z * sin.abs(),
        h.y * sin.abs() + h.z * cos,
    )
}

/// F-64, design D13 (P7.5 S6, v0.7): headless, the colliders are the rope's 77
/// bodies, the debris's bus (its own collider, a one-shape compound) and the two
/// arrays (child collider entities attached to the debris by avian), each on its
/// layer, and no collider is built from a mesh. The debris's mass properties are the
/// stated ones with the children present: 2,500 kg, the centre at the origin and
/// the three-shape compound's tensor about the origin (C1, E21).
#[test]
fn the_headless_collider_set_is_the_rope_the_bus_and_the_two_arrays() {
    let mut app = test_app();
    let handles = q13::build(&mut app);
    let chain = chain(&mut app, handles.root);
    let (array_a, array_b) = debris_arrays(&mut app, handles.debris);
    let world = app.world_mut();
    let colliders: Vec<Entity> = world
        .query_filtered::<Entity, With<Collider>>()
        .iter(world)
        .collect();
    assert_eq!(
        colliders.len(),
        80,
        "77 tether bodies + the bus + two arrays"
    );
    assert!(chain.iter().all(|body| colliders.contains(body)));
    for collider in [handles.debris, array_a, array_b] {
        assert!(colliders.contains(&collider));
    }
    assert_eq!(
        world
            .query_filtered::<Entity, (With<RigidBody>, With<crate::tether::DebrisBody>)>()
            .iter(world)
            .count(),
        1,
        "one debris rigid body"
    );
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
            Some(&CollisionLayers::new(TETHER_LAYER, TETHER_FILTERS))
        );
    }
    for collider in [handles.debris, array_a, array_b] {
        assert_eq!(
            world.get::<CollisionLayers>(collider),
            Some(&CollisionLayers::new(DEBRIS_LAYER, TETHER_LAYER))
        );
    }
    for array in [array_a, array_b] {
        assert!(
            world.get::<RigidBody>(array).is_none(),
            "a child has no body"
        );
        assert_eq!(
            world.get::<ChildOf>(array).map(ChildOf::parent),
            Some(handles.debris)
        );
        assert_eq!(
            world.get::<ColliderOf>(array),
            Some(&ColliderOf {
                body: handles.debris
            }),
            "avian attached the child to the debris"
        );
    }

    // The shapes in their own frames: the bus compound's box (with its offset) and
    // each array's own box (its rotation lives in the child's `Transform`).
    for (collider, b, what) in [
        (handles.debris, DEBRIS_BUS, "bus"),
        (array_a, DEBRIS_ARRAY_A, "array A"),
        (array_b, DEBRIS_ARRAY_B, "array B"),
    ] {
        let aabb = world
            .get::<Collider>(collider)
            .expect("Collider")
            .aabb(DVec3::ZERO, DQuat::IDENTITY);
        let half = (aabb.max - aabb.min) / 2.0;
        let centre = (aabb.max + aabb.min) / 2.0;
        println!("{what}: local AABB half-extents {half}, centre {centre} m");
        assert!(
            (half - b.half_extents).abs().max_element() < 1e-9,
            "{what}: half-extents {half}"
        );
        let expected_centre = if collider == handles.debris {
            b.centre
        } else {
            DVec3::ZERO
        };
        assert!(
            (centre - expected_centre).abs().max_element() < 1e-9,
            "{what}: centre {centre}"
        );
    }

    // The mass properties with the children present (C1, E21).
    assert!(world.get::<NoAutoMass>(handles.debris).is_some());
    assert!(world.get::<NoAutoCenterOfMass>(handles.debris).is_some());
    assert!(world.get::<NoAutoAngularInertia>(handles.debris).is_some());
    let mass = world
        .get::<ComputedMass>(handles.debris)
        .expect("ComputedMass")
        .value();
    let centre = world
        .get::<ComputedCenterOfMass>(handles.debris)
        .expect("ComputedCenterOfMass")
        .0;
    let inertia = world
        .get::<ComputedAngularInertia>(handles.debris)
        .expect("ComputedAngularInertia")
        .tensor();
    // The diagonal and the off-diagonal (xy, xz, yz), kg·m².
    let (diag, off) = (
        [inertia.m00, inertia.m11, inertia.m22],
        [inertia.m01, inertia.m02, inertia.m12],
    );
    println!(
        "debris: ComputedMass {mass} kg; ComputedCenterOfMass {centre} m; \
         ComputedAngularInertia diag ({:.4}, {:.4}, {:.4}) off (xy {:.4}, xz {:.4}, yz {:.4}) kg m2",
        diag[0], diag[1], diag[2], off[0], off[1], off[2]
    );
    assert!((mass / 2500.0 - 1.0).abs() < 1e-9, "ComputedMass {mass}");
    assert!(
        f64::from(centre.length()) < 1e-9,
        "ComputedCenterOfMass {centre}"
    );
    // The compound's tensor about the origin (design D12 G2b printed it to 0.1).
    let want = debris_angular_inertia(2500.0).tensor();
    let (want_diag, want_off) = (
        [want.m00, want.m11, want.m22].map(f64::from),
        [want.m01, want.m02, want.m12].map(f64::from),
    );
    for (actual, expected, printed, what) in [
        (diag[0], want_diag[0], 28_110.5, "Ixx"),
        (diag[1], want_diag[1], 29_584.0, "Iyy"),
        (diag[2], want_diag[2], 5_792.8, "Izz"),
        (off[0], want_off[0], -109.7, "Ixy"),
        (off[1], want_off[1], -623.4, "Ixz"),
        (off[2], want_off[2], -4_135.1, "Iyz"),
    ] {
        assert!(
            (actual / expected - 1.0).abs() < 1e-4,
            "{what}: {actual} vs the compound's {expected}"
        );
        assert!(
            (actual - printed).abs() <= 0.05 + 1e-6 * printed.abs(),
            "{what}: {actual} vs the planner's {printed}"
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

/// The signed distance from `local` (a point in the debris's axes) to the box `b`'s
/// surface, m: the point is turned into the box's frame about its centre first
/// (array B: +0.26 rad about X). Negative inside.
fn signed_distance_to_box(local: DVec3, b: &DebrisBox) -> f64 {
    let p = DQuat::from_rotation_x(-b.rotation_about_x_rad) * (local - b.centre);
    let q = p.abs() - b.half_extents;
    q.max(DVec3::ZERO).length() + q.max_element().min(0.0)
}

/// The clearance of a sphere tether body from the debris's nearest box, m: the
/// signed distance from its centre to the bus, array A or array B (design D13), in
/// the debris's axes, minus the sphere's radius. Negative inside or touching.
fn clearance_from_debris(app: &App, body: Entity, debris: Entity) -> f64 {
    let rotation = app.world().get::<Rotation>(debris).expect("Rotation").0;
    let local = rotation.inverse() * (position(app, body) - position(app, debris));
    let to_surface = [DEBRIS_BUS, DEBRIS_ARRAY_A, DEBRIS_ARRAY_B]
        .iter()
        .map(|b| signed_distance_to_box(local, b))
        .fold(f64::INFINITY, f64::min);
    let collider = app.world().get::<Collider>(body).expect("Collider");
    let aabb = collider.aabb(DVec3::ZERO, DQuat::IDENTITY);
    to_surface - (aabb.max.x - aabb.min.x) / 2.0
}

#[test]
fn clearance_from_debris_measures_to_the_nearest_box() {
    // The bus's +X face is at x = 1.49; array A's +Y face at y = 0.05 over z
    // 1.5–9.5; array B is rotated −0.26 rad about X about (−0.18, −0.95, −5.4).
    let d = |p: DVec3| {
        [DEBRIS_BUS, DEBRIS_ARRAY_A, DEBRIS_ARRAY_B]
            .iter()
            .map(|b| signed_distance_to_box(p, b))
            .fold(f64::INFINITY, f64::min)
    };
    assert!((d(DVec3::new(3.49, 0.0, 0.0)) - 2.0).abs() < 1e-12);
    assert!((d(DVec3::new(0.0, 1.05, 5.5)) - 1.0).abs() < 1e-12);
    assert!(
        (d(DVec3::new(-0.585, 0.0, 0.0)) + 1.5).abs() < 1e-12,
        "inside the bus"
    );
    // 1 m along array B's own +Y normal from its centre: 0.7 m outside its face.
    let normal = DQuat::from_rotation_x(-0.26) * DVec3::Y;
    let p = DEBRIS_ARRAY_B.centre + normal;
    assert!((signed_distance_to_box(p, &DEBRIS_ARRAY_B) - 0.7).abs() < 1e-12);
}

/// What a tether body touches, by the collider entity: the debris body's own
/// collider is the bus, a child collider an array.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct ShapePairs {
    node_bus: usize,
    node_array: usize,
}

fn shape_pairs(app: &App, chain: &[Entity], debris: Entity) -> ShapePairs {
    let (root, tail) = (chain[0], chain[chain.len() - 1]);
    let mut pairs = ShapePairs::default();
    for pair in app
        .world()
        .resource::<ContactGraph>()
        .iter_active_touching()
    {
        let (body, other) = match (
            chain.contains(&pair.collider1),
            chain.contains(&pair.collider2),
        ) {
            (true, false) => (pair.collider1, pair.collider2),
            (false, true) => (pair.collider2, pair.collider1),
            _ => continue,
        };
        if body == root || body == tail {
            continue;
        }
        if other == debris {
            pairs.node_bus += 1;
        } else {
            pairs.node_array += 1;
        }
    }
    pairs
}

/// The three debris colliders' world placement relative to the debris centre, in
/// its axes: for the bus (the body's own collider) and each array child, avian's
/// shape AABB at the collider's own `Position`/`Rotation` (which avian derives for a
/// child from its `ColliderTransform` every step) and avian's `ColliderAabb` (the
/// broad phase's box, enlarged by avian's margins). Rows `(what, centre, half,
/// collider_aabb_centre, collider_aabb_half)`.
fn debris_world_boxes(
    app: &mut App,
    debris: Entity,
) -> Vec<(&'static str, DVec3, DVec3, DVec3, DVec3)> {
    let (array_a, array_b) = debris_arrays(app, debris);
    let world = app.world();
    let debris_p = world.get::<Position>(debris).expect("Position").0;
    let debris_r = world.get::<Rotation>(debris).expect("Rotation").0;
    [("bus", debris), ("array A", array_a), ("array B", array_b)]
        .into_iter()
        .map(|(what, entity)| {
            let p = world.get::<Position>(entity).expect("collider Position").0;
            let r = world.get::<Rotation>(entity).expect("collider Rotation").0;
            let aabb = world
                .get::<Collider>(entity)
                .expect("Collider")
                .aabb(debris_r.inverse() * (p - debris_p), debris_r.inverse() * r);
            let collider_aabb = world.get::<ColliderAabb>(entity).expect("ColliderAabb");
            (
                what,
                (aabb.min + aabb.max) / 2.0,
                (aabb.max - aabb.min) / 2.0,
                debris_r.inverse() * ((collider_aabb.min + collider_aabb.max) / 2.0 - debris_p),
                (collider_aabb.max - collider_aabb.min) / 2.0,
            )
        })
        .collect()
}

/// The scenario run to the capture phase, as `baseline_q13_to_capture` runs it
/// (review C8). Decision (a) of 2026-11-01 as an asserted outcome up to the capture
/// transition: no root–debris or tail–debris pair touches, and at the transition
/// the root and the tail are clear of the debris's boxes. Self-contact is on (P7.5
/// S6, design D13): the tether–tether pair-steps per phase are printed, not
/// asserted, with the node–bus and node–array counts. At the transition (no contact
/// and no torque before it, so the debris's `Rotation` is still the identity) the
/// three debris colliders' world placement is printed and asserted against the
/// `tether.rs` tables: the proof that avian placed the children where they say.
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
    let mut shape_steps = ShapePairs::default();
    // Steps with a tether–tether pair, per phase.
    let mut tether_tether_steps: Vec<(String, u64)> = Vec::new();
    let mut transitions = Vec::new();
    let mut ticks = 0;
    let mut state = String::new();
    while ticks < MAX_TICKS {
        tick(&mut app);
        ticks += 1;
        let pairs = role_pairs(&app, &chain);
        let shapes = shape_pairs(&app, &chain, handles.debris);
        let sample = q13::sample(&app, &handles);
        if pairs.tether_tether > 0 {
            match tether_tether_steps.last_mut() {
                Some((phase, steps)) if *phase == sample.state => *steps += 1,
                _ => tether_tether_steps.push((sample.state.clone(), 1)),
            }
        }
        if pairs.node_debris > 0 && first_node_contact.is_none() {
            first_node_contact = Some(sample.step);
        }
        if pairs.root_debris + pairs.tail_debris + pairs.node_debris > 0
            && first_tether_debris_contact.is_none()
        {
            first_tether_debris_contact = Some(sample.step);
        }
        shape_steps.node_bus += usize::from(shapes.node_bus > 0);
        shape_steps.node_array += usize::from(shapes.node_array > 0);
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
    let debris_rotation = app
        .world()
        .get::<Rotation>(handles.debris)
        .expect("Rotation")
        .0;
    let boxes = debris_world_boxes(&mut app, handles.debris);
    let wall = started.elapsed().as_secs_f64();
    println!(
        "tether_to_capture_contacts: {ticks} ticks to '{state}' at step {}, range {} m; \
         transitions {transitions:?}; first tether-debris touching step \
         {first_tether_debris_contact:?}; first node-debris touching step \
         {first_node_contact:?}; peak pairs {peak:?}; pair-steps summed {totals:?}; \
         steps with a node-bus / node-array pair {} / {}; steps with a tether-tether \
         pair per phase {tether_tether_steps:?}; at the capture transition the root is \
         {root_clearance:.4} m and the tail {tail_clearance:.4} m clear of the debris; \
         wall {wall:.1} s ({:.3} ms/step)",
        end.step,
        end.range_m,
        shape_steps.node_bus,
        shape_steps.node_array,
        1000.0 * wall / ticks as f64,
    );
    println!("debris rotation at the capture transition {debris_rotation:?}");
    for (what, centre, half, aabb_centre, aabb_half) in &boxes {
        println!(
            "debris {what} at the capture transition, relative to the debris centre: \
             centre {centre} half {half} m (ColliderAabb: centre {aabb_centre} half {aabb_half} m)"
        );
    }
    assert_eq!(state, "capture", "the run reached the capture phase");
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
    assert_eq!(
        debris_rotation,
        DQuat::IDENTITY,
        "no torque on the debris before the capture"
    );
    for ((what, centre, half, _, _), b) in
        boxes
            .iter()
            .zip([DEBRIS_BUS, DEBRIS_ARRAY_A, DEBRIS_ARRAY_B])
    {
        assert!(
            (*centre - b.centre).abs().max_element() < 1e-6,
            "{what}: centre {centre} against {}",
            b.centre
        );
        let want = rotated_half_extents(&b);
        assert!(
            (*half - want).abs().max_element() < 1e-6,
            "{what}: half-extents {half} against {want}"
        );
    }
}
