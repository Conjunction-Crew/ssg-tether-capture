//! The replay mode of the sim screen (P7 S5): the transport clicks, the readouts,
//! the Earth placement across a floating-origin reset and the Replay state. No
//! rendering: the windows draw from the state these tests drive.

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::{DQuat, DVec3};
use bevy::pbr::{Atmosphere, AtmosphereSettings};
use bevy::prelude::*;

use crate::render::celestial::replay_earth_translation;
use crate::render::layers::{MAP_LAYER, SCENE_LAYER};
use crate::tests::app_support::{enter_sim, test_app, test_app_with_attach};
use crate::ui::replay::{
    ReplayBody, ReplaySession, ReturnToSim, TransportAction, apply_transport, epoch_at,
    exit_replay, followed_body, relative_state, replay_shapes, return_to_sim,
    show_replay_in_scene_view, spawn_replay_bodies,
};
use avian3d::prelude::{Collider, ColliderConstructorHierarchy};
use ssg_sim::project::recording::RecordingBodyRole;
use ssg_sim::project::{format_utc, parse_utc};
use ssg_sim::recording::BodyFrame;
use ssg_sim::recording::files::{FrameLine, RecordedFrame};
use ssg_sim::recording::player::Player;
use ssg_sim::session::{SimState, UiScreen};
use ssg_sim::test_support::fixed_epoch;
use ssg_sim::time::WorldTime;

fn body(position: DVec3, velocity: DVec3) -> BodyFrame {
    BodyFrame {
        position,
        rotation: DQuat::IDENTITY,
        velocity,
        angular_velocity: DVec3::ZERO,
    }
}

fn frame(step: u64, origin_x: f64, shift: f64, bodies: Vec<BodyFrame>) -> RecordedFrame {
    RecordedFrame {
        line: FrameLine {
            step,
            epoch_utc: format_utc(fixed_epoch()),
            phase: None,
            capture_sphere_radius_m: 25.0,
            frame_origin_eci: Some([origin_x, 0.0, 0.0, 0.0, 7500.0, 0.0]),
            origin_shift_m: [shift, 0.0, 0.0],
            origin_shift_mps: [0.0; 3],
        },
        bodies,
    }
}

/// Frames at steps 0, 32 and 64 of a still body 10 m from the origin.
fn three_frames() -> Player {
    let still = |x: f64| vec![body(DVec3::new(x, 0.0, 0.0), DVec3::ZERO)];
    Player::new(
        vec![
            frame(0, 7.0e6, 0.0, still(10.0)),
            frame(32, 7.0e6, 0.0, still(10.0)),
            frame(64, 7.0e6, 0.0, still(10.0)),
        ],
        Some(Vec::new()),
    )
    .expect("frames")
}

#[test]
fn step_snaps_to_frames_and_jumps_pause() {
    let mut player = three_frames();
    player.seek(10.0);
    apply_transport(&mut player, &TransportAction::StepForward);
    assert_eq!((player.cursor(), player.view().interpolated), (32.0, false));
    apply_transport(&mut player, &TransportAction::StepBack);
    assert_eq!(player.cursor(), 0.0);
    // At the first frame, back stays.
    apply_transport(&mut player, &TransportAction::StepBack);
    assert_eq!(player.cursor(), 0.0);

    player.playing = true;
    apply_transport(&mut player, &TransportAction::JumpToEnd);
    assert_eq!((player.cursor(), player.playing), (64.0, false));
    apply_transport(&mut player, &TransportAction::JumpToStart);
    assert_eq!((player.cursor(), player.playing), (0.0, false));
}

