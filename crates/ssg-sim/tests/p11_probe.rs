//! P11 Run 0 probe (issue #173, cross-platform determinism). MEASUREMENT ONLY:
//! this file lives on the throwaway branch `178-p11/probe` and is never merged.
//!
//! It changes no sim code. It reads the sim through its public API and adds three
//! systems to the test app's `ManualPhysics` schedule: a read-only tap after
//! avian's solve; a tap after `SimStep::CacheEci` that, in `inject` mode,
//! overwrites `OrbitalCache.eci_states` with the states a reference platform
//! recorded; and a tap before avian's first solve that, in `inject` mode, pins
//! the bodies' spawn positions and the tether joints' limits to the reference
//! (the tether is laid along an axis taken from the start states). With both,
//! everything downstream of the orbit layer gets identical input on every
//! platform.
//!
//! Output: files under `$P11_PROBE_OUT` (default `<workspace>/target/p11-probe`)
//! and two stdout lines per run (`LEVERS {…}` in P4's format, `P11PROBE {…}`).
//!   start.tsv            bit patterns of the start-state chain and of library
//!                        functions on fixed inputs
//!   fn_table.tsv         every f64 math function, std and libm, on fixed inputs
//!   steps-<mode>.tsv     one line per step: a hash per quantity
//!   raw-<mode>.tsv       the bits behind the hashes at steps <= 12, every 640
//!                        ticks up to 6,400 and every 6,400 after that
//!   events-<mode>.tsv    transitions, floating-origin resets, bubble changes
//!   cache-<mode>.bin     `OrbitalCache` states per step, losslessly packed (the
//!                        inject reference is one of these)
//!   spawn-<mode>.tsv     positions and joint limits before the first solve
//! `<mode>` is `native` or `inject`.
//!
//! Environment: `P11_PROBE_OUT` (directory), `P11_PROBE_CONFIG` (a label, e.g.
//! `A`), `P11_PROBE_REFERENCE` (a `cache-native.bin`; default
//! `tests/fixtures/p11_cache_reference.bin`; no file, no inject run; the spawn
//! reference is always `tests/fixtures/p11_spawn_reference.tsv`),
//! `P11_PROBE_TICKS` (default 6400), `P11_PROBE_MODES` (default `native,inject`).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use avian3d::prelude::{
    AngularVelocity, DistanceJoint, LinearVelocity, PhysicsSystems, Position, RigidBody,
    RigidBodyDisabled, Rotation,
};
use bevy::math::DVec3;
use bevy::prelude::*;
use brahe::eop::{StaticEOPProvider, set_global_eop_provider};
use brahe::propagators::SGPPropagator;
use brahe::utils::{DOrbitStateProvider, SOrbitStateProvider, SStateProvider};
use brahe::{AngleFormat, Epoch, KeplerianPropagator};
use nalgebra::Vector6;
use ssg_sim::SimStep;
use ssg_sim::anchor::SimAnchor;
use ssg_sim::capture::{CaptureForce, CaptureTransition};
use ssg_sim::floating_origin::OriginReset;
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::catalog::gp_record_to_elements;
use ssg_sim::orbit::lvlh::{LvlhFrame, differential_gravity};
use ssg_sim::physics_bubble::body_eci_state;
use ssg_sim::test_support::{self, fixed_epoch, q13, test_app, tick};
use ssg_sim::time::{ManualPhysics, PHYS_DT, WorldTime};

const DEFAULT_TICKS: u64 = 6_400;
/// Marks "no value" in a bit dump (a body without a cached state, a body the
/// quantity does not apply to).
const NONE_BITS: u64 = u64::MAX;

// ---------------------------------------------------------------- helpers

/// FNV-1a over little-endian bytes: the same on every platform by construction.
struct Fnv(u64);

impl Fnv {
    fn of(bits: &[u64]) -> u64 {
        let mut h = Fnv(0xcbf2_9ce4_8422_2325);
        for b in bits {
            for byte in b.to_le_bytes() {
                h.0 ^= u64::from(byte);
                h.0 = h.0.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h.0
    }
}

fn hex(bits: &[u64]) -> String {
    let mut s = String::with_capacity(bits.len() * 17);
    for (i, b) in bits.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "{b:016x}");
    }
    s
}

fn v3(v: DVec3) -> [u64; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}

fn v6(v: &Vector6<f64>) -> [u64; 6] {
    [
        v[0].to_bits(),
        v[1].to_bits(),
        v[2].to_bits(),
        v[3].to_bits(),
        v[4].to_bits(),
        v[5].to_bits(),
    ]
}

