//! Behavioural acceptance of the capture (P7.5 S7; design D12–D14, the plan's "O2
//! revision"): what the operator asked to see, as three `#[ignore]`d tests on the
//! Q13 scenario with the example plan.
//!
//! - `approach_range_never_grows`: no overshoot; the root alone pushes in approach.
//! - `tether_near_tangent_while_circling`: the rope lies near the circle while the
//!   sphere is large, the speed falls with the sphere, nothing touches the debris
//!   before the capture phase, and the root's force no longer reverses every step.
//! - `capture_phase_wraps_with_contacts`: the counter-tangent tail push wraps the
//!   rope around the bus with node–bus, node–array and tether–tether contacts, and
//!   the ends stay clear of the debris before the capture phase.
//!
//! The thresholds are the plan's (from design D12's G2b with margin); a failing one
//! is a stop for the planner (rule 30), never a re-fit. Rule 31: a panic inside
//! avian fails the test; the step it happened at is printed before the message.
//!
//! Sampling follows the planner's probe (`probe_o2`) so the numbers line up with
//! D12: `ticks` counts the ticks after `q13::build`; the range and the winding are
//! sampled when `ticks` is a multiple of 64, the attitude, the speed and the
//! force-reversal windows when it is a multiple of 6,400.

use std::cell::Cell;
use std::sync::Once;

use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

use crate::capture::{CAPTURE_CIRCLING_AXIS, CaptureForce, CaptureSphereRadius};
use crate::orbit::cache::OrbitalCache;
use crate::orbit::lvlh::LvlhFrame;
use crate::physics_bubble::body_eci_state;
use crate::test_support::{q13, test_app, tick};
use crate::tests::tether_dynamics::{chain, debris_pose, debris_world_boxes, position, velocity};

/// The range and the winding are sampled every this many ticks.
const SAMPLE_EVERY: u64 = 64;
/// The attitude, the speed and the force reversals: every this many ticks.
const WINDOW: u64 = 6_400;
/// The capture window: the capture entry and this many steps beyond.
const CAPTURE_WINDOW_STEPS: u64 = 40_000;
/// Every run reaches the capture phase well inside this (G2b: step 167,091).
const MAX_TICKS: u64 = 260_000;

// ---------------------------------------------------------------------------
// Rule 31: name the step of a panic
// ---------------------------------------------------------------------------

thread_local! {
    /// The step this thread is computing, while a behavioural run ticks.
    static STEP: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Installs, once per process, a panic hook that prints the step a behavioural run
/// was computing before the default message (the hook only speaks on the threads
/// of these tests). Nothing is caught: the test fails as the panic says.
fn report_panics_with_the_step() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Some(step) = STEP.with(Cell::get) {
                eprintln!(
                    "capture_behaviour: panic while computing step {step} \
                     (rule 31: a panic inside avian is a stop)"
                );
            }
            previous(info);
        }));
    });
}

// ---------------------------------------------------------------------------
// The metric helpers (the planner's probe, design D12)
// ---------------------------------------------------------------------------

/// The angle between root→tail and the outward radial at the root (debris →
/// root), degrees: 0° the rope points radially outward, 90° along the circle,
/// 180° radially inward.
fn attitude_deg(app: &App, root: Entity, tail: Entity, debris: Entity) -> f64 {
    (position(app, tail) - position(app, root))
        .angle_between(position(app, root) - position(app, debris))
        .to_degrees()
}

/// The rope's winding about `CAPTURE_CIRCLING_AXIS` through the debris centre, in
/// turns: the sum of the signed angle steps between consecutive chain bodies,
/// projected on the plane normal to the axis (the probe's fixed sim axis).
fn winding_turns(app: &App, chain: &[Entity], debris: Entity) -> f64 {
    use std::f64::consts::{PI, TAU};
    let axis = CAPTURE_CIRCLING_AXIS;
    let e1 = axis.any_orthonormal_vector();
    let e2 = axis.cross(e1);
    let centre = position(app, debris);
    let angle = |body: Entity| {
        let v = position(app, body) - centre;
        let p = v - axis * v.dot(axis);
        p.dot(e2).atan2(p.dot(e1))
    };
    let mut total = 0.0;
    for pair in chain.windows(2) {
        let mut step = angle(pair[1]) - angle(pair[0]);
        while step > PI {
            step -= TAU;
        }
        while step < -PI {
            step += TAU;
        }
        total += step;
    }
    total / TAU
}

