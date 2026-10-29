//! The scenario: the same set-up as `ssg_sim::test_support::q13::build`, driven
//! from files instead of the Q13 fixture.
//!
//! **Step offset (F-51).** This runner measures from step 2: three updates (enter
//! the sim, start it, apply the capture command), and the first sample is step 3.
//! `q13::build` ticks once more before the capture starts, so the harness starts the
//! capture one step later: its checkpoint after tick `t` is at step `t + 3`. Align by
//! absolute step, never by tick count. At the same absolute step the two are
//! bit-identical, which `crates/ssg-sim-cli/tests/harness_parity.rs` checks.

use std::time::{Duration, Instant};

use avian3d::collider_tree::ColliderTreeDiagnostics;
use avian3d::collision::CollisionDiagnostics;
use avian3d::dynamics::solver::SolverDiagnostics;
use avian3d::prelude::*;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::{input::InputPlugin, scene::ScenePlugin};
use brahe::Epoch;
use ssg_sim::SimPlugins;
use ssg_sim::capture::plan::CapturePlanLibrary;
use ssg_sim::capture::{CaptureComponent, CaptureTransition, SimCommand};
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::lvlh::LvlhFrame;
use ssg_sim::orbit::selection::{OrbitalSelectionSource, OrbitalSelectionState};
use ssg_sim::physics_bubble::body_eci_state;
use ssg_sim::project::{AssetsDir, SelectedProject, format_utc};
use ssg_sim::session::{SimState, UiScreen};
use ssg_sim::telemetry::{EphemerisSample, LogEvent, LogLevel};
use ssg_sim::time::{PHYS_DT, SimConfig, WorldTime};

use crate::args::Args;
use crate::inputs::{load_plans, resolve};

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
    /// The tether root's and the debris's `Position` and `LinearVelocity` (bubble
    /// frame), for the state hash.
    pub root_position: DVec3,
    pub root_velocity: DVec3,
    pub debris_position: DVec3,
    pub debris_velocity: DVec3,
}

pub struct Run {
    pub plan: String,
    /// The NORAD ids of the two bodies, when they have them.
    pub target: Option<u64>,
    pub chaser: Option<u64>,
    /// The labels of the two bodies (`OBJECT_NAME` in `ephemeris.oem`).
    pub target_label: String,
    pub chaser_label: String,
    pub epoch_mode: &'static str,
    /// The start epoch as an `Epoch` (`CREATION_DATE` of `ephemeris.oem`) and as
    /// its `format_utc` string.
    pub start: Epoch,
    pub start_epoch: String,
    /// Fixed steps run after the capture command.
    pub steps: u64,
    pub transitions: Vec<Transition>,
    pub samples: Vec<Sample>,
    /// The debris's (target) and the root's (chaser) ECI states at each sample.
    pub ephemeris: Vec<EphemerisSample>,
    pub last: Sample,
    pub wall: Duration,
}