fn platform() -> String {
    format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn out_dir() -> PathBuf {
    let dir = std::env::var_os("P11_PROBE_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| test_support::repo_root().join("target/p11-probe"));
    std::fs::create_dir_all(&dir).expect("create the probe output directory");
    dir
}

fn config_label() -> String {
    std::env::var("P11_PROBE_CONFIG").unwrap_or_else(|_| "unlabelled".to_string())
}

fn step_of(time: &WorldTime) -> u64 {
    ((time.epoch - time.start_epoch) / PHYS_DT).round() as u64
}

/// The bodies the cache names, in a platform-independent order that does not
/// rely on entity ids: the debris by name, then every tether's root and tail.
/// The cache record uses this order.
fn bodies(cache: &OrbitalCache) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut debris: Vec<(&String, &Entity)> = cache.debris.iter().collect();
    debris.sort_by(|a, b| a.0.cmp(b.0));
    out.extend(debris.into_iter().map(|(_, e)| *e));
    let mut tethers: Vec<(&String, &Vec<Entity>)> = cache.tethers.iter().collect();
    tethers.sort_by(|a, b| a.0.cmp(b.0));
    for (_, nodes) in tethers {
        out.extend(nodes.iter().copied());
    }
    out
}

/// Every rigid body, in the same kind of order: the debris by name, then every
/// tether from its root along the joints (`next`: a joint's first body to its
/// second) to its tail, interior nodes included. The per-step taps use this order.
fn all_bodies(cache: &OrbitalCache, next: &HashMap<Entity, Entity>) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut debris: Vec<(&String, &Entity)> = cache.debris.iter().collect();
    debris.sort_by(|a, b| a.0.cmp(b.0));
    out.extend(debris.into_iter().map(|(_, e)| *e));
    let mut tethers: Vec<(&String, &Vec<Entity>)> = cache.tethers.iter().collect();
    tethers.sort_by(|a, b| a.0.cmp(b.0));
    for (_, nodes) in tethers {
        let mut node = nodes[0];
        out.push(node);
        while let Some(following) = next.get(&node) {
            node = *following;
            out.push(node);
        }
        assert_eq!(
            Some(&node),
            nodes.last(),
            "the joints lead from the root to the tail"
        );
    }
    out
}

fn kinematics(world: &World, entity: Entity) -> [u64; 13] {
    let (Some(p), Some(r), Some(v), Some(w)) = (
        world.get::<Position>(entity),
        world.get::<Rotation>(entity),
        world.get::<LinearVelocity>(entity),
        world.get::<AngularVelocity>(entity),
    ) else {
        return [NONE_BITS; 13];
    };
    let (p, r, v, w) = (p.0, r.0, v.0, w.0);
    [
        p.x, p.y, p.z, r.x, r.y, r.z, r.w, v.x, v.y, v.z, w.x, w.y, w.z,
    ]
    .map(f64::to_bits)
}

// ------------------------------------------------------------ probe systems

type CacheRecord = BTreeMap<u64, Vec<[u64; 6]>>;

/// What the spawn leaves behind that the start states decide: each body's
/// position and each tether joint's distance limits (min, max), in chain order.
#[derive(Clone, Default, PartialEq)]
struct SpawnRecord {
    positions: Vec<[u64; 3]>,
    joints: Vec<[u64; 2]>,
}

#[derive(Resource, Default)]
struct Probe {
    /// [`all_bodies`], fixed before the first solve.
    order: Vec<Entity>,
    /// Inject mode: step -> the reference platform's cached state per body.
    inject: Option<CacheRecord>,
    /// Body states overwritten so far.
    injected: u64,
    /// Inject mode: the reference platform's spawn record.
    spawn_pin: Option<SpawnRecord>,
    /// Positions and joint limits overwritten before the first solve.
    pinned: u64,
    /// The spawn record before the first solve (after the pin, when there is one).
    spawn: Option<SpawnRecord>,
    /// step -> kinematics per body right after avian's solve.
    post_solve: BTreeMap<u64, Vec<[u64; 13]>>,
    /// step -> cached ECI state per body after `SimStep::CacheEci` (and after the
    /// injection, when there is one).
    cache: CacheRecord,
}

/// Read-only: runs after avian's solve and before the sim's own step systems.
fn tap_post_solve_system(
    time: Res<WorldTime>,
    q: Query<(&Position, &Rotation, &LinearVelocity, &AngularVelocity)>,
    mut probe: ResMut<Probe>,
) {
    let rows = probe
        .order
        .iter()
        .map(|e| match q.get(*e) {
            Ok((p, r, v, w)) => {
                let (p, r, v, w) = (p.0, r.0, v.0, w.0);
                [
                    p.x, p.y, p.z, r.x, r.y, r.z, r.w, v.x, v.y, v.z, w.x, w.y, w.z,
                ]
                .map(f64::to_bits)
            }
            Err(_) => [NONE_BITS; 13],
        })
        .collect();
    probe.post_solve.insert(step_of(&time), rows);
}

