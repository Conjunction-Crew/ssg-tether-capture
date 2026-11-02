//! Behavioural acceptance of the capture (P7.5 S7, S9; design D12–D16, the plan's
//! "O2 revision" and "O4 revision"): what the operator asked to see, as five
//! `#[ignore]`d tests on the Q13 scenario with the example plan (the O4 plan).
//!
//! - `tether_taut_in_rendezvous`: the rope stays taut behind the root (N1).
//! - `approach_range_never_grows`: no overshoot; the root alone pushes through
//!   `approach` and `final_approach`, and enters `terminal` slow (N6).
//! - `tether_flung_out_while_circling`: the rope is flung outward and clear of the
//!   model while the ends circle at a constant speed (N3).
//! - `capture_phase_wraps_with_contacts`: the counter-tangent tail push wraps the
//!   rope around the centre of the bus, the ends held in the circling plane (N4).
//! - `hold_phase_stops_the_ends`: in `hold` the ends come to rest and the wrap holds
//!   (N5).
//!
//! The thresholds are the plan's (behavioural acceptance v3, from design D15's
//! H10b/H11b with margin); a failing one is a stop for the planner (rule 30), never
//! a re-fit. Rule 31: a panic inside avian fails the test; the step it happened at
//! is printed before the message.
//!
//! Sampling follows the planner's probes (`probe_o2`, `probe_o4`) so the numbers
//! line up with D12 and D15: `ticks` counts the ticks after `q13::build`; the range,
//! the winding and the rope's shape are sampled when `ticks` is a multiple of 64,
//! the attitude and the force-reversal windows when it is a multiple of 6,400.

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
use crate::tests::tether_dynamics::{
    chain, clearance_from_debris, debris_pose, debris_world_boxes, position, velocity,
};

