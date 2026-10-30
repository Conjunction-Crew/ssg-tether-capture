//! Playback (P7 S4, R6): origin-aware interpolation between frames, exact phase
//! from the transitions and the radius rule, step snapping, seek, speeds, reverse.
//! Pure: synthetic frames, no app.

use bevy::math::{DQuat, DVec3};

use crate::project::recording::RecordingSource;
use crate::recording::BodyFrame;
use crate::recording::files::{FrameLine, RecordedFrame, TransitionLine, read_recording};
use crate::recording::player::{PLAYBACK_SPEEDS, Player};
use crate::test_support::TempDir;
use crate::tests::recording_files::valid_recording;
use crate::time::FIXED_HZ;

fn body(position: DVec3, rotation: DQuat, velocity: DVec3) -> BodyFrame {
    BodyFrame {
        position,
        rotation,
        velocity,
        angular_velocity: velocity * 0.5,
    }
}

/// A frame at `step` with `phase`, `radius` and the shift since the previous frame.
fn frame(
    step: u64,
    phase: &str,
    radius: f64,
    shift_m: [f64; 3],
    bodies: Vec<BodyFrame>,
) -> RecordedFrame {
    RecordedFrame {
        line: FrameLine {
            step,
            epoch_utc: String::new(),
            phase: Some(phase.to_string()),
            capture_sphere_radius_m: radius,
            frame_origin_eci: Some([7.0e6, 0.0, 0.0, 0.0, 7.5e3, 0.0]),
            origin_shift_m: shift_m,
            origin_shift_mps: [0.0; 3],
        },
        bodies,
    }
}

fn transition(step: u64, from: &str, to: &str) -> TransitionLine {
    TransitionLine {
        step,
        epoch_utc: String::new(),
        from: from.to_string(),
        to: to.to_string(),
        reason: String::new(),
    }
}

/// Frames at steps 100, 132, 164 (K = 32) of one body moving 32 m per frame along x.
fn three_frames() -> Vec<RecordedFrame> {
    [100, 132, 164]
        .into_iter()
        .enumerate()
        .map(|(i, step)| {
            frame(
                step,
                "approach",
                25.0 - i as f64,
                [0.0; 3],
                vec![body(
                    DVec3::new(32.0 * i as f64, 0.0, 0.0),
                    DQuat::IDENTITY,
                    DVec3::new(1.0 + i as f64, 0.0, 0.0),
                )],
            )
        })
        .collect()
}

fn player(frames: Vec<RecordedFrame>, transitions: Vec<TransitionLine>) -> Player {
    Player::new(frames, Some(transitions)).unwrap()
}

#[test]
fn a_frame_shows_as_it_is_and_the_midpoint_is_interpolated() {
    let p = player(three_frames(), Vec::new());
    let at = p.state_at(132.0);
    assert!(!at.interpolated);
    assert_eq!(at.bodies[0].position, DVec3::new(32.0, 0.0, 0.0));
    assert_eq!(at.radius, 24.0);

    let mid = p.state_at(116.0);
    assert!(mid.interpolated);
    assert_eq!(mid.step, 116.0);
    assert_eq!(mid.bodies[0].position, DVec3::new(16.0, 0.0, 0.0));
    assert_eq!(mid.bodies[0].velocity, DVec3::new(1.5, 0.0, 0.0));
    assert_eq!(mid.bodies[0].angular_velocity, DVec3::new(0.75, 0.0, 0.0));
    assert_eq!(mid.radius, 24.5, "no transition between: linear");
    let quarter = p.state_at(108.0);
    assert_eq!(quarter.bodies[0].position, DVec3::new(8.0, 0.0, 0.0));
}

