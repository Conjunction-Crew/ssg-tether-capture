//! Tests for `tether.rs` (P5 S6): the tether chain, the debris body and the
//! system that spawns both from the selection.
//!
//! `spawn_tether` and `spawn_debris` take `Commands`; the tests run them through a
//! one-shot system, which applies the commands before returning.

use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::DVec3;
use bevy::prelude::*;

use crate::anchor::SimAnchor;
use crate::capture::plan::{
    CapturePlanDevice, CapturePlanLibrary, SpawnOrientation, capture_plan_warnings,
    validate_capture_plan,
};
use crate::orbit::cache::OrbitalCache;
use crate::orbit::lvlh::LvlhFrame;
use crate::orbit::selection::{OrbitalSelectionState, SelectedOrbitalObject};
use crate::orbit::{Orbit, Orbital};
use crate::physics_bubble::PHYSICS_DISABLE_RADIUS;
use crate::project::SelectedProject;
use crate::telemetry::LogLevel;
use crate::test_support::{
    LogTap, assert_close, enter_sim, fixed_epoch, insert_plan_json, load_example_plans, q13,
    start_running, test_app, tick,
};
use crate::tether::{
    DEBRIS_LAYER, DebrisBody, TETHER_FILTERS, TETHER_LAYER, TetherNode, TetherRoot, TetherShape,
    chain_from_joints, spawn_debris, spawn_selected_bodies, spawn_tether,
};
use crate::time::ManualPhysics;
use brahe::utils::DOrbitStateProvider;

/// A plan whose `device` block (if any) is `device_json`, with tether `tether`.
fn plan_with(tether: &str, device_json: Option<&str>) -> String {
    let device = device_json
        .map(|d| format!(r#""device": {d},"#))
        .unwrap_or_default();
    format!(
        r#"{{"name": "p", "tether": "{tether}", {device}
  "states": [{{"id": "s", "parameters": {{"max_velocity": 1.0, "max_force": 1.0}}}}]}}"#
    )
}

fn spawn_app() -> App {
    let mut app = test_app();
    app.insert_resource(OrbitalCache::default());
    app
}

fn hst() -> SelectedOrbitalObject {
    q13::catalog_pick("HST", q13::HST_NORAD_ID)
}

/// Runs `spawn_tether` for `selected` with project `project_id`, laid along +Y;
/// returns its result as a string.
fn run_spawn_tether(
    app: &mut App,
    project_id: Option<&str>,
    selected: SelectedOrbitalObject,
) -> Result<(), String> {
    app.world_mut().resource_mut::<SelectedProject>().project_id = project_id.map(str::to_string);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut cache: ResMut<OrbitalCache>,
                  project: Res<SelectedProject>,
                  library: Res<CapturePlanLibrary>| {
                spawn_tether(
                    &mut commands,
                    &mut cache,
                    &project,
                    &library,
                    &selected,
                    DVec3::Y,
                )
                .map_err(|e| e.to_string())
            },
        )
        .expect("the spawn system failed to run")
}

fn count<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<Entity, F>()
        .iter(app.world())
        .count()
}

fn y_of(app: &App, entity: Entity) -> f64 {
    app.world().get::<Transform>(entity).unwrap().translation.y as f64
}

/// The registered `[root, tail]` of tether `name`.
fn registered(app: &App, name: &str) -> Vec<Entity> {
    app.world()
        .resource::<OrbitalCache>()
        .tethers
        .get(name)
        .unwrap_or_else(|| panic!("tether '{name}' is not registered"))
        .clone()
}

// ---------------------------------------------------------------------------
// spawn_tether
// ---------------------------------------------------------------------------

/// Every distance joint's `(min, max)` limit, in query order.
fn joint_limits(app: &mut App) -> Vec<(f64, f64)> {
    let world = app.world_mut();
    world
        .query::<&DistanceJoint>()
        .iter(world)
        .map(|j| (j.limits.min, j.limits.max))
        .collect()
}

#[test]
fn spawn_tether_without_a_project_builds_the_default_20_m_tether() {
    let mut app = spawn_app();

    run_spawn_tether(&mut app, None, hst()).expect("spawn failed");

    let ends = registered(&app, "Tether1");
    assert_eq!(ends.len(), 2, "the cache holds the root and the tail");
    let (root, tail) = (ends[0], ends[1]);
    assert!(app.world().get::<TetherRoot>(root).is_some());
    assert!(
        app.world().get::<SimAnchor>(root).is_some(),
        "the root is the anchor"
    );
    assert!(app.world().get::<TetherNode>(tail).is_some());
    assert_eq!(app.world().get::<TetherNode>(tail).unwrap().root, root);
    assert_close(y_of(&app, root), 0.0, 0.0, "root y");
    assert_close(y_of(&app, tail), 20.0, 1e-3, "tail y = the tether length");
    // F-66 (P7.5 S2): 19 m of rope at 0.25 m: 76 intervals, 75 interior nodes
    // plus the tail node (the rod: 190 + 1).
    assert_eq!(count::<With<TetherNode>>(&mut app), 76);
    assert_eq!(
        count::<With<DistanceJoint>>(&mut app),
        76,
        "one joint per node"
    );
    assert_eq!(count::<With<TetherRoot>>(&mut app), 1);
}