/// This step's touching pairs by role. The debris body's own collider is the bus;
/// a child collider of the debris is an array.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Contacts {
    root_debris: usize,
    tail_debris: usize,
    node_bus: usize,
    node_array: usize,
    tether_tether: usize,
}

impl Contacts {
    fn tether_debris(&self) -> usize {
        self.root_debris + self.tail_debris + self.node_bus + self.node_array
    }

    fn peak(&mut self, other: Contacts) {
        self.root_debris = self.root_debris.max(other.root_debris);
        self.tail_debris = self.tail_debris.max(other.tail_debris);
        self.node_bus = self.node_bus.max(other.node_bus);
        self.node_array = self.node_array.max(other.node_array);
        self.tether_tether = self.tether_tether.max(other.tether_tether);
    }

    /// Adds one pair-step for each role that has at least one pair this step.
    fn add_steps(&mut self, other: Contacts) {
        self.root_debris += usize::from(other.root_debris > 0);
        self.tail_debris += usize::from(other.tail_debris > 0);
        self.node_bus += usize::from(other.node_bus > 0);
        self.node_array += usize::from(other.node_array > 0);
        self.tether_tether += usize::from(other.tether_tether > 0);
    }
}

fn contacts_by_role(app: &App, chain: &[Entity], debris: Entity) -> Contacts {
    let (root, tail) = (chain[0], chain[chain.len() - 1]);
    let mut contacts = Contacts::default();
    for pair in app
        .world()
        .resource::<ContactGraph>()
        .iter_active_touching()
    {
        let (body, other) = match (
            chain.contains(&pair.collider1),
            chain.contains(&pair.collider2),
        ) {
            (true, true) => {
                contacts.tether_tether += 1;
                continue;
            }
            (true, false) => (pair.collider1, pair.collider2),
            (false, true) => (pair.collider2, pair.collider1),
            (false, false) => continue,
        };
        if body == root {
            contacts.root_debris += 1;
        } else if body == tail {
            contacts.tail_debris += 1;
        } else if other == debris {
            contacts.node_bus += 1;
        } else {
            contacts.node_array += 1;
        }
    }
    contacts
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

/// The Q13 scenario after `q13::build`, ticked one step at a time.
struct Run {
    app: App,
    handles: q13::Q13Handles,
    chain: Vec<Entity>,
    tail: Entity,
    /// Ticks since `q13::build` (the probe's count).
    ticks: u64,
    step: u64,
    state: String,
    range_m: f64,
    /// The debris's pose before the last tick (avian sets the array children's
    /// poses from it).
    step_start: (DVec3, DQuat),
}

impl Run {
    fn new() -> Self {
        report_panics_with_the_step();
        let mut app = test_app();
        let handles = q13::build(&mut app);
        let chain = chain(&mut app, handles.root);
        let tail = chain[chain.len() - 1];
        let sample = q13::sample(&app, &handles);
        let step_start = debris_pose(&app, handles.debris);
        Self {
            app,
            handles,
            chain,
            tail,
            ticks: 0,
            step: sample.step,
            state: sample.state,
            range_m: sample.range_m,
            step_start,
        }
    }

    /// One tick; returns whether the phase changed.
    fn tick(&mut self) -> bool {
        assert!(
            self.ticks < MAX_TICKS,
            "no capture phase in {MAX_TICKS} ticks"
        );
        self.step_start = debris_pose(&self.app, self.handles.debris);
        STEP.with(|step| step.set(Some(self.step + 1)));
        tick(&mut self.app);
        STEP.with(|step| step.set(None));
        self.ticks += 1;
        let sample = q13::sample(&self.app, &self.handles);
        let changed = sample.state != self.state;
        self.step = sample.step;
        self.state = sample.state;
        self.range_m = sample.range_m;
        changed
    }

    fn force(&self, body: Entity) -> CaptureForce {
        *self
            .app
            .world()
            .get::<CaptureForce>(body)
            .expect("CaptureForce on the root and the tail")
    }

    fn contacts(&self) -> Contacts {
        contacts_by_role(&self.app, &self.chain, self.handles.debris)
    }

    fn sphere_radius(&self) -> f64 {
        self.app.world().resource::<CaptureSphereRadius>().radius
    }

    /// The root's speed relative to the target in the target's LVLH frame, as the
    /// controller measures it (`rel_v − ω × rel_r`; the HUD's relative speed).
    fn root_lvlh_speed(&self) -> f64 {
        let (debris, root) = (self.handles.debris, self.handles.root);
        let world = self.app.world();
        let target = body_eci_state(
            world.resource::<OrbitalCache>(),
            debris,
            position(&self.app, debris),
            velocity(&self.app, debris),
            world.get::<RigidBodyDisabled>(debris).is_some(),
        )
        .expect("the target's cached ECI state");
        let frame = LvlhFrame::from_eci(&target).expect("the target's LVLH frame");
        frame
            .relative_velocity_in_frame(
                position(&self.app, root) - position(&self.app, debris),
                velocity(&self.app, root) - velocity(&self.app, debris),
            )
            .length()
    }

    fn attitude(&self) -> f64 {
        attitude_deg(&self.app, self.handles.root, self.tail, self.handles.debris)
    }

    fn winding(&self) -> f64 {
        winding_turns(&self.app, &self.chain, self.handles.debris)
    }

    /// The largest speed of a tether body relative to the root, m/s.
    fn peak_relative_speed(&self) -> f64 {
        let root_v = velocity(&self.app, self.handles.root);
        self.chain
            .iter()
            .map(|&body| (velocity(&self.app, body) - root_v).length())
            .fold(0.0, f64::max)
    }

    /// The three debris colliders' world boxes relative to the debris centre.
    fn print_debris_boxes(&mut self, when: &str) {
        for (what, centre, half, aabb_centre, aabb_half) in
            debris_world_boxes(&mut self.app, self.handles.debris, self.step_start)
        {
            println!(
                "debris {what} {when}: centre {centre:.6} half {half:.6} m \
                 (ColliderAabb centre {aabb_centre:.6} half {aabb_half:.6} m)"
            );
        }
    }

    fn tick_until(&mut self, phase: &str) {
        while self.state != phase {
            self.tick();
        }
    }
}

/// Counts the direction reversals of a body's applied force: a non-zero force whose
/// dot product with the previous non-zero force is negative.
#[derive(Debug, Default)]
struct Reversals {
    previous: Option<DVec3>,
    count: u64,
}

impl Reversals {
    /// Feeds this step's applied force; counts a reversal only when `counted`.
    fn feed(&mut self, applied: DVec3, counted: bool) {
        if applied == DVec3::ZERO {
            return;
        }
        if counted
            && self
                .previous
                .is_some_and(|previous| applied.dot(previous) < 0.0)
        {
            self.count += 1;
        }
        self.previous = Some(applied);
    }

    fn take(&mut self) -> u64 {
        std::mem::take(&mut self.count)
    }
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

/// "Approach actuated only from the root", no overshoot (O2 walk F1; E8): from the
/// approach entry to the terminal entry the root's range, sampled every 64 ticks,
/// never exceeds the previous sample, the closest approach is the last sample, and
/// the tail's applied force is zero on every approach tick.
#[test]
#[ignore = "P7.5 behaviour (~2 min): cargo test -p ssg-sim --locked -- --ignored --nocapture approach_range_never_grows"]
fn approach_range_never_grows() {
    let mut run = Run::new();
    run.tick_until("approach");
    let approach_entry = run.step;
    let mut last = run.range_m;
    let mut samples = 0u64;
    let mut growing: Vec<(u64, f64, f64)> = Vec::new();
    let mut closest = f64::INFINITY;
    let mut tail_pushed: Vec<u64> = Vec::new();
    let mut peak_relative_speed = 0.0f64;
    while !run.tick() {
        if run.force(run.tail).applied != DVec3::ZERO {
            tail_pushed.push(run.step);
        }
        if run.ticks.is_multiple_of(SAMPLE_EVERY) {
            samples += 1;
            if run.range_m > last + 1e-9 {
                growing.push((run.step, last, run.range_m));
            }
            last = run.range_m;
            closest = closest.min(run.range_m);
            peak_relative_speed = peak_relative_speed.max(run.peak_relative_speed());
        }
    }
    assert_eq!(run.state, "terminal", "approach ends in terminal");
    println!(
        "approach_range_never_grows: approach entry step {approach_entry}, terminal entry \
         step {} (range {:.3} m); {samples} samples, {} growing {growing:?}; the last \
         sample {last:.3} m, the closest {closest:.3} m; the tail pushed on {} approach \
         ticks; peak node speed relative to the root {peak_relative_speed:.3} m/s",
        run.step,
        run.range_m,
        growing.len(),
        tail_pushed.len(),
    );
    assert!(samples > 0, "the approach was sampled");
    assert_eq!(growing, Vec::new(), "approach samples with a growing range");
    assert_eq!(last, closest, "the closest approach is the phase's end");
    assert_eq!(tail_pushed, Vec::<u64>::new(), "steps with a tail force");
}

/// "Orbit the RSO with the tether roughly straight tangent" and "lower its
/// tangential velocity as the sphere shrinks" (O2 walk F2; E9, E17, E19, E22): in
/// `terminal`, every 6,400-tick sample while the sphere is at least 12 m has the
/// rope 45–140° off the outward radial and their mean is 80–125°; no tether body
/// touches the debris before the capture phase; the tail is pushed on every
/// terminal tick; the root's LVLH speed at the first sample with R ≤ 6 m is below
/// 0.2 m/s and below half its value at the first sample with R ≥ 20 m; and the
/// root's force reverses fewer than 3,200 times in every 6,400-tick window.
#[test]
#[ignore = "P7.5 behaviour (~2 min): cargo test -p ssg-sim --locked -- --ignored --nocapture tether_near_tangent_while_circling"]
fn tether_near_tangent_while_circling() {
    let mut run = Run::new();
    let mut root = Reversals::default();
    let mut tail = Reversals::default();
    let mut tether_debris_steps = 0u64;
    let mut first_tether_debris: Option<(u64, String)> = None;
    while run.state != "terminal" {
        run.tick();
        root.feed(run.force(run.handles.root).applied, false);
        tail.feed(run.force(run.tail).applied, false);
        if run.contacts().tether_debris() > 0 {
            tether_debris_steps += 1;
            first_tether_debris.get_or_insert((run.step, run.state.clone()));
        }
    }
    let terminal_entry = run.step;
    // (step, R, attitude °, root LVLH speed m/s, root speed in sim axes m/s)
    let mut samples: Vec<(u64, f64, f64, f64, f64)> = Vec::new();
    // (window end step, root reversals, tail reversals)
    let mut windows: Vec<(u64, u64, u64)> = Vec::new();
    let mut tail_unpushed: Vec<u64> = Vec::new();
    let mut tether_tether_steps = 0u64;
    loop {
        let changed = run.tick();
        let in_terminal = !changed;
        root.feed(run.force(run.handles.root).applied, in_terminal);
        tail.feed(run.force(run.tail).applied, in_terminal);
        let contacts = run.contacts();
        if !in_terminal {
            windows.push((run.step, root.take(), tail.take()));
            break;
        }
        if contacts.tether_debris() > 0 {
            tether_debris_steps += 1;
            first_tether_debris.get_or_insert((run.step, run.state.clone()));
        }
        tether_tether_steps += u64::from(contacts.tether_tether > 0);
        if run.force(run.tail).applied == DVec3::ZERO {
            tail_unpushed.push(run.step);
        }
        if run.ticks.is_multiple_of(WINDOW) {
            windows.push((run.step, root.take(), tail.take()));
            let plain = (velocity(&run.app, run.handles.root)
                - velocity(&run.app, run.handles.debris))
            .length();
            samples.push((
                run.step,
                run.sphere_radius(),
                run.attitude(),
                run.root_lvlh_speed(),
                plain,
            ));
        }
    }
    assert_eq!(run.state, "capture", "terminal ends in capture");
    let circling: Vec<f64> = samples
        .iter()
        .filter(|(_, r, ..)| *r >= 12.0)
        .map(|(_, _, att, ..)| *att)
        .collect();
    let mean = circling.iter().sum::<f64>() / circling.len() as f64;
    let speed_at = |pick: &dyn Fn(f64) -> bool| {
        samples
            .iter()
            .find(|(_, r, ..)| pick(*r))
            .map(|&(step, r, _, v, _)| (step, r, v))
    };
    let large = speed_at(&|r| r >= 20.0).expect("a sample with R >= 20 m");
    let small = speed_at(&|r| r <= 6.0).expect("a sample with R <= 6 m");
    let root_counts: Vec<u64> = windows.iter().map(|w| w.1).collect();
    println!(
        "tether_near_tangent_while_circling: terminal entry step {terminal_entry}, capture \
         entry step {}; samples (step, R m, attitude deg, root LVLH speed m/s, root speed \
         m/s) {samples:.3?}",
        run.step
    );
    println!(
        "  attitude while R >= 12 m: {} samples, {:.1}–{:.1} deg, mean {mean:.1} deg; root \
         LVLH speed {:.3} m/s at R = {:.1} m (step {}) against {:.3} m/s at R = {:.1} m \
         (step {})",
        circling.len(),
        circling.iter().copied().fold(f64::INFINITY, f64::min),
        circling.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        small.2,
        small.1,
        small.0,
        large.2,
        large.1,
        large.0,
    );
    println!(
        "  force reversals per window (end step, root, tail): {windows:?}; root {}–{}",
        root_counts.iter().min().unwrap(),
        root_counts.iter().max().unwrap(),
    );
    println!(
        "  tether-debris pair-steps before the capture {tether_debris_steps} (first \
         {first_tether_debris:?}); terminal tether-tether pair-steps {tether_tether_steps}; \
         terminal ticks without a tail force {}",
        tail_unpushed.len()
    );

    assert!(!circling.is_empty(), "attitude samples while R >= 12 m");
    for (step, r, att, ..) in samples.iter().filter(|(_, r, ..)| *r >= 12.0) {
        assert!(
            (45.0..=140.0).contains(att),
            "attitude {att:.1} deg at step {step} (R = {r:.2} m) is outside 45–140 deg"
        );
    }
    assert!(
        (80.0..=125.0).contains(&mean),
        "the mean attitude {mean:.1} deg is outside 80–125 deg"
    );
    assert_eq!(
        (tether_debris_steps, first_tether_debris),
        (0, None),
        "tether-debris pairs before the capture phase"
    );
    assert_eq!(
        tail_unpushed,
        Vec::<u64>::new(),
        "terminal ticks without a tail force"
    );
    assert!(
        small.2 < 0.2 && small.2 < 0.5 * large.2,
        "root LVLH speed {:.3} m/s at R = {:.2} m against {:.3} m/s at R = {:.2} m",
        small.2,
        small.1,
        large.2,
        large.1
    );
    for (end, root_reversals, _) in &windows {
        assert!(
            *root_reversals < 3_200,
            "{root_reversals} root force reversals in the window ending at step {end}"
        );
    }
}

/// "Initiate capture by actuating the tail in the opposite direction causing a
/// wrap", with collisions (O2 walk F3, F4; E9–E13b, C6): in the capture entry and
/// the 40,000 steps after it the rope's `max |winding|` about the circling axis is
/// at least one turn, nodes touch the bus on at least 2,000 steps and the arrays on
/// at least 10,000, the rope touches itself at least once in the run, the tail is
/// pushed against the circling sense, and neither end touches the debris in
/// approach or terminal. The ends' capture-phase contacts are printed, not asserted
/// (E13b: a capture-assessment metric for a later stage).
#[test]
#[ignore = "P7.5 behaviour (~3 min): cargo test -p ssg-sim --locked -- --ignored --nocapture capture_phase_wraps_with_contacts"]
fn capture_phase_wraps_with_contacts() {
    let mut run = Run::new();
    // Pair-steps per phase, in order of entry.
    let mut phases: Vec<(String, Contacts, Contacts)> =
        vec![(run.state.clone(), Contacts::default(), Contacts::default())];
    let mut first_contact: Option<(u64, String)> = None;
    let mut capture_entry: Option<u64> = None;
    let mut windings: Vec<(u64, f64)> = Vec::new();
    // The first capture tick with both ends pushed and the tail in its tangent
    // branch: (step, the tail's force along its tangent, the root's along its own).
    let mut counter_tangent: Option<(u64, f64, f64)> = None;
    loop {
        if run.tick() {
            phases.push((run.state.clone(), Contacts::default(), Contacts::default()));
            if run.state == "capture" {
                capture_entry = Some(run.step);
            }
        }
        let contacts = run.contacts();
        let (_, steps, peak) = phases.last_mut().expect("a phase");
        steps.add_steps(contacts);
        peak.peak(contacts);
        if contacts.tether_debris() > 0 && first_contact.is_none() {
            first_contact = Some((run.step, run.state.clone()));
            println!(
                "first tether-debris contact at step {} in {} (range {:.3} m): {contacts:?}",
                run.step, run.state, run.range_m
            );
            run.print_debris_boxes("at the first contact");
        }
        let Some(entry) = capture_entry else {
            continue;
        };
        if run.ticks.is_multiple_of(SAMPLE_EVERY) {
            windings.push((run.step, run.winding()));
        }
        let (root_force, tail_force) = (run.force(run.handles.root), run.force(run.tail));
        if counter_tangent.is_none()
            && root_force.applied != DVec3::ZERO
            && tail_force.applied != DVec3::ZERO
            && !(tail_force.inner || tail_force.outer || tail_force.kill)
        {
            let up = run
                .app
                .world()
                .get::<Rotation>(run.handles.debris)
                .expect("Rotation")
                .0
                * CAPTURE_CIRCLING_AXIS;
            let target = position(&run.app, run.handles.debris);
            let along = |body: Entity, force: DVec3| {
                force.dot(up.cross(target - position(&run.app, body)).normalize())
            };
            counter_tangent = Some((
                run.step,
                along(run.tail, tail_force.applied),
                along(run.handles.root, root_force.applied),
            ));
            println!(
                "  root branches at that tick: inner {} outer {} kill {}",
                root_force.inner, root_force.outer, root_force.kill
            );
        }
        if run.step >= entry + CAPTURE_WINDOW_STEPS {
            break;
        }
    }
    let phase = |name: &str| {
        phases
            .iter()
            .find(|(phase, ..)| phase == name)
            .map(|(_, steps, peak)| (*steps, *peak))
            .unwrap_or_default()
    };
    let (approach, _) = phase("approach");
    let (terminal, _) = phase("terminal");
    let (capture, capture_peak) = phase("capture");
    let tether_tether: usize = phases.iter().map(|(_, steps, _)| steps.tether_tether).sum();
    let positive = windings
        .iter()
        .map(|w| w.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let negative = windings.iter().map(|w| w.1).fold(f64::INFINITY, f64::min);
    let max_abs = positive.abs().max(negative.abs());
    let at_peak = windings.iter().find(|w| w.1.abs() == max_abs).map(|w| w.0);
    println!(
        "capture_phase_wraps_with_contacts: capture entry step {:?}, window to step {}; \
         max |winding| {max_abs:.3} turns (signed peaks {positive:+.3} / {negative:+.3}; \
         at step {at_peak:?}); first tether-debris contact {first_contact:?}",
        capture_entry, run.step
    );
    for (name, steps, peak) in &phases {
        println!("  {name}: pair-steps {steps:?}; peak pairs {peak:?}");
    }
    println!(
        "  capture window: node-bus {} node-array {} tether-tether {} pair-steps; root {} \
         and tail {} debris pair-steps (reported, not asserted); tether-tether in the run \
         {tether_tether}; peak pairs {capture_peak:?}",
        capture.node_bus,
        capture.node_array,
        capture.tether_tether,
        capture.root_debris,
        capture.tail_debris,
    );
    println!(
        "  the first capture tick with both ends pushed and the tail on its tangent branch: \
         {counter_tangent:?} (step, the tail's force along its tangent N, the root's along \
         its own N)"
    );

    assert!(capture_entry.is_some(), "the run reached the capture phase");
    assert!(max_abs >= 1.0, "max |winding| {max_abs:.3} turns < 1.0");
    assert!(
        capture.node_bus >= 2_000,
        "node-bus pair-steps {} < 2,000",
        capture.node_bus
    );
    assert!(
        capture.node_array >= 10_000,
        "node-array pair-steps {} < 10,000",
        capture.node_array
    );
    assert!(tether_tether >= 1, "no tether-tether contact in the run");
    let (step, tail_along, _) = counter_tangent.expect("a capture tick with both ends pushed");
    assert!(
        tail_along < 0.0,
        "at step {step} the tail is pushed along its tangent ({tail_along} N), not against it"
    );
    for (name, steps) in [("approach", approach), ("terminal", terminal)] {
        assert_eq!(
            (steps.root_debris, steps.tail_debris),
            (0, 0),
            "root and tail debris pair-steps in {name}"
        );
    }
}