/// Once, before avian's first solve: pin the spawn positions and the tether
/// joints' limits to the reference (inject mode), then record them. The cache
/// injection does not reach these: `tether::spawn_axis` lays the tether along an
/// axis computed from the native start states, as `f32` translations.
fn tap_or_pin_spawn(
    cache: Res<OrbitalCache>,
    mut q: Query<(&mut Position, &mut Transform)>,
    mut joints: Query<&mut DistanceJoint>,
    mut probe: ResMut<Probe>,
) {
    if probe.spawn.is_some() {
        return;
    }
    let next: HashMap<Entity, Entity> = joints.iter().map(|j| (j.body1, j.body2)).collect();
    let order = all_bodies(&cache, &next);
    // The joints in chain order: by the place of their first body.
    let mut chain: Vec<(usize, Mut<DistanceJoint>)> = joints
        .iter_mut()
        .filter_map(|j| order.iter().position(|e| *e == j.body1).map(|i| (i, j)))
        .collect();
    chain.sort_by_key(|(i, _)| *i);

    if let Some(pin) = probe.spawn_pin.clone()
        && pin.positions.len() == order.len()
        && pin.joints.len() == chain.len()
    {
        for (entity, bits) in order.iter().zip(&pin.positions) {
            if let Ok((mut position, mut transform)) = q.get_mut(*entity) {
                position.0 = DVec3::from_array(bits.map(f64::from_bits));
                transform.translation = position.0.as_vec3();
                probe.pinned += 1;
            }
        }
        for ((_, joint), bits) in chain.iter_mut().zip(&pin.joints) {
            joint.limits.min = f64::from_bits(bits[0]);
            joint.limits.max = f64::from_bits(bits[1]);
            probe.pinned += 1;
        }
    }

    probe.order = order.clone();
    probe.spawn = Some(SpawnRecord {
        positions: order
            .iter()
            .map(|e| q.get(*e).map_or([NONE_BITS; 3], |(p, _)| v3(p.0)))
            .collect(),
        joints: chain
            .iter()
            .map(|(_, j)| [j.limits.min.to_bits(), j.limits.max.to_bits()])
            .collect(),
    });
}

fn spawn_tsv(record: &SpawnRecord) -> String {
    let mut s = String::from("kind\tindex\tbits\n");
    for (i, row) in record.positions.iter().enumerate() {
        let _ = writeln!(s, "position\t{i}\t{}", hex(row));
    }
    for (i, row) in record.joints.iter().enumerate() {
        let _ = writeln!(s, "joint\t{i}\t{}", hex(row));
    }
    s
}

fn read_spawn_tsv(path: &Path) -> SpawnRecord {
    let text = std::fs::read_to_string(path).expect("read the spawn reference");
    let mut record = SpawnRecord::default();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        let bits: Vec<u64> = cols[2]
            .split(',')
            .map(|b| u64::from_str_radix(b, 16).expect("hex bits"))
            .collect();
        match cols[0] {
            "position" => record.positions.push([bits[0], bits[1], bits[2]]),
            "joint" => record.joints.push([bits[0], bits[1]]),
            other => panic!("unknown spawn row kind {other}"),
        }
    }
    record
}

/// After `SimStep::CacheEci`, before `SimStep::Bubble`: overwrite the cached
/// states with the reference (inject mode), then record what the step will use.
fn tap_or_inject_cache(
    mut cache: ResMut<OrbitalCache>,
    time: Res<WorldTime>,
    mut probe: ResMut<Probe>,
) {
    let step = step_of(&time);
    let order = bodies(&cache);
    let reference = probe.inject.as_ref().and_then(|m| m.get(&step)).cloned();
    if let Some(reference) = reference
        && reference.len() == order.len()
    {
        for (entity, bits) in order.iter().zip(&reference) {
            if bits[0] == NONE_BITS {
                continue;
            }
            if let Some(state) = cache.eci_states.get_mut(entity) {
                *state = Vector6::from_iterator(bits.iter().map(|b| f64::from_bits(*b)));
                probe.injected += 1;
            }
        }
    }
    let rows = order
        .iter()
        .map(|e| cache.eci_states.get(e).map_or([NONE_BITS; 6], v6))
        .collect();
    probe.cache.insert(step, rows);
}

/// The cache record in a compact, lossless form: per step, a presence mask, then
/// for every present body and component the third-order difference of the raw
/// `f64` bit pattern (wrapping `u64` arithmetic, zigzag, LEB128). Orbit states are
/// smooth, so most values take one to three bytes instead of eight. Header:
/// `P11C1\n`, first step (u64 LE), step count (u64 LE), bodies (u8).
fn encode_cache(record: &CacheRecord) -> Vec<u8> {
    let mut bytes = b"P11C1\n".to_vec();
    let first = record.keys().next().copied().unwrap_or(0);
    let bodies = record.values().next().map_or(0, Vec::len);
    assert!(bodies <= 8, "the presence mask holds eight bodies");
    bytes.extend(first.to_le_bytes());
    bytes.extend((record.len() as u64).to_le_bytes());
    bytes.push(bodies as u8);
    // The last three values per body and component.
    let mut history = vec![[[0u64; 3]; 6]; bodies];
    for (i, (step, rows)) in record.iter().enumerate() {
        assert_eq!(*step, first + i as u64, "the record has no gaps");
        assert_eq!(rows.len(), bodies, "the body count is constant");
        let mut mask = 0u8;
        for (b, row) in rows.iter().enumerate() {
            if row[0] != NONE_BITS {
                mask |= 1 << b;
            }
        }
        bytes.push(mask);
        for (b, row) in rows.iter().enumerate() {
            if mask & (1 << b) == 0 {
                continue;
            }
            for (c, value) in row.iter().enumerate() {
                let [x1, x2, x3] = history[b][c];
                // Third difference: x - 3*x1 + 3*x2 - x3.
                let predicted = x1
                    .wrapping_mul(3)
                    .wrapping_sub(x2.wrapping_mul(3))
                    .wrapping_add(x3);
                let delta = value.wrapping_sub(predicted) as i64;
                let mut zigzag = ((delta << 1) ^ (delta >> 63)) as u64;
                loop {
                    let byte = (zigzag & 0x7f) as u8;
                    zigzag >>= 7;
                    if zigzag == 0 {
                        bytes.push(byte);
                        break;
                    }
                    bytes.push(byte | 0x80);
                }
                history[b][c] = [*value, x1, x2];
            }
        }
    }
    bytes
}