#[test]
fn play_runs_at_the_chosen_speed_forward_and_in_reverse() {
    let mut player = three_frames();
    apply_transport(&mut player, &TransportAction::SetSpeed(2.0));
    apply_transport(&mut player, &TransportAction::TogglePlay);
    assert!(player.playing);
    // One wall second at 2× is two sim seconds: 128 steps, so play reaches the end
    // (step 64) and stops there.
    player.advance(0.25);
    assert_eq!(player.cursor(), 32.0);
    player.advance(1.0);
    assert_eq!((player.cursor(), player.playing), (64.0, false));

    // Play at the end starts again from the start; in reverse it runs back.
    apply_transport(&mut player, &TransportAction::TogglePlay);
    assert!(player.playing && player.cursor() == 0.0);
    apply_transport(&mut player, &TransportAction::TogglePlay);
    apply_transport(&mut player, &TransportAction::Seek(64.0));
    apply_transport(&mut player, &TransportAction::SetReverse(true));
    apply_transport(&mut player, &TransportAction::TogglePlay);
    player.advance(0.25);
    assert_eq!(player.cursor(), 32.0);
    player.advance(5.0);
    assert_eq!((player.cursor(), player.playing), (0.0, false));
    // Reverse at the start restarts from the end.
    apply_transport(&mut player, &TransportAction::TogglePlay);
    assert!(player.playing && player.cursor() == 64.0);
}

#[test]
fn scrubbing_clamps_to_the_recording() {
    let mut player = three_frames();
    apply_transport(&mut player, &TransportAction::Seek(1000.0));
    assert_eq!(player.cursor(), 64.0);
    apply_transport(&mut player, &TransportAction::Seek(-5.0));
    assert_eq!(player.cursor(), 0.0);
    apply_transport(&mut player, &TransportAction::Seek(40.5));
    assert_eq!(player.cursor(), 40.5);
    assert!(player.view().interpolated);
}

#[test]
fn the_epoch_readout_counts_from_the_frame_before_the_cursor() {
    let start = fixed_epoch();
    let epochs = vec![(100, format_utc(start)), (132, format_utc(start))];
    let at = |cursor| parse_utc(&epoch_at(&epochs, cursor).expect("an epoch")).expect("UTC");
    assert!((at(100.0) - start).abs() < 1e-6);
    // 16 steps are a quarter of a second.
    assert!((at(116.0) - start - 0.25).abs() < 1e-6);
    // Before the first frame there is nothing to count from.
    assert!(epoch_at(&epochs, 50.0).is_none());
}

#[test]
fn range_and_relative_speed_are_the_targets_to_the_tether_root() {
    let bodies = vec![
        body(DVec3::new(30.0, 40.0, 0.0), DVec3::new(0.0, 0.0, 2.0)),
        body(DVec3::ZERO, DVec3::new(0.0, 0.0, -2.0)),
        body(DVec3::new(1.0, 1.0, 1.0), DVec3::ZERO),
    ];
    let (range, speed) = relative_state(&bodies).expect("two bodies");
    assert_eq!((range, speed), (50.0, 4.0));
    assert!(relative_state(&bodies[..1]).is_none());
}

/// A body at rest in ECI, 10 m from the frame origin; a floating-origin reset
/// moves the origin 50 m along +X between two frames, so the body's bubble
/// position drops by 50 m. In the replay the Earth sits at minus the frame origin.
fn reset_between_two_frames() -> Player {
    let at = |origin_x: f64, shift: f64, x: f64| {
        frame(
            if shift == 0.0 { 0 } else { 32 },
            origin_x,
            shift,
            vec![body(DVec3::new(x, 0.0, 0.0), DVec3::ZERO)],
        )
    };
    Player::new(
        vec![at(7.0e6, 0.0, 10.0), at(7.0e6 + 50.0, 50.0, 10.0 - 50.0)],
        Some(Vec::new()),
    )
    .expect("frames")
}

