//! The scenario: the same set-up as `ssg_sim::test_support::q13::build`, driven
//! from files instead of the Q13 fixture.

use std::time::{Duration, Instant};

use avian3d::collider_tree::ColliderTreeDiagnostics;
use avian3d::collision::CollisionDiagnostics;
use avian3d::dynamics::solver::SolverDiagnostics;
use avian3d::prelude::*;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::{input::InputPlugin, scene::ScenePlugin};
use brahe::utils::DOrbitStateProvider;
use brahe::{AngleFormat, Epoch, KeplerianPropagator, TimeSystem};
use ssg_sim::SimPlugins;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::capture::{CaptureComponent, SimCommand};
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::catalog::{JsonOrbital, gp_record_to_elements};
use ssg_sim::orbit::selection::{
    EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState, SelectedOrbitalObject,
};
use ssg_sim::orbit::{Orbit, Orbital};
use ssg_sim::project::{AssetsDir, SelectedProject};
use ssg_sim::session::{SimState, UiScreen};
use ssg_sim::telemetry::{LogEvent, LogLevel};
use ssg_sim::time::{PHYS_DT, WorldTime};

use crate::args::{Args, EpochArg};
use crate::inputs::{find_records, load_plans};

pub struct Transition {
    pub step: u64,
    pub from: String,
    pub to: String,
}

pub struct Sample {
    pub step: u64,
    pub epoch: String,
    pub state: String,
    pub range_m: f64,
    pub rel_speed_mps: f64,
}

pub struct Run {
    pub plan: String,
    pub epoch_mode: &'static str,
    pub start_epoch: String,
    /// Updates run after the capture command.
    pub updates: u64,
    pub transitions: Vec<Transition>,
    pub samples: Vec<Sample>,
    pub last: Sample,
    pub wall: Duration,
}