#[test]
fn spawn_tether_default_rope_is_locked_point_masses_on_slack_joints_that_touch_each_other_and_the_debris()
 {
    // F-66 (P7.5 S2, design D1, D2; S6, design D13): 77 bodies from the root along
    // the joints; centre-to-centre joints slack below the spawn distance; every
    // tether body rotation-locked (the nodes are point masses; the root and the tail
    // hold their attitude) and on the tether layer, filtering the tether and the
    // debris (self-contact on).
    let mut app = spawn_app();
    run_spawn_tether(&mut app, None, hst()).expect("spawn failed");

    let ends = registered(&app, "Tether1");
    let world = app.world_mut();
    let joints: Vec<DistanceJoint> = world
        .query::<&DistanceJoint>()
        .iter(world)
        .cloned()
        .collect();
    let chain = chain_from_joints(ends[0], joints.iter());
    assert_eq!(chain.len(), 77, "root + 75 nodes + tail");
    assert_eq!((chain[0], chain[76]), (ends[0], ends[1]));
    let y = |e: Entity| world.get::<Transform>(e).unwrap().translation;
    for joint in &joints {
        assert_eq!(joint.limits.min, 0.0, "slack below the limit");
        assert_close(
            joint.limits.max,
            y(joint.body2).distance(y(joint.body1)) as f64,
            1e-6,
            "the limit is the spawn distance",
        );
        assert_eq!(
            (joint.anchor1, joint.anchor2),
            (JointAnchor::ZERO, JointAnchor::ZERO),
            "centre to centre"
        );
    }
    let layers = CollisionLayers::new(TETHER_LAYER, TETHER_LAYER | DEBRIS_LAYER);
    assert_eq!(TETHER_FILTERS, TETHER_LAYER | DEBRIS_LAYER);
    for (i, &body) in chain.iter().enumerate() {
        assert_eq!(
            world.get::<CollisionLayers>(body),
            Some(&layers),
            "body {i}"
        );
        let locked = world
            .get::<LockedAxes>(body)
            .is_some_and(|axes| axes.is_rotation_locked());
        assert!(locked, "every tether body is rotation-locked: body {i}");
    }
    let tail_mass = world.get::<Mass>(chain[76]).unwrap().0;
    let mass: f32 = chain.iter().map(|&e| world.get::<Mass>(e).unwrap().0).sum();
    assert_close(mass as f64, 22.75, 1e-4, "2 + 75 x 0.25 + 2 kg (1 kg/m)");
    assert_close(tail_mass as f64, 2.0, 1e-6, "tail mass");
}