fn decode_cache(bytes: &[u8]) -> CacheRecord {
    assert!(bytes.starts_with(b"P11C1\n"), "not a P11 cache record");
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    let (first, count, bodies) = (u64_at(6), u64_at(14), bytes[22] as usize);
    let mut at = 23;
    let mut history = vec![[[0u64; 3]; 6]; bodies];
    let mut record = CacheRecord::new();
    for i in 0..count {
        let mask = bytes[at];
        at += 1;
        let mut rows = vec![[NONE_BITS; 6]; bodies];
        for (b, row) in rows.iter_mut().enumerate() {
            if mask & (1 << b) == 0 {
                continue;
            }
            for (c, slot) in row.iter_mut().enumerate() {
                let (mut zigzag, mut shift) = (0u64, 0);
                loop {
                    let byte = bytes[at];
                    at += 1;
                    zigzag |= u64::from(byte & 0x7f) << shift;
                    shift += 7;
                    if byte & 0x80 == 0 {
                        break;
                    }
                }
                let delta = ((zigzag >> 1) as i64) ^ -((zigzag & 1) as i64);
                let [x1, x2, x3] = history[b][c];
                let predicted = x1
                    .wrapping_mul(3)
                    .wrapping_sub(x2.wrapping_mul(3))
                    .wrapping_add(x3);
                *slot = predicted.wrapping_add(delta as u64);
                history[b][c] = [*slot, x1, x2];
            }
        }
        record.insert(first + i, rows);
    }
    assert_eq!(at, bytes.len(), "the cache record has trailing bytes");
    record
}

fn write_cache_bin(path: &Path, record: &CacheRecord) {
    let bytes = encode_cache(record);
    assert!(
        decode_cache(&bytes) == *record,
        "the cache record round-trips"
    );
    std::fs::write(path, bytes).expect("write the cache record");
}

fn read_cache_bin(path: &Path) -> CacheRecord {
    decode_cache(&std::fs::read(path).expect("read the cache reference"))
}

// ------------------------------------------------------------------ the run

struct RunOutcome {
    end_hash: u64,
    state_hash: u64,
}