pub fn run(args: &Args) -> Result<Run, String> {
    let started = Instant::now();
    let reference_epoch = match args.epoch {
        EpochArg::Now => Epoch::now(),
        EpochArg::At(epoch) => epoch,
    };
    let (target, chaser) = find_records(args)?;
    let pick = |record: &JsonOrbital, id: u64| -> Result<SelectedOrbitalObject, String> {
        let (_, gpu) = gp_record_to_elements(record, reference_epoch)
            .ok_or_else(|| format!("NORAD {id}: the record is missing an orbital element"))?;
        Ok(SelectedOrbitalObject {
            source: OrbitalSelectionSource::Custom {
                label: record
                    .object_name
                    .clone()
                    .unwrap_or_else(|| format!("NORAD {id}")),
            },
            // The f32 round trip the app performs (F-27).
            elements: EditableOrbitalElements::from_gpu(&gpu),
        })
    };
    let target = pick(&target, args.target)?;
    let chaser = pick(&chaser, args.chaser)?;

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        AssetPlugin::default(),
        InputPlugin,
        ScenePlugin,
    ))
    .init_asset::<Mesh>()
    .add_plugins(SimPlugins)
    .init_resource::<CollisionDiagnostics>()
    .init_resource::<SpatialQueryDiagnostics>()
    .init_resource::<SolverDiagnostics>()
    .init_resource::<ColliderTreeDiagnostics>()
    .insert_resource(AssetsDir(args.assets.clone()))
    // One `app.update()` = one fixed step (the P2 clock recipe, D8).
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        PHYS_DT,
    )));

    let plan = load_plans(&mut app, args)?;
    app.world_mut().resource_mut::<SelectedProject>().project_id = Some(plan.clone());
    {
        let mut selection = app.world_mut().resource_mut::<OrbitalSelectionState>();
        selection.target = Some(target);
        selection.chaser = Some(chaser);
    }

    app.world_mut()
        .resource_mut::<NextState<UiScreen>>()
        .set(UiScreen::Sim);
    app.update();
    if let EpochArg::At(epoch) = args.epoch {
        // F-06: `setup_time` has just set both epochs from the wall clock.
        let mut world_time = app.world_mut().resource_mut::<WorldTime>();
        world_time.epoch = epoch;
        world_time.start_epoch = epoch;
    }
    app.world_mut()
        .resource_mut::<NextState<SimState>>()
        .set(SimState::Running);
    app.update();

    if let EpochArg::At(epoch) = args.epoch {
        // Replace `init_orbitals`' wall-clock start with the requested epoch.
        let pending: Vec<(Entity, nalgebra::Vector6<f64>)> = app
            .world_mut()
            .query::<(Entity, &Orbit)>()
            .iter(app.world())
            .filter_map(|(entity, orbit)| match orbit {
                Orbit::FromElements(elements) => Some((entity, *elements)),
                Orbit::FromTle(_) => None,
            })
            .collect();
        if pending.len() != 2 {
            return Err(format!(
                "expected the tether root and the debris to await their orbits, found {}",
                pending.len()
            ));
        }
        for (entity, elements) in pending {
            let eci =
                KeplerianPropagator::from_keplerian(epoch, elements, AngleFormat::Radians, 1.0)
                    .state_eci(epoch)
                    .map_err(|e| format!("Keplerian propagation failed: {e:?}"))?;
            let mut entity = app.world_mut().entity_mut(entity);
            entity.remove::<Orbit>();
            entity
                .get_mut::<Orbital>()
                .ok_or("an orbiting body has no Orbital")?
                .propagator = Some(KeplerianPropagator::from_eci(epoch, eci, 1.0));
        }
    }

    let (debris, root) = {
        let cache = app.world().resource::<OrbitalCache>();
        let library = app.world().resource::<CapturePlanLibrary>();
        let tether = &library.plans[&plan].tether;
        (
            *cache
                .debris
                .get("Satellite1")
                .ok_or("the debris was not spawned")?,
            cache
                .tethers
                .get(tether)
                .and_then(|nodes| nodes.first())
                .copied()
                .ok_or_else(|| format!("tether `{tether}` was not spawned"))?,
        )
    };

    // Queued before the next update: `apply_sim_commands` runs in `Last` of that
    // update, after its physics step, which is the capture start `q13::build` makes
    // between its two ticks.
    app.world_mut()
        .resource_mut::<Messages<SimCommand>>()
        .write(SimCommand::StartCapture {
            target: Some(debris),
            plan_id: plan.clone(),
        });
    app.update();
    app.world_mut().resource_mut::<WorldTime>().multiplier = args.warp;

    let start_epoch = app.world().resource::<WorldTime>().start_epoch;
    let mut cursor: MessageCursor<LogEvent> = app
        .world()
        .resource::<Messages<LogEvent>>()
        .get_cursor_current();
    let mut transitions = Vec::new();
    let mut samples = Vec::new();
    let mut updates = 0;
    let mut last = sample(&app, debris, root)?;
    while updates < args.steps {
        app.update();
        updates += 1;
        last = sample(&app, debris, root)?;
        let messages = app.world().resource::<Messages<LogEvent>>();
        for event in cursor.read(messages) {
            if event.source == "capture"
                && let Some((from, to)) = parse_transition(&event.message)
            {
                transitions.push(Transition {
                    step: last.step,
                    from,
                    to,
                });
            } else if matches!(event.level, LogLevel::Warn | LogLevel::Error) {
                eprintln!("ssg-sim: {}: {}", event.source, event.message);
            }
        }
        if updates % args.sample_every == 0 {
            samples.push(sample(&app, debris, root)?);
        }
        if args.stop_at_capture && last.state == "capture" {
            break;
        }
    }
    if args.stop_at_capture && last.state != "capture" {
        return Err(format!(
            "no capture within {updates} updates (final state `{}`)",
            last.state
        ));
    }

    Ok(Run {
        plan,
        epoch_mode: match args.epoch {
            EpochArg::Now => "now",
            EpochArg::At(_) => "fixed",
        },
        start_epoch: iso(start_epoch),
        updates,
        transitions,
        samples,
        last,
        wall: started.elapsed(),
    })
}

fn sample(app: &App, debris: Entity, root: Entity) -> Result<Sample, String> {
    let world = app.world();
    let world_time = world.resource::<WorldTime>();
    let state = world
        .get::<CaptureComponent>(debris)
        .ok_or("the debris lost its CaptureComponent")?
        .current_state
        .clone();
    let (debris_position, debris_velocity) = (
        world.get::<Position>(debris).ok_or("debris Position")?.0,
        world
            .get::<LinearVelocity>(debris)
            .ok_or("debris velocity")?
            .0,
    );
    let (root_position, root_velocity) = (
        world.get::<Position>(root).ok_or("root Position")?.0,
        world.get::<LinearVelocity>(root).ok_or("root velocity")?.0,
    );
    Ok(Sample {
        step: ((world_time.epoch - world_time.start_epoch) / PHYS_DT).round() as u64,
        epoch: iso(world_time.epoch),
        state,
        range_m: (debris_position - root_position).length(),
        rel_speed_mps: (root_velocity - debris_velocity).length(),
    })
}

/// `YYYY-MM-DDTHH:MM:SS.ffffffZ` (UTC). brahe's own `isostring_with_decimals`
/// drops the leading zeros of the fraction.
fn iso(epoch: Epoch) -> String {
    let (year, month, day, hour, minute, second, nanoseconds) =
        epoch.to_datetime_as_time_system(TimeSystem::UTC);
    let seconds = second + nanoseconds / 1e9;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{seconds:09.6}Z")
}

/// Parse `State: {from} → {to} ({reason})`.
fn parse_transition(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("State: ")?;
    let (from, rest) = rest.split_once(" → ")?;
    let (to, _) = rest.split_once(" (")?;
    Some((from.to_string(), to.to_string()))
}
