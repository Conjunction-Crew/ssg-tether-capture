//! Tests for `tether.rs` (P5 S6): the tether chain, the debris body and the
//! system that spawns both from the selection.
//!
//! `spawn_tether` and `spawn_debris` take `Commands`; the tests run them through a
//! one-shot system, which applies the commands before returning.

use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

use crate::anchor::SimAnchor;
use crate::capture::plan::CapturePlanLibrary;
use crate::orbit::Orbit;
use crate::orbit::cache::OrbitalCache;
use crate::orbit::selection::{OrbitalSelectionState, SelectedOrbitalObject};
use crate::physics_bubble::PHYSICS_DISABLE_RADIUS;
use crate::project::SelectedProject;
use crate::tether::{
    DebrisBody, TetherNode, TetherRoot, TetherShape, spawn_debris, spawn_selected_bodies,
    spawn_tether,
};
use crate::test_support::{assert_close, insert_plan_json, q13, test_app};

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

/// Runs `spawn_tether` for `selected` with project `project_id`; returns its result as a string.
fn run_spawn_tether(
    app: &mut App,
    project_id: Option<&str>,
    selected: SelectedOrbitalObject,
) -> Result<(), String> {
    app.world_mut().resource_mut::<SelectedProject>().project_id =
        project_id.map(str::to_string);
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut cache: ResMut<OrbitalCache>,
                  project: Res<SelectedProject>,
                  library: Res<CapturePlanLibrary>| {
                spawn_tether(&mut commands, &mut cache, &project, &library, &selected)
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

#[test]
fn spawn_tether_without_a_project_builds_the_default_20_m_tether() {
    let mut app = spawn_app();

    run_spawn_tether(&mut app, None, hst()).expect("spawn failed");

    let ends = registered(&app, "Tether1");
    assert_eq!(ends.len(), 2, "the cache holds the root and the tail");
    let (root, tail) = (ends[0], ends[1]);
    assert!(app.world().get::<TetherRoot>(root).is_some());
    assert!(app.world().get::<SimAnchor>(root).is_some(), "the root is the anchor");
    assert!(app.world().get::<TetherNode>(tail).is_some());
    assert_eq!(app.world().get::<TetherNode>(tail).unwrap().root, root);
    assert_close(y_of(&app, root), 0.0, 0.0, "root y");
    assert_close(y_of(&app, tail), 20.0, 1e-3, "tail y = the tether length");
    // 19 m of interior at 0.1 m: 190 interior nodes plus the tail node.
    assert_eq!(count::<With<TetherNode>>(&mut app), 191);
    assert_eq!(count::<With<DistanceJoint>>(&mut app), 191, "one joint per node");
    assert_eq!(count::<With<TetherRoot>>(&mut app), 1);
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
    assert_close(y_of(&app, ends[1]), 5.0, 1e-3, "tail y = the plan's tether length");
    assert!(
        !app.world().resource::<OrbitalCache>().tethers.contains_key("Tether1"),
        "the plan's tether name replaces the default"
    );
    assert_eq!(count::<With<TetherNode>>(&mut app), 41, "4 m interior at 0.1 m, + tail");
}

#[test]
fn spawn_tether_falls_back_to_defaults_for_a_missing_device_a_zero_length_and_an_empty_name() {
    for (case, tether, device) in [
        ("no device", "Named", None),
        ("zero length", "Named", Some(r#"{"type": "tether", "tether_length": 0.0}"#)),
        ("negative length", "Named", Some(r#"{"type": "tether", "tether_length": -3.0}"#)),
    ] {
        let mut app = spawn_app();
        insert_plan_json(&mut app, "p", &plan_with(tether, device));

        run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");

        let ends = registered(&app, "Named");
        assert_close(y_of(&app, ends[1]), 20.0, 1e-3, &format!("{case}: default length"));
    }

    // An empty tether name (a plan the validator rejects, but `insert_plan_json`
    // does not validate) falls back to "Tether1".
    let mut app = spawn_app();
    insert_plan_json(&mut app, "p", &plan_with("", None));
    run_spawn_tether(&mut app, Some("p"), hst()).expect("spawn failed");
    assert_eq!(registered(&app, "Tether1").len(), 2);
}

#[test]
fn spawn_tether_with_an_unknown_project_id_uses_the_defaults() {
    let mut app = spawn_app();

    run_spawn_tether(&mut app, Some("not_a_plan"), hst()).expect("spawn failed");

    assert_close(y_of(&app, registered(&app, "Tether1")[1]), 20.0, 1e-3, "tail y");
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
    // 2 m interior at 0.1 m: 20 interior segments, all the same length, then the
    // tail sphere.
    let (tail_node, interior) = nodes.split_last().expect("no nodes");
    assert_eq!(tail_node.1, TetherShape::Sphere { radius: 0.5 });
    assert_close(tail_node.2, 2.0, 1e-6, "tail mass");
    assert_close(tail_node.0, 3.0, 1e-3, "tail y");
    let TetherShape::Cylinder { radius, length } = interior[0].1 else {
        panic!("the interior nodes are cylinders: {:?}", interior[0]);
    };
    assert_close(radius, 0.25 / 8.0, 1e-9, "rope radius");
    for (_, shape, mass) in interior {
        assert_eq!(*shape, TetherShape::Cylinder { radius, length });
        assert_close(*mass, 0.1, 1e-6, "interior node mass");
    }
    assert_close(
        length * interior.len() as f64,
        3.0 - 1.0,
        1e-9,
        "the interior segments span the length minus the two end spheres",
    );
    let root_mass = world.get::<Mass>(root).unwrap().0 as f64;
    assert_close(root_mass, 2.0, 1e-6, "root mass");
    assert_eq!(
        *world.get::<TetherShape>(root).unwrap(),
        TetherShape::Sphere { radius: 0.5 }
    );
    assert!(world.get::<RigidBody>(tail).unwrap().is_dynamic());
    // Consecutive nodes are one segment apart (centre to centre) with no gap.
    let ys: Vec<f64> = std::iter::once(0.0)
        .chain(interior.iter().map(|n| n.0))
        .collect();
    for pair in ys.windows(2).skip(1) {
        assert_close(pair[1] - pair[0], length, 1e-3, "interior spacing");
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

    // No interior nodes; the negative surface gap puts the tail's centre at the
    // plan's length, 0.5 m from the root, although both spheres have a 0.5 m radius.
    assert_eq!(count::<With<TetherNode>>(&mut app), 1);
    assert_close(y_of(&app, registered(&app, "T")[1]), 0.5, 1e-3, "tail y");
}

#[test]
fn spawn_tether_starts_the_root_from_sgp4_for_a_catalog_pick() {
    let mut app = spawn_app();
    let pick = hst();
    let expected_gp = pick.gp.clone().expect("a catalog pick carries its record");

    run_spawn_tether(&mut app, None, pick).expect("spawn failed");

    let root = registered(&app, "Tether1")[0];
    match app.world().get::<Orbit>(root).expect("the root has an Orbit") {
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
    match app.world().get::<Orbit>(root).expect("the root has an Orbit") {
        Orbit::FromElementsAt { elements, epoch: at } => {
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
    assert_close(world.get::<Mass>(debris).unwrap().0 as f64, 2500.0, 1e-3, "mass");
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

fn select(app: &mut App, target: Option<SelectedOrbitalObject>, chaser: Option<SelectedOrbitalObject>) {
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

    assert_eq!(count::<With<TetherRoot>>(&mut app), 0, "not even the chaser's tether");
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
    assert_close(y_of(&app, registered(&app, "Rope")[1]), 2.0, 1e-3, "tail y");
}