/// The range and the winding are sampled every this many ticks.
const SAMPLE_EVERY: u64 = 64;
/// The attitude, the speed and the force reversals: every this many ticks.
const WINDOW: u64 = 6_400;
/// The capture window: the capture entry and this many steps beyond (the capture
/// phase's 90 s, then `hold`).
const CAPTURE_WINDOW_STEPS: u64 = 40_000;
/// Every run reaches the end of the capture window well inside this.
const MAX_TICKS: u64 = 260_000;
/// The tether's length, m (the example plan's `tether_length_m`).
const TETHER_LENGTH_M: f64 = 20.0;
/// The bus spans ±1.5 m along the array axis (the debris body's Z, design D13): a
/// node–debris contact at a larger |z| is on an array's inner end.
const BUS_HALF_Z_M: f64 = 1.5;

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
        self.lvlh_speed(self.handles.root)
    }

    /// A body's speed relative to the target in the target's LVLH frame.
    fn lvlh_speed(&self, body: Entity) -> f64 {
        let debris = self.handles.debris;
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
                position(&self.app, body) - position(&self.app, debris),
                velocity(&self.app, body) - velocity(&self.app, debris),
            )
            .length()
    }

    /// The rope's shape now (design D15's metrics, the planner's `o4_sample`).
    fn shape(&self) -> Shape {
        let (root, tail, debris) = (self.handles.root, self.tail, self.handles.debris);
        let (p_d, r_d) = debris_pose(&self.app, debris);
        let up = (r_d * CAPTURE_CIRCLING_AXIS).normalize_or(CAPTURE_CIRCLING_AXIS);
        let (p_root, p_tail) = (position(&self.app, root), position(&self.app, tail));
        let axis = (p_tail - p_root).normalize_or_zero();
        let end_range = (p_root - p_d).length().max((p_tail - p_d).length());
        let mut shape = Shape {
            slack: TETHER_LENGTH_M - (p_tail - p_root).length(),
            lateral: 0.0,
            bulge: f64::NEG_INFINITY,
            clearance: f64::INFINITY,
            h_root: (p_root - p_d).dot(up),
            h_tail: (p_tail - p_d).dot(up),
            tail_relative_speed: (velocity(&self.app, tail) - velocity(&self.app, root)).length(),
        };
        let last = self.chain.len() - 1;
        for (i, &body) in self.chain.iter().enumerate() {
            let p = position(&self.app, body);
            let d = p - p_root;
            shape.lateral = shape.lateral.max((d - axis * d.dot(axis)).length());
            shape.clearance = shape
                .clearance
                .min(clearance_from_debris(&self.app, body, debris));
            if i != 0 && i != last {
                shape.bulge = shape.bulge.max((p - p_d).length() - end_range);
            }
        }
        shape
    }

    /// Whether this step has a pair between a tether body and the debris whose tether
    /// body lies at |z| ≥ [`BUS_HALF_Z_M`] in the debris's axes (off the bus), and
    /// whether it has any tether–debris pair: `(any, off the bus)`.
    fn debris_contact_off_the_bus(&self) -> (bool, bool) {
        let (p_d, r_d) = debris_pose(&self.app, self.handles.debris);
        let (mut any, mut off) = (false, false);
        for pair in self
            .app
            .world()
            .resource::<ContactGraph>()
            .iter_active_touching()
        {
            let body = match (
                self.chain.contains(&pair.collider1),
                self.chain.contains(&pair.collider2),
            ) {
                (true, false) => pair.collider1,
                (false, true) => pair.collider2,
                _ => continue,
            };
            any = true;
            let z = (r_d.inverse() * (position(&self.app, body) - p_d)).z.abs();
            off |= z >= BUS_HALF_Z_M;
        }
        (any, off)
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

/// The rope's shape at one sample (design D15's metrics).
#[derive(Debug, Clone, Copy)]
struct Shape {
    /// 20 m minus the root→tail distance: the rope's slack, m.
    slack: f64,
    /// The largest distance of a chain body from the root→tail line, m.
    lateral: f64,
    /// How far the farthest interior node lies beyond the farther end's range from
    /// the debris centre, m (positive: the rope is flung out beyond the ends).
    bulge: f64,
    /// The smallest clearance of any chain body from the debris's boxes, m.
    clearance: f64,
    /// The ends' height above the circling plane through the debris centre, m.
    h_root: f64,
    h_tail: f64,
    /// The tail's speed relative to the root, m/s.
    tail_relative_speed: f64,
}

/// Minimum, mean and maximum of a sampled quantity.
#[derive(Debug, Clone, Copy)]
struct Stat {
    n: u64,
    sum: f64,
    min: f64,
    max: f64,
}

impl Default for Stat {
    fn default() -> Self {
        Self {
            n: 0,
            sum: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }
}

impl Stat {
    fn add(&mut self, v: f64) {
        self.n += 1;
        self.sum += v;
        self.min = self.min.min(v);
        self.max = self.max.max(v);
    }

    fn mean(&self) -> f64 {
        self.sum / self.n as f64
    }
}

impl std::fmt::Display for Stat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:.3}/{:.3}/{:.3} (min/mean/max of {})",
            self.min,
            self.mean(),
            self.max,
            self.n
        )
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