fn run(
    mode: &str,
    inject: Option<CacheRecord>,
    spawn_pin: Option<SpawnRecord>,
    ticks: u64,
    out: &Path,
) -> RunOutcome {
    let mut app = test_app();
    app.insert_resource(Probe {
        inject,
        spawn_pin,
        ..Default::default()
    });
    app.add_systems(
        ManualPhysics,
        (
            tap_or_pin_spawn
                .after(PhysicsSystems::Prepare)
                .before(PhysicsSystems::StepSimulation),
            tap_post_solve_system
                .in_set(PhysicsSystems::Last)
                .before(SimStep::CenterOfMass),
            tap_or_inject_cache
                .after(SimStep::CacheEci)
                .before(SimStep::Bubble),
        ),
    );
    let handles = q13::build(&mut app);

    let anchors: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<SimAnchor>>()
        .iter(app.world())
        .collect();
    assert_eq!(anchors, [handles.root], "the chaser's root is the anchor");

    let mut transition_cursor = app
        .world()
        .resource::<Messages<CaptureTransition>>()
        .get_cursor_current();
    let mut reset_cursor = app
        .world()
        .resource::<Messages<OriginReset>>()
        .get_cursor_current();

    let mut steps = String::from(
        "step\tn_bodies\tn_disabled\th_post_solve\th_cache\th_lvlh\th_dgrav\th_force\th_end\trange_bits\tstate\n",
    );
    let mut raw = String::from("step\tquantity\tbody\tbits\n");
    let mut events = String::from("step\tkind\tdetail\n");
    let mut checkpoints = Vec::new();
    let mut transitions: Vec<(u64, String, String)> = Vec::new();
    let mut resets = 0u64;
    let mut last_disabled = None;
    let mut end_hash = 0;

    for t in 1..=ticks {
        tick(&mut app);
        let sample = q13::sample(&app, &handles);
        let step = sample.step;
        let world = app.world();
        let cache = world.resource::<OrbitalCache>();
        let probe = world.resource::<Probe>();
        let order = probe.order.clone();
        // Raw bits: the first steps, every 640 ticks for 6,400 ticks, then every 6,400.
        let dump = step <= 12 || (t % q13::CHECKPOINT_EVERY == 0 && (t <= 6_400 || t % 6_400 == 0));

        let disabled: Vec<bool> = order
            .iter()
            .map(|e| world.get::<RigidBodyDisabled>(*e).is_some())
            .collect();
        let n_disabled = disabled.iter().filter(|d| **d).count();
        if last_disabled != Some(n_disabled) {
            let _ = writeln!(events, "{step}\tdisabled_bodies\t{n_disabled}");
            last_disabled = Some(n_disabled);
        }

        // Avian's result, before the sim's step systems touched it.
        let post_solve = probe.post_solve.get(&step).cloned().unwrap_or_default();
        // The cached ECI states this step used.
        let cached = probe.cache.get(&step).cloned().unwrap_or_default();

        // The capture target's LVLH frame, as `capture_state_machine_update` builds it.
        let lvlh: Vec<u64> = {
            let p = world.get::<Position>(handles.debris).expect("Position").0;
            let v = world
                .get::<LinearVelocity>(handles.debris)
                .expect("LinearVelocity")
                .0;
            let debris_disabled = world.get::<RigidBodyDisabled>(handles.debris).is_some();
            body_eci_state(cache, handles.debris, p, v, debris_disabled)
                .and_then(|state| LvlhFrame::from_eci(&state))
                .map_or(vec![NONE_BITS; 18], |f| {
                    [f.origin_r, f.origin_v, f.radial, f.along, f.cross, f.omega]
                        .into_iter()
                        .flat_map(v3)
                        .collect()
                })
        };

        // The differential gravity `apply_relative_gravity` applied this step.
        let r_o = cache
            .eci_states
            .get(&handles.root)
            .map(|s| DVec3::new(s[0], s[1], s[2]));
        let dgrav: Vec<[u64; 3]> = order
            .iter()
            .zip(&disabled)
            .map(|(e, is_disabled)| {
                let dynamic = world.get::<RigidBody>(*e).is_some_and(|rb| rb.is_dynamic());
                match (r_o, world.get::<Position>(*e)) {
                    (Some(r_o), Some(p)) if dynamic && !is_disabled => {
                        v3(differential_gravity(r_o, p.0))
                    }
                    _ => [NONE_BITS; 3],
                }
            })
            .collect();

        // The capture controller's applied forces.
        let force: Vec<[u64; 3]> = order
            .iter()
            .map(|e| {
                world
                    .get::<CaptureForce>(*e)
                    .map_or([NONE_BITS; 3], |f| v3(f.applied))
            })
            .collect();

        // The step's end state.
        let end: Vec<[u64; 13]> = order.iter().map(|e| kinematics(world, *e)).collect();

        let flat = |rows: &[[u64; 13]]| rows.iter().flatten().copied().collect::<Vec<u64>>();
        let h_post_solve = Fnv::of(&flat(&post_solve));
        let h_cache = Fnv::of(&cached.iter().flatten().copied().collect::<Vec<u64>>());
        let h_lvlh = Fnv::of(&lvlh);
        let h_dgrav = Fnv::of(&dgrav.iter().flatten().copied().collect::<Vec<u64>>());
        let h_force = Fnv::of(&force.iter().flatten().copied().collect::<Vec<u64>>());
        end_hash = Fnv::of(&flat(&end));
        let _ = writeln!(
            steps,
            "{step}\t{}\t{n_disabled}\t{h_post_solve:016x}\t{h_cache:016x}\t{h_lvlh:016x}\t{h_dgrav:016x}\t{h_force:016x}\t{end_hash:016x}\t{:016x}\t{}",
            order.len(),
            sample.range_m.to_bits(),
            sample.state,
        );

        if dump {
            for (i, row) in post_solve.iter().enumerate() {
                let _ = writeln!(raw, "{step}\tpost_solve\t{i}\t{}", hex(row));
            }
            for (i, row) in cached.iter().enumerate() {
                let _ = writeln!(raw, "{step}\tcache\t{i}\t{}", hex(row));
            }
            let _ = writeln!(raw, "{step}\tlvlh\t0\t{}", hex(&lvlh));
            // Rows without a value (a disabled body, a body without a capture
            // force) are left out.
            for (i, row) in dgrav.iter().enumerate() {
                if *row != [NONE_BITS; 3] {
                    let _ = writeln!(raw, "{step}\tdgrav\t{i}\t{}", hex(row));
                }
            }
            for (i, row) in force.iter().enumerate() {
                if *row != [NONE_BITS; 3] {
                    let _ = writeln!(raw, "{step}\tforce\t{i}\t{}", hex(row));
                }
            }
            for (i, row) in end.iter().enumerate() {
                let _ = writeln!(raw, "{step}\tend\t{i}\t{}", hex(row));
            }
        }

        let start_epoch = world.resource::<WorldTime>().start_epoch;
        for tr in transition_cursor.read(world.resource::<Messages<CaptureTransition>>()) {
            let at = ((tr.epoch - start_epoch) / PHYS_DT).round() as u64;
            let _ = writeln!(events, "{at}\ttransition\t{}->{}", tr.from, tr.to);
            transitions.push((at, tr.from.clone(), tr.to.clone()));
        }
        for reset in reset_cursor.read(world.resource::<Messages<OriginReset>>()) {
            resets += 1;
            let _ = writeln!(
                events,
                "{}\torigin_reset\t{}",
                reset.step,
                hex(&[v3(reset.com_r), v3(reset.com_v)].concat())
            );
        }

        if t % q13::CHECKPOINT_EVERY == 0 {
            checkpoints.push(serde_json::json!({
                "bits": format!("{:016x}", sample.range_m.to_bits()),
                "range_m": sample.range_m,
                "state": sample.state,
                "step": step,
            }));
        }
    }

    let (n_bodies, injected, pinned) = {
        let world = app.world();
        let probe = world.resource::<Probe>();
        write_cache_bin(&out.join(format!("cache-{mode}.bin")), &probe.cache);
        let spawn = probe.spawn.clone().unwrap_or_default();
        std::fs::write(out.join(format!("spawn-{mode}.tsv")), spawn_tsv(&spawn))
            .expect("write spawn");
        (probe.order.len(), probe.injected, probe.pinned)
    };
    std::fs::write(out.join(format!("steps-{mode}.tsv")), steps).expect("write steps");
    std::fs::write(out.join(format!("raw-{mode}.tsv")), raw).expect("write raw");
    std::fs::write(out.join(format!("events-{mode}.tsv")), events).expect("write events");

    let state_hash = test_support::state_hash(&mut app);
    println!(
        "LEVERS {}",
        serde_json::json!({
            "checkpoints": checkpoints,
            "first_transition": transitions.first().map(|t| t.0),
            "platform": platform(),
        })
    );
    println!(
        "P11PROBE {}",
        serde_json::json!({
            "config": config_label(),
            "mode": mode,
            "platform": platform(),
            "ticks": ticks,
            "n_bodies": n_bodies,
            "injected_states": injected,
            "pinned_spawn_values": pinned,
            "origin_resets": resets,
            "transitions": transitions,
            "end_hash": format!("{end_hash:016x}"),
            "state_hash": format!("{state_hash:016x}"),
        })
    );
    RunOutcome {
        end_hash,
        state_hash,
    }
}

