//! Every example capture plan, loaded the way the app loads it and run on the Q13
//! pair (P5 S7).

use std::path::PathBuf;

use avian3d::prelude::Position;
use bevy::ecs::message::Messages;
use bevy::prelude::*;
use ssg_sim::capture::plan::{CapturePlanLibrary, validate_capture_plan};
use ssg_sim::capture::{CaptureComponent, SimCommand};
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::selection::OrbitalSelectionState;
use ssg_sim::project::SelectedProject;
use ssg_sim::test_support::{enter_sim, q13, repo_root, start_running, test_app, tick};

/// Ticks per plan: 10 s of sim time, enough to move the pair and apply forces,
/// cheap enough for the test budget.
const TICKS: u64 = 640;

fn example_dir() -> PathBuf {
    repo_root().join("assets/example_capture_plans")
}

fn empty_user_dir(tag: &str) -> ssg_sim::test_support::TempDir {
    ssg_sim::test_support::TempDir::new(tag)
}

fn example_plan_ids() -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(example_dir())
        .expect("the example plan directory exists")
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension()? == "json").then(|| path.file_stem()?.to_str().map(str::to_string))?
        })
        .collect();
    ids.sort();
    ids
}

#[test]
fn every_example_plan_loads_validates_and_compiles() {
    let ids = example_plan_ids();
    assert!(!ids.is_empty(), "the example directory has plans");
    let user = empty_user_dir("example-plans-load");
    let mut library = CapturePlanLibrary::default();

    let errors = library.load_all(&example_dir(), user.path());

    assert!(errors.is_empty(), "load errors: {errors:?}");
    for id in &ids {
        let plan = library
            .plans
            .get(id)
            .unwrap_or_else(|| panic!("plan `{id}` did not load"));
        assert_eq!(&plan.id, id, "the id is the file stem");
        let problems = validate_capture_plan(id, plan);
        assert!(
            problems.is_empty(),
            "`{id}` does not validate: {problems:?}"
        );
        let compiled = library
            .compiled_plans
            .get(id)
            .unwrap_or_else(|| panic!("plan `{id}` is not compiled"));
        for state in &plan.states {
            assert!(
                compiled.state(&state.id).is_some(),
                "`{id}`: compiled plan lacks state `{}`",
                state.id
            );
        }
    }
    assert_eq!(library.example_plans.len(), ids.len());
}

#[test]
fn every_example_plan_runs_on_the_q13_pair_without_a_non_finite_state() {
    for id in example_plan_ids() {
        let mut app = test_app();
        let user = empty_user_dir("example-plans-run");
        let errors = app
            .world_mut()
            .resource_mut::<CapturePlanLibrary>()
            .load_all(&example_dir(), user.path());
        assert!(errors.is_empty(), "load errors: {errors:?}");
        let plan = app.world().resource::<CapturePlanLibrary>().plans[&id].clone();
        app.world_mut().resource_mut::<SelectedProject>().project_id = Some(id.clone());
        {
            let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
            selection.target = Some(q13::catalog_pick("HST", q13::HST_NORAD_ID));
            selection.chaser = Some(q13::catalog_pick("CHASER", q13::CHASER_NORAD_ID));
        }
        enter_sim(&mut app);
        start_running(&mut app);
        tick(&mut app); // `init_orbitals` turns the picks into propagators
        let (debris, root) = {
            let cache = app.world().resource::<OrbitalCache>();
            (cache.debris["Satellite1"], cache.tethers[&plan.tether][0])
        };

        // As the UI's "Capture" button does.
        app.world_mut()
            .resource_mut::<Messages<SimCommand>>()
            .write(SimCommand::StartCapture {
                target: Some(debris),
                plan_id: id.clone(),
            });
        tick(&mut app);
        let first = &plan.states[0].id;
        assert_eq!(
            &app.world()
                .get::<CaptureComponent>(debris)
                .expect("capture started")
                .current_state,
            first,
            "`{id}` starts in its first state"
        );

        for n in 0..TICKS {
            tick(&mut app);
            for (what, entity) in [("debris", debris), ("root", root)] {
                let position = app.world().get::<Position>(entity).unwrap().0;
                assert!(
                    position.is_finite(),
                    "`{id}`: {what} position {position:?} after {n} ticks"
                );
            }
            let state = &app
                .world()
                .get::<CaptureComponent>(debris)
                .unwrap()
                .current_state;
            assert!(
                plan.states.iter().any(|s| &s.id == state),
                "`{id}`: state `{state}` is not one of the plan's states"
            );
        }
    }
}