/// "Control the tail so the tether stays taut behind the root" in rendezvous (O4
/// walk N1; E26): from the capture start to the approach entry the rope's slack
/// (20 m − root→tail) is at most 2.0 m at every 64-tick sample, and the root's
/// force reverses fewer than 250 times in every 6,400-tick window. The tail's speed
/// relative to the root and the lateral sag are printed.
#[test]
#[ignore = "P7.5 behaviour (~1 min): cargo test -p ssg-sim --locked -- --ignored --nocapture tether_taut_in_rendezvous"]
fn tether_taut_in_rendezvous() {
    let mut run = Run::new();
    let start = run.step;
    let mut root = Reversals::default();
    let (mut slack, mut lateral, mut tail_relative) =
        (Stat::default(), Stat::default(), Stat::default());
    let mut slack_over: Vec<(u64, f64)> = Vec::new();
    // (window end step, root reversals)
    let mut windows: Vec<(u64, u64)> = Vec::new();
    loop {
        let changed = run.tick();
        root.feed(run.force(run.handles.root).applied, !changed);
        if changed {
            windows.push((run.step, root.take()));
            break;
        }
        if run.ticks.is_multiple_of(SAMPLE_EVERY) {
            let shape = run.shape();
            slack.add(shape.slack);
            lateral.add(shape.lateral);
            tail_relative.add(shape.tail_relative_speed);
            if shape.slack > 2.0 {
                slack_over.push((run.step, shape.slack));
            }
        }
        if run.ticks.is_multiple_of(WINDOW) {
            windows.push((run.step, root.take()));
        }
    }
    assert_eq!(run.state, "approach", "rendezvous ends in approach");
    println!(
        "tether_taut_in_rendezvous: steps {start}..{} (approach entry); slack m {slack}; \
         lateral sag m {lateral}; tail speed relative to the root m/s {tail_relative}; root \
         force reversals per window (end step, count) {windows:?}",
        run.step
    );
    assert!(slack.n > 0, "rendezvous was sampled");
    assert_eq!(
        slack_over,
        Vec::new(),
        "samples with more than 2.0 m of slack"
    );
    for (end, reversals) in &windows {
        assert!(
            *reversals < 250,
            "{reversals} root force reversals in the window ending at step {end}"
        );
    }
}