// ------------------------------------------------------- start-state chain

fn start_states(out: &Path) {
    let epoch = fixed_epoch();
    let mut table = String::from("key\tbits\n");
    let mut put = |key: &str, bits: &[u64]| {
        let _ = writeln!(table, "{key}\t{}", hex(bits));
    };

    // The sim's EOP provider (`OrbitPlugin`, `use_zero_eop`): zero EOP.
    set_global_eop_provider(StaticEOPProvider::from_zero());

    for (label, id) in [("hst", q13::HST_NORAD_ID), ("chaser", q13::CHASER_NORAD_ID)] {
        let (_, gp, _) = gp_record_to_elements(&q13::fixture_record(id), epoch)
            .expect("the fixture record is complete");
        put(
            &format!("{label}.gp"),
            &[
                gp.mean_motion_rev_per_day,
                gp.eccentricity,
                gp.inclination_deg,
                gp.raan_deg,
                gp.arg_perigee_deg,
                gp.mean_anomaly_deg,
                gp.bstar.unwrap_or(f64::NAN),
                gp.mean_motion_dot.unwrap_or(f64::NAN),
                gp.mean_motion_ddot.unwrap_or(f64::NAN),
            ]
            .map(f64::to_bits),
        );
        put(
            &format!("{label}.gp_epoch_minus_start_s"),
            &[(gp.epoch - epoch).to_bits()],
        );

        // The propagator `ssg_sim::orbit::sgp4_state_eci` builds (same arguments).
        let sgp = SGPPropagator::from_omm_elements(
            &gp.epoch_str,
            gp.mean_motion_rev_per_day,
            gp.eccentricity,
            gp.inclination_deg,
            gp.raan_deg,
            gp.arg_perigee_deg,
            gp.mean_anomaly_deg,
            gp.norad_id as u64,
            1.0,
            None,
            None,
            gp.classification,
            gp.bstar,
            gp.mean_motion_dot,
            gp.mean_motion_ddot,
            gp.ephemeris_type,
            None,
            None,
        )
        .expect("SGP4 propagator");
        let teme = SStateProvider::state(&sgp, epoch).expect("TEME");
        let pef = sgp.state_pef(epoch).expect("PEF");
        let itrf = SOrbitStateProvider::state_itrf(&sgp, epoch).expect("ITRF");
        let gcrf = SOrbitStateProvider::state_gcrf(&sgp, epoch).expect("GCRF");
        let eci = DOrbitStateProvider::state_eci(&sgp, epoch).expect("ECI");
        put(&format!("{label}.sgp4_teme"), &v6(&teme));
        put(&format!("{label}.brahe_pef"), &v6(&pef));
        put(&format!("{label}.brahe_itrf"), &v6(&itrf));
        put(&format!("{label}.brahe_gcrf"), &v6(&gcrf));
        // `eci` is what `ssg_sim::orbit::sgp4_state_eci` returns: the sim's start state.
        put(&format!("{label}.brahe_state_eci"), &v6(&eci));
        let sim = eci;

        // What `init_orbitals` does with it, and the first steps of the Kepler orbit.
        let koe = brahe::state_eci_to_koe(sim, AngleFormat::Radians);
        put(&format!("{label}.koe_from_eci"), &v6(&koe));
        let kepler = KeplerianPropagator::from_eci(epoch, sim, 1.0);
        for k in [0u64, 1, 2, 3, 64, 640, 6_400, 196_610] {
            let state = DOrbitStateProvider::state_eci(&kepler, epoch + k as f64 * PHYS_DT)
                .expect("Kepler state");
            put(&format!("{label}.kepler_state_eci.step{k}"), &v6(&state));
        }
    }

    // Library functions on fixed inputs: the same input bits on every platform, so
    // a difference here belongs to the function alone.
    let fixed_itrf = Vector6::new(
        -1_234_567.25,
        6_543_210.5,
        2_345_678.75,
        -5_432.125,
        -1_234.5,
        4_321.062_5,
    );
    for dt in [0.0, 100.0, 3_072.0, 86_400.0] {
        let at: Epoch = epoch + dt;
        let m = |m: nalgebra::SMatrix<f64, 3, 3>| m.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        put(
            &format!("fixed.sofa.bias_precession_nutation.t{dt}"),
            &m(brahe::bias_precession_nutation(at)),
        );
        put(
            &format!("fixed.sofa.earth_rotation.t{dt}"),
            &m(brahe::earth_rotation(at)),
        );
        put(
            &format!("fixed.sofa.polar_motion.t{dt}"),
            &m(brahe::polar_motion(at)),
        );
        put(
            &format!("fixed.sofa.rotation_itrf_to_gcrf.t{dt}"),
            &m(brahe::rotation_itrf_to_gcrf(at)),
        );
        put(
            &format!("fixed.brahe.state_itrf_to_gcrf.t{dt}"),
            &v6(&brahe::state_itrf_to_gcrf(at, fixed_itrf)),
        );
    }

    let fixed_koe = Vector6::new(6_921_000.5, 0.000_287_5, 0.496_75, 1.25, 2.5, 0.75);
    let fixed_eci = Vector6::new(
        -2_345_678.5,
        5_432_109.25,
        3_456_789.125,
        -5_678.25,
        -3_456.5,
        2_345.75,
    );
    put(
        "fixed.brahe.state_koe_to_eci",
        &v6(&brahe::state_koe_to_eci(fixed_koe, AngleFormat::Radians)),
    );
    put(
        "fixed.brahe.state_eci_to_koe",
        &v6(&brahe::state_eci_to_koe(fixed_eci, AngleFormat::Radians)),
    );
    let fixed_kepler =
        KeplerianPropagator::from_keplerian(epoch, fixed_koe, AngleFormat::Radians, 1.0);
    for k in [0u64, 1, 2, 3, 64, 640, 6_400, 196_610] {
        let state = DOrbitStateProvider::state_eci(&fixed_kepler, epoch + k as f64 * PHYS_DT)
            .expect("Kepler state");
        put(
            &format!("fixed.brahe.kepler_state_eci.step{k}"),
            &v6(&state),
        );
    }
    let from_eci = KeplerianPropagator::from_eci(epoch, fixed_eci, 1.0);
    for k in [0u64, 1, 6_400] {
        let state = DOrbitStateProvider::state_eci(&from_eci, epoch + k as f64 * PHYS_DT)
            .expect("Kepler state");
        put(
            &format!("fixed.brahe.from_eci_then_state_eci.step{k}"),
            &v6(&state),
        );
    }
    let mut lcg = Lcg(0x5EED_0011);
    let anomalies: Vec<u64> = (0..64)
        .map(|_| {
            let m = lcg.unit() * std::f64::consts::TAU;
            brahe::anomaly_mean_to_eccentric(m, 0.000_287_5, AngleFormat::Radians)
                .expect("Kepler's equation converges")
                .to_bits()
        })
        .collect();
    put("fixed.brahe.anomaly_mean_to_eccentric.x64", &anomalies);

    std::fs::write(out.join("start.tsv"), table).expect("write start.tsv");
}