#[test]
fn the_earth_stays_put_relative_to_the_bodies_across_a_reset() {
    let player = reset_between_two_frames();
    let eci_x = |cursor: f64| {
        let view = player.state_at(cursor);
        let origin = view.frame_origin_eci.expect("a frame origin");
        let earth = replay_earth_translation(&origin);
        // The body in the scene, and the Earth's centre as the scene places it.
        (view.bodies[0].position.x, f64::from(earth.x), origin[0])
    };
    for cursor in (0..=64).map(|i| f64::from(i) * 0.5) {
        let (body_x, earth_x, origin_x) = eci_x(cursor);
        // Body minus Earth is the body's ECI position: 7e6 + 10 at every cursor
        // (the Earth's f32 translation is good to half a metre at this size).
        assert!(
            (body_x - earth_x - (7.0e6 + 10.0)).abs() < 1.0,
            "cursor {cursor}: body {body_x}, Earth {earth_x}"
        );
        // In f64, from the player's own numbers, it is exact.
        assert!(
            (origin_x + body_x - (7.0e6 + 10.0)).abs() < 1e-6,
            "cursor {cursor}"
        );
    }
    // The scene shifts as a whole right after the first frame (body and Earth by
    // the same 50 m) and is continuous from there to the second frame.
    let (b0, e0, _) = eci_x(0.0);
    let (b1, e1, _) = eci_x(0.5);
    assert!(
        (b1 - b0 + 50.0).abs() < 1e-6 && (e1 - e0 + 50.0).abs() < 1.0,
        "{b0} {b1} {e0} {e1}"
    );
    let (b2, e2, _) = eci_x(31.5);
    let (b3, e3, _) = eci_x(32.0);
    assert!(
        (b3 - b2).abs() < 1e-6 && (e3 - e2).abs() < 1.0,
        "{b2} {b3} {e2} {e3}"
    );
}

#[test]
fn the_earth_is_placed_at_minus_the_frame_origin() {
    let translation =
        replay_earth_translation(&[7_000_000.0, -1_500_000.0, 250_000.0, 1.0, 2.0, 3.0]);
    assert_eq!(
        translation,
        Vec3::new(-7_000_000.0, 1_500_000.0, -250_000.0)
    );
}

/// The sim state at each entry of the sim screen.
#[derive(Resource, Default)]
struct ScreenEntries(Vec<SimState>);

/// Stands for what the sim screen spawns on entry (camera, Earth, lights).
#[derive(Component)]
struct SimScreenEntity;

#[test]
fn a_replay_runs_no_physics_and_exit_returns_to_a_fresh_setup() {
    let mut app = test_app();
    app.init_resource::<ScreenEntries>()
        .init_resource::<ReturnToSim>()
        .add_systems(
            OnEnter(UiScreen::Sim),
            |mut commands: Commands,
             mut entries: ResMut<ScreenEntries>,
             sim_state: Res<State<SimState>>| {
                entries.0.push(sim_state.get().clone());
                commands.spawn((SimScreenEntity, DespawnOnExit(UiScreen::Sim)));
            },
        )
        .add_systems(Update, return_to_sim.run_if(in_state(UiScreen::Home)));
    enter_sim(&mut app);
    assert_eq!(app.world().resource::<ScreenEntries>().0, [SimState::Setup]);

    app.world_mut()
        .resource_mut::<NextState<SimState>>()
        .set(SimState::Replay);
    app.update();
    assert_eq!(
        *app.world().resource::<State<SimState>>().get(),
        SimState::Replay
    );
    let epoch = app.world().resource::<WorldTime>().epoch;
    for _ in 0..30 {
        app.update();
    }
    // The fixed step is gated by `SimState::Running`: nothing advanced.
    assert_eq!(app.world().resource::<WorldTime>().epoch, epoch);
    assert_eq!(app.world().resource::<WorldTime>().effective_multiplier, 0);

    // Exit replay, as the Transport window does: Home this frame, the sim screen
    // the next.
    app.world_mut()
        .run_system_once(
            |mut next_sim_state: ResMut<NextState<SimState>>,
             mut next_screen: ResMut<NextState<UiScreen>>,
             mut pending: ResMut<ReturnToSim>| {
                exit_replay(&mut next_sim_state, &mut next_screen, &mut pending);
            },
        )
        .expect("exit_replay ran");
    app.update();
    assert_eq!(
        *app.world().resource::<State<UiScreen>>().get(),
        UiScreen::Home
    );
    app.update();
    assert_eq!(
        *app.world().resource::<State<SimState>>().get(),
        SimState::Setup
    );
    assert_eq!(
        *app.world().resource::<State<UiScreen>>().get(),
        UiScreen::Sim
    );
    assert_eq!(
        app.world().resource::<ScreenEntries>().0,
        [SimState::Setup, SimState::Setup],
        "the sim screen is entered again, already in its setup state, so its setup screen is rebuilt"
    );
    // What the first entry spawned is gone: one camera, not two (O5 C9).
    let world = app.world_mut();
    let spawned = world
        .query_filtered::<(), With<SimScreenEntity>>()
        .iter(world)
        .count();
    assert_eq!(spawned, 1);
    assert!(!app.world().resource::<ReturnToSim>().0);
}