/// "Approach actuated only from the root", no overshoot (O2 walk F1; E8), and "slow
/// … and roughly zero its relative velocity before terminal" (O4 walk N6; E25):
/// the phases run rendezvous → approach → final_approach → terminal; from the
/// approach entry to the terminal entry the root's range, sampled every 64 ticks,
/// never exceeds the previous sample, the closest approach is the last sample, and
/// the tail's applied force is zero on every approach and final-approach tick; the
/// root enters terminal at 120 ± 1 m and below 0.5 m/s (LVLH).
#[test]
#[ignore = "P7.5 behaviour (~3 min): cargo test -p ssg-sim --locked -- --ignored --nocapture approach_range_never_grows"]
fn approach_range_never_grows() {
    let mut run = Run::new();
    let mut order = vec![run.state.clone()];
    run.tick_until("approach");
    order.push(run.state.clone());
    let approach_entry = run.step;
    let mut final_approach_entry = None;
    let mut last = run.range_m;
    let mut samples = 0u64;
    let mut growing: Vec<(u64, f64, f64)> = Vec::new();
    let mut closest = f64::INFINITY;
    let mut tail_pushed: Vec<u64> = Vec::new();
    let mut peak_relative_speed = 0.0f64;
    loop {
        if run.tick() {
            order.push(run.state.clone());
            if run.state != "final_approach" {
                break;
            }
            final_approach_entry = Some((run.step, run.range_m, run.root_lvlh_speed()));
        }
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
    let entry_speed = run.root_lvlh_speed();
    println!(
        "approach_range_never_grows: phases {order:?}; approach entry step {approach_entry}, \
         final_approach entry (step, range m, root LVLH speed m/s) {final_approach_entry:.3?}, \
         terminal entry step {} at range {:.3} m and root LVLH speed {entry_speed:.3} m/s; \
         {samples} samples, {} growing {growing:?}; the last sample {last:.3} m, the closest \
         {closest:.3} m; the tail pushed on {} approach ticks; peak node speed relative to \
         the root {peak_relative_speed:.3} m/s",
        run.step,
        run.range_m,
        growing.len(),
        tail_pushed.len(),
    );
    assert_eq!(
        order,
        ["rendezvous", "approach", "final_approach", "terminal"],
        "the phase order"
    );
    assert!(samples > 0, "the approach was sampled");
    assert_eq!(growing, Vec::new(), "approach samples with a growing range");
    assert_eq!(last, closest, "the closest approach is the phases' end");
    assert_eq!(tail_pushed, Vec::<u64>::new(), "steps with a tail force");
    assert!(
        (119.0..=121.0).contains(&run.range_m),
        "terminal entered at {:.3} m, not 120 ± 1 m",
        run.range_m
    );
    assert!(
        entry_speed < 0.5,
        "terminal entered at a root LVLH speed of {entry_speed:.3} m/s"
    );
}

/// "The slack should whip out … and stay out as the sphere shrinks", clear of the
/// RSO (O4 walk N3; E24, E29): through `terminal`, at every 64-tick sample the
/// smallest clearance of any chain body from the model is at least 1.0 m, and no
/// tether body touches the debris before the capture phase; the bulge's mean over
/// the phase is at least 1.0 m and over the samples with R < 24 m at least 2.0 m;
/// the attitude's mean over the 6,400-tick samples while R ≥ 12 m is 60–125° (no
/// per-sample band, E29); the root's LVLH speed at the first 64-tick sample with
/// R ≤ 6 m is at least 0.3 m/s (the limit is constant); the tail is pushed on every
/// terminal tick; and the root's force reverses fewer than 250 times in every
/// 6,400-tick window.
#[test]
#[ignore = "P7.5 behaviour (~3 min): cargo test -p ssg-sim --locked -- --ignored --nocapture tether_flung_out_while_circling"]
fn tether_flung_out_while_circling() {
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
    // (step, R, attitude °, root LVLH speed m/s) every 6,400 ticks.
    let mut attitudes: Vec<(u64, f64, f64, f64)> = Vec::new();
    // (window end step, root reversals, tail reversals)
    let mut windows: Vec<(u64, u64, u64)> = Vec::new();
    let (mut clearance, mut bulge, mut bulge_small_r, mut root_speed) = (
        Stat::default(),
        Stat::default(),
        Stat::default(),
        Stat::default(),
    );
    let mut too_close: Vec<(u64, f64)> = Vec::new();
    let mut speed_at_6m: Option<(u64, f64, f64)> = None;
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
        if run.ticks.is_multiple_of(SAMPLE_EVERY) {
            let shape = run.shape();
            let r = run.sphere_radius();
            let speed = run.root_lvlh_speed();
            clearance.add(shape.clearance);
            if shape.clearance < 1.0 {
                too_close.push((run.step, shape.clearance));
            }
            bulge.add(shape.bulge);
            if r < 24.0 {
                bulge_small_r.add(shape.bulge);
            }
            root_speed.add(speed);
            if r <= 6.0 {
                speed_at_6m.get_or_insert((run.step, r, speed));
            }
        }
        if run.ticks.is_multiple_of(WINDOW) {
            windows.push((run.step, root.take(), tail.take()));
            attitudes.push((
                run.step,
                run.sphere_radius(),
                run.attitude(),
                run.root_lvlh_speed(),
            ));
        }
    }
    assert_eq!(run.state, "capture", "terminal ends in capture");
    let circling: Vec<f64> = attitudes
        .iter()
        .filter(|(_, r, ..)| *r >= 12.0)
        .map(|(_, _, att, _)| *att)
        .collect();
    let mean = circling.iter().sum::<f64>() / circling.len() as f64;
    let root_counts: Vec<u64> = windows.iter().map(|w| w.1).collect();
    println!(
        "tether_flung_out_while_circling: terminal entry step {terminal_entry}, capture entry \
         step {}; samples every 6,400 ticks (step, R m, attitude deg, root LVLH speed m/s) \
         {attitudes:.3?}",
        run.step
    );
    println!(
        "  every 64 ticks: clearance m {clearance}; bulge m {bulge}; bulge with R < 24 m \
         {bulge_small_r}; root LVLH speed m/s {root_speed}; the first sample with R <= 6 m \
         (step, R m, root LVLH speed m/s) {speed_at_6m:.3?}"
    );
    println!(
        "  attitude while R >= 12 m: {} samples, {:.1}–{:.1} deg, mean {mean:.1} deg",
        circling.len(),
        circling.iter().copied().fold(f64::INFINITY, f64::min),
        circling.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    println!(
        "  force reversals per window (end step, root, tail): {windows:?}; root {}–{}",
        root_counts.iter().min().unwrap(),
        root_counts.iter().max().unwrap(),
    );
    println!(
        "  tether-debris pair-steps before the capture {tether_debris_steps} (first \
         {first_tether_debris:?}); terminal tether-tether pair-steps {tether_tether_steps}; \
         terminal ticks without a tail force {}; samples with a clearance below 1.0 m {}",
        tail_unpushed.len(),
        too_close.len(),
    );

    assert_eq!(
        too_close,
        Vec::new(),
        "terminal samples with a chain body within 1.0 m of the model"
    );
    assert_eq!(
        (tether_debris_steps, first_tether_debris),
        (0, None),
        "tether-debris pairs before the capture phase"
    );
    assert!(
        bulge.mean() >= 1.0,
        "the bulge's mean over terminal {:.3} m < 1.0 m",
        bulge.mean()
    );
    assert!(
        bulge_small_r.n > 0 && bulge_small_r.mean() >= 2.0,
        "the bulge's mean while R < 24 m {:.3} m < 2.0 m ({} samples)",
        bulge_small_r.mean(),
        bulge_small_r.n
    );
    assert!(!circling.is_empty(), "attitude samples while R >= 12 m");
    assert!(
        (60.0..=125.0).contains(&mean),
        "the mean attitude {mean:.1} deg is outside 60–125 deg"
    );
    let (step, r, speed) = speed_at_6m.expect("a terminal sample with R <= 6 m");
    assert!(
        speed >= 0.3,
        "root LVLH speed {speed:.3} m/s at R = {r:.2} m (step {step}) < 0.3 m/s"
    );
    assert_eq!(
        tail_unpushed,
        Vec::<u64>::new(),
        "terminal ticks without a tail force"
    );
    for (end, root_reversals, _) in &windows {
        assert!(
            *root_reversals < 250,
            "{root_reversals} root force reversals in the window ending at step {end}"
        );
    }
}

/// "Initiate capture by actuating the tail in the opposite direction causing a
/// wrap", with collisions (O2 walk F3, F4; E9–E13b, C6), on "the centre section"
/// (O4 walk N4; E27): over the capture phase (its 90 s, to the `hold` entry) the
/// rope's `max |winding|` about the circling axis is at least one turn, nodes touch
/// the bus on at least 2,000 steps, at most 10 % of the node–debris contact steps
/// have a tether body at |z| ≥ 1.5 m (off the bus), both ends stay within ±1.0 m of
/// the circling plane at every 64-tick sample, the rope touches itself at least once
/// in the run, the tail is pushed against the circling sense, and neither end
/// touches the debris in approach, final approach or terminal. The ends' and the
/// arrays' capture-phase contacts are printed, not asserted (E13b).
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
    let (mut h_root, mut h_tail) = (Stat::default(), Stat::default());
    let mut off_plane: Vec<(u64, f64, f64)> = Vec::new();
    // Capture-phase steps with a tether–debris pair, and those with one off the bus.
    let (mut contact_steps, mut off_bus_steps) = (0u64, 0u64);
    // The first capture tick with both ends pushed and the tail in its tangent
    // branch: (step, the tail's force along its tangent, the root's along its own).
    let mut counter_tangent: Option<(u64, f64, f64)> = None;
    loop {
        if run.tick() {
            if capture_entry.is_some() {
                break;
            }
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
        if capture_entry.is_none() {
            continue;
        }
        let (any, off) = run.debris_contact_off_the_bus();
        contact_steps += u64::from(any);
        off_bus_steps += u64::from(off);
        if run.ticks.is_multiple_of(SAMPLE_EVERY) {
            windings.push((run.step, run.winding()));
            let shape = run.shape();
            h_root.add(shape.h_root);
            h_tail.add(shape.h_tail);
            if shape.h_root.abs() > 1.0 || shape.h_tail.abs() > 1.0 {
                off_plane.push((run.step, shape.h_root, shape.h_tail));
            }
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
    }
    let hold_entry = (run.state == "hold").then_some(run.step);
    let phase = |name: &str| {
        phases
            .iter()
            .find(|(phase, ..)| phase == name)
            .map(|(_, steps, peak)| (*steps, *peak))
            .unwrap_or_default()
    };
    let (capture, capture_peak) = phase("capture");
    let tether_tether: usize = phases.iter().map(|(_, steps, _)| steps.tether_tether).sum();
    let positive = windings
        .iter()
        .map(|w| w.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let negative = windings.iter().map(|w| w.1).fold(f64::INFINITY, f64::min);
    let max_abs = positive.abs().max(negative.abs());
    let at_peak = windings.iter().find(|w| w.1.abs() == max_abs).map(|w| w.0);
    let off_bus_share = off_bus_steps as f64 / contact_steps.max(1) as f64;
    println!(
        "capture_phase_wraps_with_contacts: capture entry step {capture_entry:?}, hold entry \
         step {hold_entry:?}; max |winding| before the hold {max_abs:.3} turns (signed peaks \
         {positive:+.3} / {negative:+.3}; at step {at_peak:?}); first tether-debris contact \
         {first_contact:?}"
    );
    for (name, steps, peak) in &phases {
        println!("  {name}: pair-steps {steps:?}; peak pairs {peak:?}");
    }
    println!(
        "  capture phase: node-bus {} node-array {} tether-tether {} pair-steps; root {} and \
         tail {} debris pair-steps (reported, not asserted); tether-tether in the run \
         {tether_tether}; peak pairs {capture_peak:?}",
        capture.node_bus,
        capture.node_array,
        capture.tether_tether,
        capture.root_debris,
        capture.tail_debris,
    );
    println!(
        "  capture phase: {contact_steps} steps with a tether-debris pair, {off_bus_steps} of \
         them with a tether body at |z| >= {BUS_HALF_Z_M} m ({:.1} %); the ends' height above \
         the circling plane m: root {h_root}, tail {h_tail}; samples off ±1.0 m {off_plane:?}",
        100.0 * off_bus_share
    );
    println!(
        "  the first capture tick with both ends pushed and the tail on its tangent branch: \
         {counter_tangent:?} (step, the tail's force along its tangent N, the root's along \
         its own N)"
    );

    assert!(capture_entry.is_some(), "the run reached the capture phase");
    assert!(hold_entry.is_some(), "the capture phase ends in hold");
    assert!(
        max_abs >= 1.0,
        "max |winding| before the hold {max_abs:.3} turns < 1.0"
    );
    assert!(
        capture.node_bus >= 2_000,
        "node-bus pair-steps {} < 2,000",
        capture.node_bus
    );
    assert!(
        off_bus_share <= 0.10,
        "{off_bus_steps} of {contact_steps} capture contact steps off the bus ({:.1} % > 10 %)",
        100.0 * off_bus_share
    );
    assert_eq!(
        off_plane,
        Vec::new(),
        "capture samples with an end more than 1.0 m off the circling plane"
    );
    assert!(tether_tether >= 1, "no tether-tether contact in the run");
    let (step, tail_along, _) = counter_tangent.expect("a capture tick with both ends pushed");
    assert!(
        tail_along < 0.0,
        "at step {step} the tail is pushed along its tangent ({tail_along} N), not against it"
    );
    for name in ["approach", "final_approach", "terminal"] {
        let (steps, _) = phase(name);
        assert_eq!(
            (steps.root_debris, steps.tail_debris),
            (0, 0),
            "root and tail debris pair-steps in {name}"
        );
    }
}

/// "Once the capture is complete the nodes must change their actuation" (O4 walk
/// N5; E28): from the `hold` entry to the end of the capture window (the capture
/// entry and 40,000 steps) the ends' LVLH speed, sampled every 64 ticks after the
/// hold's first minute, has a mean below 0.1 m/s for each end, each end's force
/// reverses fewer than 50 times, and the rope's |winding| is at least 0.5 turn at
/// every sample. The ends' creep from their hold-entry positions is printed.
#[test]
#[ignore = "P7.5 behaviour (~3 min): cargo test -p ssg-sim --locked -- --ignored --nocapture hold_phase_stops_the_ends"]
fn hold_phase_stops_the_ends() {
    const FIRST_MINUTE_TICKS: u64 = 60 * 64;
    let mut run = Run::new();
    run.tick_until("capture");
    let capture_entry = run.step;
    run.tick_until("hold");
    let hold_entry = run.step;
    let hold_ticks = run.ticks;
    let debris = run.handles.debris;
    let relative = |run: &Run, body: Entity| position(&run.app, body) - position(&run.app, debris);
    let (root_at_entry, tail_at_entry) =
        (relative(&run, run.handles.root), relative(&run, run.tail));
    let winding_at_entry = run.winding();
    let mut root = Reversals::default();
    let mut tail = Reversals::default();
    let (mut root_speed, mut tail_speed, mut winding) =
        (Stat::default(), Stat::default(), Stat::default());
    let mut unwound: Vec<(u64, f64)> = Vec::new();
    while run.step < capture_entry + CAPTURE_WINDOW_STEPS {
        assert!(!run.tick(), "hold is an end phase: no transition out of it");
        root.feed(run.force(run.handles.root).applied, true);
        tail.feed(run.force(run.tail).applied, true);
        if run.ticks.is_multiple_of(SAMPLE_EVERY) {
            let w = run.winding();
            winding.add(w);
            if w.abs() < 0.5 {
                unwound.push((run.step, w));
            }
            if run.ticks - hold_ticks > FIRST_MINUTE_TICKS {
                root_speed.add(run.root_lvlh_speed());
                tail_speed.add(run.lvlh_speed(run.tail));
            }
        }
    }
    let (root_reversals, tail_reversals) = (root.take(), tail.take());
    let creep = (
        (relative(&run, run.handles.root) - root_at_entry).length(),
        (relative(&run, run.tail) - tail_at_entry).length(),
    );
    println!(
        "hold_phase_stops_the_ends: capture entry step {capture_entry}, hold entry step \
         {hold_entry}, window to step {}; after the first minute the ends' LVLH speed m/s: \
         root {root_speed}, tail {tail_speed}; force reversals root {root_reversals} tail \
         {tail_reversals}; winding turns {winding} ({winding_at_entry:+.3} at the hold entry, \
         {:+.3} at the end); samples below 0.5 turn {unwound:?}; the ends' creep from their \
         hold-entry positions m: root {:.3}, tail {:.3}",
        run.step,
        run.winding(),
        creep.0,
        creep.1,
    );
    assert!(
        root_speed.n > 0,
        "the hold was sampled after its first minute"
    );
    assert!(
        root_speed.mean() < 0.1 && tail_speed.mean() < 0.1,
        "the ends' mean LVLH speed after the first minute: root {:.3} m/s, tail {:.3} m/s",
        root_speed.mean(),
        tail_speed.mean()
    );
    assert!(
        root_reversals < 50 && tail_reversals < 50,
        "force reversals in hold: root {root_reversals}, tail {tail_reversals}"
    );
    assert_eq!(
        unwound,
        Vec::new(),
        "hold samples with |winding| below 0.5 turn"
    );
}