// ------------------------------------------------------------ function table

/// Integer LCG (Knuth's MMIX constants): the inputs are built with exact integer
/// steps and correctly rounded IEEE operations, so they are the same everywhere.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
    /// In [0, 1): 53 random bits over 2^53, exact.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// In [-1, 1).
    fn signed(&mut self) -> f64 {
        self.unit() * 2.0 - 1.0
    }
}

const FN_INPUTS: usize = 256;

fn fn_table(out: &Path) {
    use std::f64::consts::PI;
    let mut table = String::from("function\tx_bits\ty_bits\tstd_bits\tlibm_bits\n");
    let mut lcg = Lcg(0x0173_0178);

    type Unary = (
        &'static str,
        fn(&mut Lcg) -> f64,
        fn(f64) -> f64,
        fn(f64) -> f64,
    );
    let angle: fn(&mut Lcg) -> f64 = |l| l.signed() * 8.0 * PI;
    let big_angle: fn(&mut Lcg) -> f64 = |l| l.unit() * 1.0e6;
    let unit: fn(&mut Lcg) -> f64 = |l| l.signed();
    let small: fn(&mut Lcg) -> f64 = |l| l.signed() * 20.0;
    let positive: fn(&mut Lcg) -> f64 = |l| l.unit() * 1.0e7 + 1.0e-3;
    let unary: [Unary; 22] = [
        ("sin", angle, f64::sin, libm::sin),
        ("cos", angle, f64::cos, libm::cos),
        ("tan", angle, f64::tan, libm::tan),
        ("sin.large", big_angle, f64::sin, libm::sin),
        ("cos.large", big_angle, f64::cos, libm::cos),
        ("asin", unit, f64::asin, libm::asin),
        ("acos", unit, f64::acos, libm::acos),
        ("atan", small, f64::atan, libm::atan),
        ("sinh", small, f64::sinh, libm::sinh),
        ("cosh", small, f64::cosh, libm::cosh),
        ("tanh", small, f64::tanh, libm::tanh),
        ("exp", small, f64::exp, libm::exp),
        ("exp2", small, f64::exp2, libm::exp2),
        ("exp_m1", small, f64::exp_m1, libm::expm1),
        ("ln", positive, f64::ln, libm::log),
        ("ln_1p", positive, f64::ln_1p, libm::log1p),
        ("log2", positive, f64::log2, libm::log2),
        ("log10", positive, f64::log10, libm::log10),
        ("sqrt", positive, f64::sqrt, libm::sqrt),
        ("cbrt", positive, f64::cbrt, libm::cbrt),
        ("powi3", positive, |x| x.powi(3), |x| x * x * x),
        (
            "powf_third",
            positive,
            |x| x.powf(1.0 / 3.0),
            |x| libm::pow(x, 1.0 / 3.0),
        ),
    ];
    for (name, input, std_fn, libm_fn) in unary {
        for _ in 0..FN_INPUTS {
            let x = input(&mut lcg);
            let _ = writeln!(
                table,
                "{name}\t{:016x}\t-\t{:016x}\t{:016x}",
                x.to_bits(),
                std_fn(x).to_bits(),
                libm_fn(x).to_bits()
            );
        }
    }

    type Binary = (
        &'static str,
        fn(&mut Lcg) -> f64,
        fn(&mut Lcg) -> f64,
        fn(f64, f64) -> f64,
        fn(f64, f64) -> f64,
    );
    let exponent: fn(&mut Lcg) -> f64 = |l| l.signed() * 4.0;
    let binary: [Binary; 5] = [
        ("atan2", small, small, f64::atan2, libm::atan2),
        ("powf", positive, exponent, f64::powf, libm::pow),
        ("hypot", small, small, f64::hypot, libm::hypot),
        ("rem", big_angle, |_| 2.0 * PI, |x, y| x % y, libm::fmod),
        (
            "mul_add.x_y_y",
            small,
            small,
            |x, y| x.mul_add(y, y),
            |x, y| libm::fma(x, y, y),
        ),
    ];
    for (name, input_x, input_y, std_fn, libm_fn) in binary {
        for _ in 0..FN_INPUTS {
            let (x, y) = (input_x(&mut lcg), input_y(&mut lcg));
            let _ = writeln!(
                table,
                "{name}\t{:016x}\t{:016x}\t{:016x}\t{:016x}",
                x.to_bits(),
                y.to_bits(),
                std_fn(x, y).to_bits(),
                libm_fn(x, y).to_bits()
            );
        }
    }
    for _ in 0..FN_INPUTS {
        let x = angle(&mut lcg);
        let (s, c) = x.sin_cos();
        let (ls, lc) = libm::sincos(x);
        let _ = writeln!(
            table,
            "sin_cos.sin\t{:016x}\t-\t{:016x}\t{:016x}",
            x.to_bits(),
            s.to_bits(),
            ls.to_bits()
        );
        let _ = writeln!(
            table,
            "sin_cos.cos\t{:016x}\t-\t{:016x}\t{:016x}",
            x.to_bits(),
            c.to_bits(),
            lc.to_bits()
        );
    }
    std::fs::write(out.join("fn_table.tsv"), table).expect("write fn_table.tsv");
}