#[test]
fn the_camera_follows_the_tether_root_or_the_debris() {
    use ssg_sim::project::recording::RecordingBodyRole::*;
    let roles = [Target, TetherRoot, TetherNode, TetherTail];
    assert_eq!(followed_body(&roles, false), Some(1));
    assert_eq!(followed_body(&roles, true), Some(0));
    assert_eq!(followed_body(&[], false), None);
}

#[test]
fn a_replay_opens_in_the_detail_view() {
    let mut app = test_app();
    let camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            RenderLayers::layer(MAP_LAYER),
            Atmosphere {
                bottom_radius: 6_360_000.0,
                top_radius: 6_460_000.0,
                world_position: Vec3::ZERO,
                ground_albedo: Vec3::splat(0.3),
                medium: Handle::default(),
            },
            AtmosphereSettings::default(),
        ))
        .id();
    // From the map to the scene, and it stays there when already on the scene.
    for _ in 0..2 {
        app.world_mut()
            .run_system_once(show_replay_in_scene_view)
            .expect("show_replay_in_scene_view ran");
        let layers = app.world().get::<RenderLayers>(camera).expect("layers");
        assert_eq!(*layers, RenderLayers::layer(SCENE_LAYER));
    }
}

/// F-64 (P7.5 S3, C5): a replay of the default tether (the target and 77 tether
/// bodies) with the app's attach systems gets its meshes and no collider: the
/// replay world has no physics, and the attach path builds none.
#[test]
fn a_replay_with_the_attach_systems_has_no_collider() {
    let mut app = test_app_with_attach();
    app.add_systems(OnEnter(SimState::Replay), spawn_replay_bodies);
    enter_sim(&mut app);
    let mut roles = vec![RecordingBodyRole::Target, RecordingBodyRole::TetherRoot];
    roles.extend([RecordingBodyRole::TetherNode; 75]);
    roles.push(RecordingBodyRole::TetherTail);
    let bodies: Vec<BodyFrame> = (0..roles.len())
        .map(|i| body(DVec3::new(0.0, i as f64 * 0.25, 0.0), DVec3::ZERO))
        .collect();
    let player =
        Player::new(vec![frame(0, 7.0e6, 0.0, bodies)], Some(Vec::new())).expect("a frame");
    app.insert_resource(ReplaySession {
        name: "r".to_string(),
        player,
        shapes: replay_shapes(&roles),
        roles,
        epochs: vec![(0, format_utc(fixed_epoch()))],
        differences: Vec::new(),
    });
    app.world_mut()
        .resource_mut::<NextState<SimState>>()
        .set(SimState::Replay);
    app.update();

    let world = app.world_mut();
    let replayed = world.query::<&ReplayBody>().iter(world).count();
    let meshes = world
        .query_filtered::<(), (With<ReplayBody>, With<Mesh3d>)>()
        .iter(world)
        .count();
    let colliders = world
        .query_filtered::<(), With<Collider>>()
        .iter(world)
        .count();
    let hulls = world
        .query::<&ColliderConstructorHierarchy>()
        .iter(world)
        .count();
    assert_eq!((replayed, meshes), (78, 77), "the bodies and their meshes");
    assert_eq!((colliders, hulls), (0, 0));
}