pub fn run(args: &Args) -> Result<Run, String> {
    let started = Instant::now();
    let inputs = resolve(args)?;
    let reference_epoch = inputs.start_epoch;
    let (target, chaser) = (inputs.target.clone(), inputs.chaser.clone());
    let label = |source: &OrbitalSelectionSource| match source {
        OrbitalSelectionSource::Catalog { label, .. }
        | OrbitalSelectionSource::Custom { label } => label.clone(),
    };
    let (target_label, chaser_label) = (label(&target.source), label(&chaser.source));

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
    // The sim start epoch: `--epoch`, with `now` resolved once above (F-06).
    .insert_resource(SimConfig {
        start_epoch: reference_epoch,
    })
    // One `app.update()` = one fixed step (the P2 clock recipe, D8).
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        PHYS_DT,
    )));

    let plan = load_plans(&mut app, args, inputs.plan.as_deref())?;
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
    app.world_mut()
        .resource_mut::<NextState<SimState>>()
        .set(SimState::Running);
    app.update();

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
    let mut transition_cursor: MessageCursor<CaptureTransition> = app
        .world()
        .resource::<Messages<CaptureTransition>>()
        .get_cursor_current();
    // One update runs `--warp` fixed steps (`--warp` is at most
    // `MAX_STEPS_PER_UPDATE`); `--steps` and `--sample-every` are multiples of it.
    let warp = u64::from(args.warp);
    let (total_updates, sample_every_updates) = (args.steps / warp, args.sample_every / warp);
    let mut transitions = Vec::new();
    let mut samples = Vec::new();
    let mut ephemeris = Vec::new();
    let mut updates = 0;
    let mut last = sample(&app, debris, root)?;
    while updates < total_updates {
        app.update();
        updates += 1;
        last = sample(&app, debris, root)?;
        let messages = app.world().resource::<Messages<CaptureTransition>>();
        for transition in transition_cursor.read(messages) {
            transitions.push(Transition {
                // The step the transition fired in (exact also when warped).
                step: ((transition.epoch - start_epoch) / PHYS_DT).round() as u64,
                from: transition.from.clone(),
                to: transition.to.clone(),
            });
        }
        let messages = app.world().resource::<Messages<LogEvent>>();
        for event in cursor.read(messages) {
            if matches!(event.level, LogLevel::Warn | LogLevel::Error) {
                eprintln!("ssg-sim: {}: {}", event.source, event.message);
            }
        }
        if updates % sample_every_updates == 0 {
            samples.push(sample(&app, debris, root)?);
            ephemeris.push(ephemeris_sample(&app, debris, root)?);
        }
        if args.stop_at_capture && last.state == "capture" {
            break;
        }
    }
    if args.stop_at_capture && last.state != "capture" {
        return Err(format!(
            "no capture within {} steps (final state `{}`)",
            updates * warp,
            last.state
        ));
    }

    Ok(Run {
        plan,
        epoch_mode: inputs.epoch_mode,
        target: inputs.target_id,
        chaser: inputs.chaser_id,
        target_label,
        chaser_label,
        start: start_epoch,
        start_epoch: format_utc(start_epoch),
        steps: updates * warp,
        transitions,
        samples,
        ephemeris,
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
    // The root's velocity relative to the debris in the debris's LVLH frame, as
    // the controller measures it (F-35).
    let (rel_r, rel_v) = (
        root_position - debris_position,
        root_velocity - debris_velocity,
    );
    let rel_v = body_eci_state(
        world.resource::<OrbitalCache>(),
        debris,
        debris_position,
        debris_velocity,
        world.get::<RigidBodyDisabled>(debris).is_some(),
    )
    .and_then(|state| LvlhFrame::from_eci(&state))
    .map_or(rel_v, |frame| {
        frame.relative_velocity_in_frame(rel_r, rel_v)
    });
    Ok(Sample {
        step: ((world_time.epoch - world_time.start_epoch) / PHYS_DT).round() as u64,
        epoch: format_utc(world_time.epoch),
        state,
        range_m: (debris_position - root_position).length(),
        rel_speed_mps: rel_v.length(),
        root_position,
        root_velocity,
        debris_position,
        debris_velocity,
    })
}

/// The debris's and the root's ECI states now (bubble offset added for enabled
/// bodies), as the app's capture controller samples them.
fn ephemeris_sample(app: &App, debris: Entity, root: Entity) -> Result<EphemerisSample, String> {
    let world = app.world();
    let cache = world.resource::<OrbitalCache>();
    let eci = |entity: Entity, what: &str| {
        let position = world
            .get::<Position>(entity)
            .ok_or(format!("{what} Position"))?;
        let velocity = world
            .get::<LinearVelocity>(entity)
            .ok_or(format!("{what} velocity"))?;
        body_eci_state(
            cache,
            entity,
            position.0,
            velocity.0,
            world.get::<RigidBodyDisabled>(entity).is_some(),
        )
        .ok_or(format!("the {what} has no ECI state"))
    };
    Ok(EphemerisSample {
        epoch: world.resource::<WorldTime>().epoch,
        target: eci(debris, "debris")?,
        chaser: eci(root, "root")?,
    })
}