#[test]
fn an_interpolated_rotation_is_normalised_and_takes_the_short_arc() {
    let a = DQuat::from_rotation_z(0.0);
    let b = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
    let frames = vec![
        frame(
            0,
            "approach",
            25.0,
            [0.0; 3],
            vec![body(DVec3::ZERO, a, DVec3::ZERO)],
        ),
        // The same rotation with the opposite sign: the short arc is unchanged.
        frame(
            32,
            "approach",
            25.0,
            [0.0; 3],
            vec![body(DVec3::ZERO, -b, DVec3::ZERO)],
        ),
    ];
    let p = player(frames, Vec::new());
    for cursor in [4.0, 16.0, 29.0] {
        let r = p.state_at(cursor).bodies[0].rotation;
        assert!((r.length() - 1.0).abs() < 1e-15, "|q| = {}", r.length());
    }
    let mid = p.state_at(16.0).bodies[0].rotation;
    let expected = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_4);
    assert!(
        mid.dot(expected).abs() > 1.0 - 1e-15,
        "{mid:?} vs {expected:?}"
    );
}

#[test]
fn a_50_m_origin_shift_interpolates_to_the_midpoint_of_the_shifted_positions() {
    // The body stands still in ECI. A reset between the frames moved the origin by
    // 50 m along x, so the body's bubble position jumps from 60 m to 10 m and the
    // frame origin from x0 to x0 + 50 m.
    let shift = [50.0, 0.0, 0.0];
    let mut a = frame(
        0,
        "approach",
        25.0,
        [0.0; 3],
        vec![body(
            DVec3::new(60.0, 4.0, 0.0),
            DQuat::IDENTITY,
            DVec3::ZERO,
        )],
    );
    let mut b = frame(
        32,
        "approach",
        25.0,
        shift,
        vec![body(
            DVec3::new(10.0, 4.0, 0.0),
            DQuat::IDENTITY,
            DVec3::ZERO,
        )],
    );
    a.line.frame_origin_eci = Some([7.0e6, 0.0, 0.0, 0.0, 0.0, 0.0]);
    b.line.frame_origin_eci = Some([7.0e6 + 50.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    let p = player(vec![a, b], Vec::new());
    for cursor in [0.0, 1.0, 16.0, 31.0, 32.0] {
        let view = p.state_at(cursor);
        let eci_x = view.frame_origin_eci.unwrap()[0] + view.bodies[0].position.x;
        assert_eq!(
            eci_x,
            7.0e6 + 60.0,
            "the body stays put in ECI at cursor {cursor}"
        );
    }
    let mid = p.state_at(16.0);
    assert_eq!(mid.bodies[0].position, DVec3::new(10.0, 4.0, 0.0));
    assert_eq!(mid.frame_origin_eci.unwrap()[0], 7.0e6 + 50.0);
}

#[test]
fn the_phase_is_exact_at_a_transition_inside_a_frame_interval() {
    let mut frames = three_frames();
    frames[0].line.phase = Some("rendezvous".into());
    frames[1].line.capture_sphere_radius_m = 40.0;
    let p = player(frames, vec![transition(120, "rendezvous", "approach")]);
    assert_eq!(p.state_at(119.0).phase.as_deref(), Some("rendezvous"));
    assert_eq!(p.state_at(119.9).phase.as_deref(), Some("rendezvous"));
    assert_eq!(p.state_at(120.0).phase.as_deref(), Some("approach"));
    assert_eq!(p.state_at(150.0).phase.as_deref(), Some("approach"));
    // The radius holds the value of the frame on the cursor's side of the transition.
    assert_eq!(p.state_at(119.0).radius, 25.0);
    assert_eq!(p.state_at(120.0).radius, 40.0);
    assert_eq!(p.state_at(131.0).radius, 40.0);
    // No transition in the next interval: linear again.
    assert_eq!(p.state_at(148.0).radius, 40.0 + (23.0 - 40.0) * 0.5);
}

#[test]
fn without_transitions_the_phase_and_radius_are_the_frame_befores() {
    let mut frames = three_frames();
    frames[1].line.phase = Some("terminal".into());
    let p = Player::new(frames.clone(), None).unwrap();
    assert_eq!(p.state_at(131.0).phase.as_deref(), Some("approach"));
    assert_eq!(p.state_at(131.0).radius, 25.0);
    assert_eq!(p.state_at(133.0).phase.as_deref(), Some("terminal"));
    // An empty transitions file: the first frame's phase holds throughout.
    let p = player(frames, Vec::new());
    assert_eq!(p.state_at(133.0).phase.as_deref(), Some("approach"));
}

#[test]
fn step_snaps_to_frames_and_stops_at_both_ends() {
    let mut p = player(three_frames(), Vec::new());
    assert_eq!(p.cursor(), 100.0);
    p.step(-1);
    assert_eq!(p.cursor(), 100.0, "stays at the start");
    p.seek(110.5);
    p.playing = true;
    p.step(1);
    assert_eq!(p.cursor(), 132.0);
    assert!(!p.playing, "a step pauses");
    assert!(!p.view().interpolated);
    p.step(1);
    p.step(1);
    assert_eq!(p.cursor(), 164.0, "stays at the end");
    p.seek(140.0);
    p.step(-1);
    assert_eq!(p.cursor(), 132.0);
    p.step(-1);
    assert_eq!(p.cursor(), 100.0);
}

#[test]
fn seek_outside_the_range_clamps() {
    let mut p = player(three_frames(), Vec::new());
    p.seek(-5.0);
    assert_eq!(p.cursor(), 100.0);
    p.seek(1e9);
    assert_eq!(p.cursor(), 164.0);
    p.seek(f64::NAN);
    assert_eq!(p.cursor(), 100.0);
    assert_eq!(p.state_at(1e9).bodies, p.state_at(164.0).bodies);
    assert_eq!((p.start_step(), p.end_step()), (100, 164));
}

#[test]
fn play_moves_speed_times_the_wall_time_and_stops_at_the_end() {
    let mut p = player(three_frames(), Vec::new());
    p.advance(1.0);
    assert_eq!(p.cursor(), 100.0, "paused: nothing moves");
    p.playing = true;
    p.speed = PLAYBACK_SPEEDS[0];
    p.advance(1.0);
    assert_eq!(p.cursor(), 100.0 + 0.125 * FIXED_HZ);
    p.speed = 0.5;
    p.advance(0.25);
    assert_eq!(p.cursor(), 108.0 + 8.0);
    assert!(p.playing);
    p.speed = 64.0;
    p.advance(1.0);
    assert_eq!(p.cursor(), 164.0);
    assert!(!p.playing, "play stops at the end");
}

#[test]
fn reverse_play_runs_back_and_stops_at_the_start() {
    let mut p = player(three_frames(), Vec::new());
    p.seek(164.0);
    p.playing = true;
    p.reverse = true;
    p.advance(0.5);
    assert_eq!(p.cursor(), 132.0);
    assert!(p.playing);
    p.speed = 2.0;
    p.advance(1.0);
    assert_eq!(p.cursor(), 100.0);
    assert!(!p.playing, "reverse play stops at the start");
    assert_eq!(PLAYBACK_SPEEDS.len(), 10);
    assert_eq!((PLAYBACK_SPEEDS[3], PLAYBACK_SPEEDS[9]), (1.0, 64.0));
}

#[test]
fn a_player_needs_a_frame_and_reads_a_loaded_recording() {
    assert!(Player::new(Vec::new(), None).is_none());
    // Frames at 100 … 164 (K = 16); the frame at 132 has no phase; one transition
    // `rendezvous → approach` at 116.
    let dir = TempDir::new("player_from_recording");
    let recording = read_recording(&valid_recording(&dir, RecordingSource::Cli)).unwrap();
    let with = Player::from_recording(&recording, true).unwrap();
    assert_eq!((with.start_step(), with.end_step()), (100, 164));
    assert_eq!(with.view().step, 100.0);
    assert_eq!(with.state_at(140.0).phase.as_deref(), Some("approach"));
    let without = Player::from_recording(&recording, false).unwrap();
    assert_eq!(without.state_at(140.0).phase, None, "the frame at 132's");
    assert_eq!(
        with.state_at(148.0).bodies,
        recording.frames[3].bodies,
        "a frame is shown bit for bit"
    );
}