// ----------------------------------------------------------------- the test

#[test]
fn p11_probe() {
    let out = out_dir();
    let ticks = std::env::var("P11_PROBE_TICKS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(DEFAULT_TICKS);
    println!(
        "P11PROBE {}",
        serde_json::json!({
            "config": config_label(),
            "platform": platform(),
            "out": out.display().to_string(),
            "debug_assertions": cfg!(debug_assertions),
        })
    );

    start_states(&out);
    fn_table(&out);

    let modes = std::env::var("P11_PROBE_MODES").unwrap_or_else(|_| "native,inject".to_string());
    let wants = |mode: &str| modes.split(',').any(|m| m.trim() == mode);
    let native = wants("native").then(|| run("native", None, None, ticks, &out));

    let reference = std::env::var_os("P11_PROBE_REFERENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/p11_cache_reference.bin")
        });
    if !wants("inject") {
        println!(
            "P11PROBE {}",
            serde_json::json!({ "inject": "not requested" })
        );
    } else if reference.exists() {
        let spawn_reference = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/p11_spawn_reference.tsv");
        let spawn_pin = spawn_reference
            .exists()
            .then(|| read_spawn_tsv(&spawn_reference));
        let injected = run(
            "inject",
            Some(read_cache_bin(&reference)),
            spawn_pin,
            ticks,
            &out,
        );
        if let Some(native) = native {
            println!(
                "P11PROBE {}",
                serde_json::json!({
                    "config": config_label(),
                    "platform": platform(),
                    "inject_equals_native": injected.end_hash == native.end_hash
                        && injected.state_hash == native.state_hash,
                })
            );
        }
    } else {
        println!(
            "P11PROBE {}",
            serde_json::json!({ "inject": "skipped", "missing": reference.display().to_string() })
        );
    }
}