#[test]
fn spawn_tether_uses_the_plans_device_length_and_tether_name() {
    let mut app = spawn_app();
    insert_plan_json(
        &mut app,
        "short",
        &plan_with("Rope7", Some(r#"{"type": "tether", "tether_length": 5.0}"#)),
    );

    run_spawn_tether(&mut app, Some("short"), hst()).expect("spawn failed");

    let ends = registered(&app, "Rope7");
    assert_close(
        y_of(&app, ends[1]),
        5.0,
        1e-3,
        "tail y = the plan's tether length",
    );
    assert!(
        !app.world()
            .resource::<OrbitalCache>()
            .tethers
            .contains_key("Tether1"),
        "the plan's tether name replaces the default"
    );
    assert_eq!(
        count::<With<TetherNode>>(&mut app),
        16,
        "4 m of rope at 0.25 m: 15 interior nodes, + tail"
    );
}

#[test]
fn spawn_tether_falls_back_to_defaults_for_a_missing_device_a_zero_length_and_an_empty_name() {
    for (case, tether, device) in [
        ("no device", "Named", None),
        (
            "zero length",
            "Named",
            Some(r#"{"type": "tether", "tether_length": 0.0}"#),
        ),
        (
            "negative length",
            "Named",
            Some(r#"{"type": "tether", "tether_length": -3.0}"#),
        ),
    ] {
        let mut app = spawn_app();
        insert_plan_json(&mut app, "p", &plan_with(tether, device));

        run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");

        let ends = registered(&app, "Named");
        assert_close(
            y_of(&app, ends[1]),
            20.0,
            1e-3,
            &format!("{case}: default length"),
        );
    }

    // F-43 (P6): an empty tether name (a plan the validator rejects, but
    // `insert_plan_json` does not validate) is registered under the plan's own id,
    // the one the sidebar looks up; "Tether1" is only for no active plan (it fell
    // back to "Tether1" before P6).
    let mut app = spawn_app();
    insert_plan_json(&mut app, "p", &plan_with("", None));
    run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");
    assert_eq!(registered(&app, "").len(), 2);
    assert!(
        !app.world()
            .resource::<OrbitalCache>()
            .tethers
            .contains_key("Tether1")
    );
}

#[test]
fn spawn_tether_with_an_unknown_project_id_uses_the_defaults() {
    let mut app = spawn_app();

    run_spawn_tether(&mut app, Some("not_a_plan"), hst()).expect("spawn failed");

    assert_close(
        y_of(&app, registered(&app, "Tether1")[1]),
        20.0,
        1e-3,
        "tail y",
    );
}

#[test]
fn spawn_tether_chain_is_evenly_spaced_with_the_stated_shapes_and_masses() {
    let mut app = spawn_app();
    insert_plan_json(
        &mut app,
        "p",
        &plan_with("T", Some(r#"{"type": "tether", "tether_length": 3.0}"#)),
    );

    run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");

    let ends = registered(&app, "T");
    let (root, tail) = (ends[0], ends[1]);
    let world = app.world_mut();
    let mut nodes: Vec<(f64, TetherShape, f64)> = world
        .query::<(&Transform, &TetherShape, &Mass, &TetherNode)>()
        .iter(world)
        .map(|(t, s, m, _)| (t.translation.y as f64, *s, m.0 as f64))
        .collect();
    nodes.sort_by(|a, b| a.0.total_cmp(&b.0));
    // F-66 (P7.5 S2): 2 m of rope at 0.25 m: 8 intervals, 7 interior point
    // masses, then the tail sphere.
    let (tail_node, interior) = nodes.split_last().expect("no nodes");
    assert_eq!(tail_node.1, TetherShape::Sphere { radius: 0.5 });
    assert_close(tail_node.2, 2.0, 1e-6, "tail mass");
    assert_close(tail_node.0, 3.0, 1e-3, "tail y");
    assert_eq!(interior.len(), 7);
    for (_, shape, mass) in interior {
        assert_eq!(*shape, TetherShape::Sphere { radius: 0.25 / 8.0 });
        assert_close(*mass, 0.25, 1e-6, "interior node mass");
    }
    let interval = (3.0 - 1.0) / 8.0;
    let root_mass = world.get::<Mass>(root).unwrap().0 as f64;
    assert_close(root_mass, 2.0, 1e-6, "root mass");
    assert_eq!(
        *world.get::<TetherShape>(root).unwrap(),
        TetherShape::Sphere { radius: 0.5 }
    );
    assert!(world.get::<RigidBody>(tail).unwrap().is_dynamic());
    // Consecutive nodes are one interval apart (centre to centre); the first and
    // the last are an end-sphere radius further from the root and the tail.
    let ys: Vec<f64> = std::iter::once(0.0)
        .chain(interior.iter().map(|n| n.0))
        .chain(std::iter::once(tail_node.0))
        .collect();
    assert_close(ys[1], 0.5 + interval, 1e-6, "root to the first node");
    assert_close(
        ys[8] - ys[7],
        interval + 0.5,
        1e-6,
        "the last node to the tail",
    );
    for pair in ys[1..8].windows(2) {
        assert_close(pair[1] - pair[0], interval, 1e-6, "interior spacing");
    }
}

#[test]
fn spawn_tether_of_one_metre_has_only_the_two_end_spheres() {
    let mut app = spawn_app();
    insert_plan_json(
        &mut app,
        "p",
        &plan_with("T", Some(r#"{"type": "tether", "tether_length": 1.0}"#)),
    );

    run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");

    assert_eq!(count::<With<TetherNode>>(&mut app), 1, "just the tail");
    assert_eq!(count::<With<DistanceJoint>>(&mut app), 1);
    assert_close(y_of(&app, registered(&app, "T")[1]), 1.0, 1e-3, "tail y");
    assert_eq!(
        joint_limits(&mut app),
        [(0.0, 1.0)],
        "root to tail at the length"
    );
}

#[test]
fn spawn_tether_shorter_than_two_end_spheres_overlaps_them() {
    let mut app = spawn_app();
    insert_plan_json(
        &mut app,
        "p",
        &plan_with("T", Some(r#"{"type": "tether", "tether_length": 0.5}"#)),
    );

    run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");

    // No interior nodes (one interval, `max(1)`); the tail's centre is at the
    // plan's length, 0.5 m from the root, although both spheres have a 0.5 m
    // radius, and the one joint's limit is that length.
    assert_eq!(count::<With<TetherNode>>(&mut app), 1);
    assert_close(y_of(&app, registered(&app, "T")[1]), 0.5, 1e-3, "tail y");
    assert_eq!(joint_limits(&mut app), [(0.0, 0.5)]);
}

#[test]
fn spawn_tether_starts_the_root_from_sgp4_for_a_catalog_pick() {
    let mut app = spawn_app();
    let pick = hst();
    let expected_gp = pick.gp.clone().expect("a catalog pick carries its record");

    run_spawn_tether(&mut app, None, pick).expect("spawn failed");

    let root = registered(&app, "Tether1")[0];
    match app
        .world()
        .get::<Orbit>(root)
        .expect("the root has an Orbit")
    {
        Orbit::FromGp(gp) => assert_eq!(gp.epoch, expected_gp.epoch),
        other => panic!("expected Orbit::FromGp, got {other:?}"),
    }
}

#[test]
fn spawn_tether_starts_an_edited_selection_from_its_elements_at_their_epoch() {
    let mut app = spawn_app();
    let mut pick = hst();
    pick.gp = None;
    pick.elements.semi_major_axis_m += 1000.0;
    let expected = pick.elements.to_vec6();
    let epoch = pick.elements.epoch;

    run_spawn_tether(&mut app, None, pick).expect("spawn failed");

    let root = registered(&app, "Tether1")[0];
    match app
        .world()
        .get::<Orbit>(root)
        .expect("the root has an Orbit")
    {
        Orbit::FromElementsAt {
            elements,
            epoch: at,
        } => {
            assert_eq!(*elements, expected);
            assert_eq!(*at, epoch);
        }
        other => panic!("expected Orbit::FromElementsAt, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// spawn_debris
// ---------------------------------------------------------------------------

#[test]
fn spawn_debris_registers_a_disabled_dynamic_body_outside_the_bubble() {
    let mut app = spawn_app();
    let pick = hst();

    let result = app
        .world_mut()
        .run_system_once(
            move |mut commands: Commands, mut cache: ResMut<OrbitalCache>| {
                spawn_debris(&mut commands, &mut cache, &pick).map_err(|e| e.to_string())
            },
        )
        .expect("the spawn system failed to run");

    assert_eq!(result, Ok(()));
    let debris = *app
        .world()
        .resource::<OrbitalCache>()
        .debris
        .get("Satellite1")
        .expect("the debris is registered as Satellite1");
    let world = app.world();
    assert_eq!(
        world.get::<DebrisBody>(debris).unwrap().model_path,
        "models/broken_satellite.glb"
    );
    assert!(world.get::<RigidBody>(debris).unwrap().is_dynamic());
    assert!(
        world.get::<RigidBodyDisabled>(debris).is_some(),
        "the debris starts outside the physics bubble"
    );
    assert_close(
        world.get::<Mass>(debris).unwrap().0 as f64,
        2500.0,
        1e-3,
        "mass",
    );
    let at = world.get::<Transform>(debris).unwrap().translation;
    for axis in [at.x, at.y, at.z] {
        assert_close(
            axis as f64,
            PHYSICS_DISABLE_RADIUS + 10.0,
            1e-3,
            "parked 10 m beyond the disable radius on each axis",
        );
    }
    assert!(matches!(world.get::<Orbit>(debris), Some(Orbit::FromGp(_))));
}

// ---------------------------------------------------------------------------
// spawn_selected_bodies
// ---------------------------------------------------------------------------

fn run_spawn_selected(app: &mut App) {
    app.world_mut()
        .run_system_once(spawn_selected_bodies)
        .expect("spawn_selected_bodies failed to run");
}

fn select(
    app: &mut App,
    target: Option<SelectedOrbitalObject>,
    chaser: Option<SelectedOrbitalObject>,
) {
    let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
    selection.target = target;
    selection.chaser = chaser;
}

#[test]
fn spawn_selected_bodies_without_a_chaser_spawns_nothing() {
    let mut app = spawn_app();
    select(&mut app, Some(hst()), None);

    run_spawn_selected(&mut app);

    assert_eq!(count::<With<TetherRoot>>(&mut app), 0);
    assert_eq!(count::<With<DebrisBody>>(&mut app), 0);
    assert!(app.world().resource::<OrbitalCache>().tethers.is_empty());
    assert!(app.world().resource::<OrbitalCache>().debris.is_empty());
}

#[test]
fn spawn_selected_bodies_without_a_target_spawns_nothing() {
    let mut app = spawn_app();
    select(&mut app, None, Some(hst()));

    run_spawn_selected(&mut app);

    assert_eq!(
        count::<With<TetherRoot>>(&mut app),
        0,
        "not even the chaser's tether"
    );
    assert_eq!(count::<With<DebrisBody>>(&mut app), 0);
}

#[test]
fn spawn_selected_bodies_spawns_the_chasers_tether_and_the_targets_debris() {
    let mut app = spawn_app();
    insert_plan_json(
        &mut app,
        "p",
        &plan_with("Rope", Some(r#"{"type": "tether", "tether_length": 2.0}"#)),
    );
    app.world_mut().resource_mut::<SelectedProject>().project_id = Some("p".to_string());
    let target = q13::catalog_pick("HST", q13::HST_NORAD_ID);
    let chaser = q13::catalog_pick("CHASER", q13::CHASER_NORAD_ID);
    let chaser_epoch = chaser.gp.as_ref().expect("catalog pick").epoch;
    select(&mut app, Some(target), Some(chaser));

    run_spawn_selected(&mut app);

    let root = registered(&app, "Rope")[0];
    match app.world().get::<Orbit>(root).unwrap() {
        Orbit::FromGp(gp) => assert_eq!(gp.epoch, chaser_epoch, "the tether root is the chaser"),
        other => panic!("expected Orbit::FromGp, got {other:?}"),
    }
    assert_eq!(count::<With<DebrisBody>>(&mut app), 1);
    let cache = app.world().resource::<OrbitalCache>();
    assert!(cache.debris.contains_key("Satellite1"));
    // P7.5 S3: laid along the spawn orientation, not +Y; the root is at the origin.
    let tail = registered(&app, "Rope")[1];
    let tail_distance = app
        .world()
        .get::<Transform>(tail)
        .unwrap()
        .translation
        .length();
    assert_close(
        f64::from(tail_distance),
        2.0,
        1e-3,
        "tail distance from the root",
    );
}

// ---------------------------------------------------------------------------
// P6 S4: the plan's physical parameters reach the spawned bodies
// ---------------------------------------------------------------------------

/// A v1 plan whose `device` block is `device`.
fn plan_v1(device: serde_json::Value) -> String {
    serde_json::json!({
        "schema_version": 1,
        "name": "p",
        "tether": "T",
        "device": device,
        "phases": [{"id": "s", "max_velocity_mps": 1.0, "max_force_n": 1.0}]
    })
    .to_string()
}

/// Every tether body as `(y, mass)` sorted by y, and every joint's `(linear, angular)` damping.
fn tether_bodies(app: &mut App) -> (Vec<(f64, f64)>, Vec<(f64, f64)>) {
    let world = app.world_mut();
    let mut bodies: Vec<(f64, f64)> = world
        .query_filtered::<(&Transform, &Mass), Or<(With<TetherRoot>, With<TetherNode>)>>()
        .iter(world)
        .map(|(t, m)| (t.translation.y as f64, m.0 as f64))
        .collect();
    bodies.sort_by(|a, b| a.0.total_cmp(&b.0));
    let damping = world
        .query::<&JointDamping>()
        .iter(world)
        .map(|d| (d.linear, d.angular))
        .collect();
    (bodies, damping)
}

#[test]
fn spawn_tether_applies_the_plans_spacing_masses_and_damping() {
    let mut app = spawn_app();
    insert_plan_json(
        &mut app,
        "p",
        &plan_v1(serde_json::json!({
            "tether_length_m": 11.0,
            "segment_spacing_m": 0.5,
            "segment_mass_kg": 0.3,
            "end_mass_kg": 4.0,
            "joint_damping_linear": 2.5,
            "joint_damping_angular": 0.75
        })),
    );

    run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");

    let (bodies, damping) = tether_bodies(&mut app);
    // F-66 (P7.5 S2): 10 m of rope at 0.5 m: 20 intervals, 19 interior nodes,
    // plus the root and the tail.
    assert_eq!(bodies.len(), 21, "root + 19 interior + tail");
    assert_close(bodies[0].1, 4.0, 1e-6, "root mass");
    assert_close(bodies[20].1, 4.0, 1e-6, "tail mass");
    for (index, (_, mass)) in bodies[1..20].iter().enumerate() {
        assert_close(*mass, 0.3, 1e-6, &format!("interior node {index} mass"));
    }
    assert_close(bodies[20].0, 11.0, 1e-3, "tail y = the tether length");
    for pair in bodies[1..20].windows(2) {
        assert_close(pair[1].0 - pair[0].0, 0.5, 1e-6, "interval");
    }
    assert_eq!(damping.len(), 20, "one joint per node");
    // C1: the plan's linear damping, and no angular damping whatever it says.
    for (linear, angular) in damping {
        assert_eq!((linear, angular), (2.5, 0.0));
    }
}

#[test]
fn a_plan_that_spells_out_the_defaults_spawns_the_same_tether_as_a_plan_without_a_device() {
    let spawn = |device: Option<serde_json::Value>| {
        let mut app = spawn_app();
        let plan = match device {
            Some(device) => plan_v1(device),
            None => plan_with("T", None),
        };
        insert_plan_json(&mut app, "p", &plan);
        run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");
        let (bodies, damping) = tether_bodies(&mut app);
        assert_eq!(bodies.len(), 77, "root + 75 interior + tail");
        (bodies, damping)
    };

    let literal = spawn(None);
    let spelled_out = spawn(Some(serde_json::json!({
        "type": "tether",
        "tether_length_m": 20.0,
        "segment_spacing_m": 0.25,
        "segment_mass_kg": 0.25,
        "end_mass_kg": 2.0,
        "joint_damping_linear": 1.0,
        "joint_damping_angular": 1.0
    })));

    assert_eq!(
        literal, spelled_out,
        "the plan's defaults are the spawner's (F-66, P7.5 S2: 0.25 m, 0.25 kg)"
    );
    assert!(
        literal.1.iter().all(|d| *d == (1.0, 0.0)),
        "the default linear joint damping is 1.0; the angular term is 0 on the rope (C1)"
    );
}

#[test]
fn spawn_tether_builds_as_many_bodies_as_the_validator_counts() {
    // The 1,000-body limit (error) and the 200-body warning are computed from the
    // plan; the spawner must build exactly that many.
    // F-66 (P7.5 S2): max(1, round(rope / spacing)) intervals, one body more.
    for (length, spacing, bodies) in [
        (100.5, 0.5, 200),
        (101.0, 0.5, 201),
        (500.5, 0.5, 1000),
        (20.0, 0.25, 77),
        (20.0, 0.1, 191),
        (20.1, 0.25, 77),
        (1.0, 0.25, 2),
        (0.5, 0.25, 2),
    ] {
        let mut app = spawn_app();
        let document = plan_v1(serde_json::json!({
            "tether_length_m": length, "segment_spacing_m": spacing
        }));
        insert_plan_json(&mut app, "p", &document);
        let plan = app
            .world()
            .resource::<CapturePlanLibrary>()
            .plans
            .get("p")
            .unwrap()
            .clone();
        assert_eq!(validate_capture_plan("p", &plan), Vec::<String>::new());
        let warned = !capture_plan_warnings("p", &plan).is_empty();
        assert_eq!(warned, bodies > 200, "{length} m at {spacing} m");

        run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");

        assert_eq!(
            tether_bodies(&mut app).0.len(),
            bodies,
            "{length} m at {spacing} m"
        );
    }
    // One more is refused by the validator and never reaches the spawner.
    let mut app = spawn_app();
    insert_plan_json(
        &mut app,
        "p",
        &plan_v1(serde_json::json!({"tether_length_m": 501.0, "segment_spacing_m": 0.5})),
    );
    let plan = app
        .world()
        .resource::<CapturePlanLibrary>()
        .plans
        .get("p")
        .unwrap()
        .clone();
    assert_eq!(
        validate_capture_plan("p", &plan),
        ["[p] /device/segment_spacing_m: gives 1001 tether bodies; at most 1000 are allowed"]
    );
}

#[test]
fn spawn_tether_falls_back_to_the_default_spacing_for_a_plan_the_validator_rejects() {
    // `insert_plan_json` does not validate; a spacing of 0 or less must not
    // divide by zero.
    for spacing in [0.0, -0.1] {
        let mut app = spawn_app();
        insert_plan_json(
            &mut app,
            "p",
            &plan_v1(serde_json::json!({"segment_spacing_m": spacing})),
        );

        run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");

        assert_eq!(tether_bodies(&mut app).0.len(), 77, "spacing {spacing}");
    }
}

#[test]
fn spawn_debris_takes_the_targets_mass_from_the_selection() {
    let spawn = |mass_kg: Option<f64>| {
        let mut app = spawn_app();
        let mut pick = hst();
        pick.mass_kg = mass_kg;
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut cache: ResMut<OrbitalCache>| {
                    spawn_debris(&mut commands, &mut cache, &pick).map_err(|e| e.to_string())
                },
            )
            .expect("the spawn system failed to run")
            .expect("spawn failed");
        let debris = *app
            .world()
            .resource::<OrbitalCache>()
            .debris
            .get("Satellite1")
            .unwrap();
        app.world().get::<Mass>(debris).unwrap().0 as f64
    };

    assert_close(spawn(None), 2500.0, 1e-3, "default target mass");
    assert_close(spawn(Some(3200.0)), 3200.0, 1e-3, "mass from project.json");
}

#[test]
fn spawn_selected_bodies_gives_the_debris_the_targets_mass_and_not_the_tether() {
    let mut app = spawn_app();
    let mut target = q13::catalog_pick("HST", q13::HST_NORAD_ID);
    target.mass_kg = Some(3200.0);
    let chaser = q13::catalog_pick("CHASER", q13::CHASER_NORAD_ID);
    select(&mut app, Some(target), Some(chaser));

    run_spawn_selected(&mut app);

    let debris = *app
        .world()
        .resource::<OrbitalCache>()
        .debris
        .get("Satellite1")
        .unwrap();
    assert_close(
        app.world().get::<Mass>(debris).unwrap().0 as f64,
        3200.0,
        1e-3,
        "debris mass",
    );
    let root = registered(&app, "Tether1")[0];
    assert_close(
        app.world().get::<Mass>(root).unwrap().0 as f64,
        2.0,
        1e-6,
        "the tether root keeps the plan's end mass",
    );
}

/// A symmetric, positive-definite tensor with off-diagonal terms (kg m²).
const CHASER_TENSOR: [[f64; 3]; 3] = [[500.0, 10.0, -5.0], [10.0, 400.0, 3.0], [-5.0, 3.0, 300.0]];

/// The six independent entries of avian's computed angular inertia tensor.
fn computed_tensor(app: &App, entity: Entity) -> [f64; 6] {
    let t = app
        .world()
        .get::<ComputedAngularInertia>(entity)
        .expect("ComputedAngularInertia")
        .tensor();
    [t.m00, t.m01, t.m02, t.m11, t.m12, t.m22]
}

#[track_caller]
fn assert_tensor(actual: [f64; 6], expected: &[[f64; 3]; 3], what: &str) {
    let e = expected;
    let expected = [e[0][0], e[0][1], e[0][2], e[1][1], e[1][2], e[2][2]];
    for (a, b) in actual.iter().zip(expected) {
        // The tensor goes through f32 and avian's eigen-decomposition.
        assert_close(*a, b, 1e-3 * 500.0, what);
    }
}

/// A storage check (P7.5 S6, C9, E18): the root is the chaser (design D8). With a
/// stated mass and tensor the root carries them (Q10 row 11); the tail keeps the
/// plan's end mass. The root is `LockedAxes::ROTATION_LOCKED` (design D13), so its
/// `AngularInertia` is stored and inert until an attitude model exists (P12): this
/// test checks what is stored, not the dynamics.
#[test]
fn the_chaser_inertia_tensor_is_stored_on_the_locked_root() {
    let mut app = spawn_app();
    let mut chaser = q13::catalog_pick("CHASER", q13::CHASER_NORAD_ID);
    chaser.mass_kg = Some(500.0);
    chaser.inertia_kg_m2 = Some(CHASER_TENSOR);
    run_spawn_tether(&mut app, None, chaser).expect("spawn failed");
    // Avian folds a stated tensor in at the next physics step's Prepare set.
    app.world_mut().run_schedule(ManualPhysics);

    let nodes = registered(&app, "Tether1");
    let (root, tail) = (nodes[0], nodes[1]);
    let world = app.world();
    assert_close(
        world.get::<Mass>(root).unwrap().0 as f64,
        500.0,
        1e-3,
        "root Mass",
    );
    assert_close(
        world.get::<ComputedMass>(root).unwrap().value(),
        500.0,
        1e-3,
        "root ComputedMass",
    );
    assert_tensor(computed_tensor(&app, root), &CHASER_TENSOR, "root inertia");
    assert!(
        world
            .get::<LockedAxes>(root)
            .is_some_and(|axes| axes.is_rotation_locked()),
        "the root holds its attitude: the tensor is stored and inert"
    );
    assert_eq!(
        app.world().get::<Rotation>(root).unwrap().0,
        bevy::math::DQuat::IDENTITY,
        "the tensor is in the sim's axes: the root keeps the identity rotation"
    );
    assert_close(
        app.world().get::<Mass>(tail).unwrap().0 as f64,
        2.0,
        1e-6,
        "tail mass",
    );

    // Without them: the end mass and the sphere's inertia scaled to it (0.4 m r²).
    let mut app = spawn_app();
    run_spawn_tether(
        &mut app,
        None,
        q13::catalog_pick("CHASER", q13::CHASER_NORAD_ID),
    )
    .expect("spawn failed");
    app.update();
    let root = registered(&app, "Tether1")[0];
    assert_close(
        app.world().get::<Mass>(root).unwrap().0 as f64,
        2.0,
        1e-6,
        "default root mass",
    );
    let sphere = 0.4 * 2.0 * 0.5 * 0.5;
    assert_tensor(
        computed_tensor(&app, root),
        &[[sphere, 0.0, 0.0], [0.0, sphere, 0.0], [0.0, 0.0, sphere]],
        "default root inertia",
    );
}

#[test]
fn spawn_debris_takes_the_targets_inertia_from_the_selection() {
    // P7.5 S6 (C1, E21): with a manifest tensor the debris's computed inertia is
    // that tensor, not the tensor plus the array children's (the debris carries
    // `NoAutoAngularInertia`); its mass and centre are the stated ones too.
    let mut app = spawn_app();
    let mut pick = hst();
    pick.inertia_kg_m2 = Some(CHASER_TENSOR);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands, mut cache: ResMut<OrbitalCache>| {
                spawn_debris(&mut commands, &mut cache, &pick).map_err(|e| e.to_string())
            },
        )
        .expect("the spawn system failed to run")
        .expect("spawn failed");
    app.update();
    // Avian recomputes the mass properties with the children attached.
    app.world_mut().run_schedule(ManualPhysics);
    let debris = app.world().resource::<OrbitalCache>().debris["Satellite1"];
    let world = app.world_mut();
    let attached = world
        .query::<&ColliderOf>()
        .iter(world)
        .filter(|of| of.body == debris)
        .count();
    assert_eq!(attached, 3, "the bus and the two array children");
    let world = app.world();
    assert!(world.get::<AngularInertia>(debris).is_some());
    assert!(world.get::<NoAutoAngularInertia>(debris).is_some());
    assert_tensor(
        computed_tensor(&app, debris),
        &CHASER_TENSOR,
        "debris inertia",
    );
    assert_close(
        world.get::<ComputedMass>(debris).unwrap().value(),
        2500.0,
        1e-3,
        "default mass",
    );
    assert!(
        world
            .get::<ComputedCenterOfMass>(debris)
            .unwrap()
            .0
            .length()
            < 1e-9
    );
}

// ---------------------------------------------------------------------------
// P7.5 S3: the spawn orientation (design D9)
// ---------------------------------------------------------------------------

/// The Q13 pair spawned at the fixed epoch with `example_plan`'s device set to
/// `orientation` (`None`: no key), the propagators built after one tick. Returns
/// the app, the chain root first, and the debris; also the log lines of the spawn.
fn q13_spawned(
    orientation: Option<SpawnOrientation>,
    chaser: SelectedOrbitalObject,
) -> (
    App,
    Vec<Entity>,
    Entity,
    Vec<(LogLevel, &'static str, String)>,
) {
    let mut app = test_app();
    load_example_plans(&mut app);
    if let Some(orientation) = orientation {
        app.world_mut()
            .resource_mut::<CapturePlanLibrary>()
            .plans
            .get_mut(q13::PLAN_ID)
            .expect("example_plan")
            .device = Some(CapturePlanDevice {
            spawn_orientation: Some(orientation),
            ..CapturePlanDevice::default()
        });
    }
    app.world_mut().resource_mut::<SelectedProject>().project_id = Some(q13::PLAN_ID.to_string());
    select(&mut app, Some(hst()), Some(chaser));
    enter_sim(&mut app);
    let mut tap = LogTap::new(app.world());
    start_running(&mut app);
    let logs = tap.drain(app.world());
    tick(&mut app);
    let root = registered(&app, "Tether1")[0];
    let world = app.world_mut();
    let joints: Vec<DistanceJoint> = world
        .query::<&DistanceJoint>()
        .iter(world)
        .cloned()
        .collect();
    let chain = chain_from_joints(root, joints.iter());
    let debris = app.world().resource::<OrbitalCache>().debris["Satellite1"];
    (app, chain, debris, logs)
}

fn q13_chaser() -> SelectedOrbitalObject {
    q13::catalog_pick("P0 HST CHASER", q13::CHASER_NORAD_ID)
}

fn position(app: &App, entity: Entity) -> DVec3 {
    app.world().get::<Position>(entity).expect("Position").0
}

/// Degrees between two directions.
fn degrees(a: DVec3, b: DVec3) -> f64 {
    a.angle_between(b).to_degrees()
}

#[test]
fn the_tail_trails_away_from_the_target_by_default() {
    for orientation in [None, Some(SpawnOrientation::TowardTarget)] {
        let (app, chain, debris, logs) = q13_spawned(orientation, q13_chaser());
        let root = position(&app, chain[0]);
        let angle = degrees(
            position(&app, chain[76]) - root,
            position(&app, debris) - root,
        );
        println!("{orientation:?}: angle(root->tail, root->target) {angle:.4} deg");
        assert!((angle - 180.0).abs() < 0.1, "{orientation:?}: {angle} deg");
        assert!(
            logs.iter().all(|(level, ..)| *level != LogLevel::Warn),
            "{logs:?}"
        );
        // Every body along the one axis, 20 m from the root to the tail.
        let axis = (position(&app, chain[76]) - root).normalize();
        for &body in &chain[1..] {
            let offset = position(&app, body) - root;
            assert!(
                (offset - axis * offset.dot(axis)).length() < 1e-3,
                "{body:?} off the axis"
            );
        }
        assert_close(
            (position(&app, chain[76]) - root).length(),
            20.0,
            1e-3,
            "root to tail",
        );
    }
}

#[test]
fn v_bar_and_r_bar_lay_the_tail_behind_and_below_the_chaser() {
    for (orientation, lvlh) in [
        (SpawnOrientation::VBar, DVec3::new(0.0, -1.0, 0.0)),
        (SpawnOrientation::RBar, DVec3::new(-1.0, 0.0, 0.0)),
    ] {
        let (app, chain, _, _) = q13_spawned(Some(orientation), q13_chaser());
        // The chaser's LVLH frame at the start epoch, from its propagator.
        let state = app
            .world()
            .get::<Orbital>(chain[0])
            .and_then(|orbital| orbital.propagator.as_ref())
            .expect("the root has a propagator")
            .state_eci(fixed_epoch())
            .expect("the start state");
        let frame = LvlhFrame::from_eci(&state).expect("a frame");
        let tail = position(&app, chain[76]) - position(&app, chain[0]);
        let angle = degrees(tail, frame.to_eci(lvlh));
        println!("{orientation:?}: {angle:.5} deg from the LVLH axis");
        assert!(angle < 0.1, "{orientation:?}: {angle} deg");
    }
}

#[test]
fn every_tether_body_keeps_the_identity_rotation_whatever_the_orientation() {
    // C3: the orientation lives in the positions only, so a stated chaser inertia
    // tensor stays in the sim's ECI-aligned axes.
    for orientation in [
        SpawnOrientation::TowardTarget,
        SpawnOrientation::VBar,
        SpawnOrientation::RBar,
    ] {
        let (app, chain, debris, _) = q13_spawned(Some(orientation), q13_chaser());
        for &body in chain.iter().chain([&debris]) {
            assert_eq!(
                app.world().get::<Rotation>(body).expect("Rotation").0,
                bevy::math::DQuat::IDENTITY,
                "{orientation:?}: {body:?}"
            );
        }
    }
}

#[test]
fn without_a_start_direction_the_tether_falls_back_to_plus_y_with_a_warning() {
    // A chaser at the target's own position gives no root-to-target direction.
    let (app, chain, _, logs) = q13_spawned(None, hst());
    let tail = position(&app, chain[76]) - position(&app, chain[0]);
    assert!(degrees(tail, DVec3::Y) < 1e-6, "{tail}");
    assert!(
        logs.iter()
            .any(|(level, source, message)| *level == LogLevel::Warn
                && *source == "tether"
                && message.contains("laid along +Y")),
        "{logs:?}"
    );
}
