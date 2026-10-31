//! `StepCap` (P7 S4, R9, F-46): the cap changes how many fixed steps run per
//! update, never which steps. A Q13 run whose cap changes every update equals the
//! run at a fixed cap, compared step by step.

use std::hash::{Hash, Hasher};

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::SimStep;
use crate::test_support::{current_step, q13, test_app, tick};
use crate::time::{MAX_STEPS_PER_UPDATE, ManualPhysics, StepCap, WorldTime};

/// `(step, hash of every rigid body's state bits)` after every fixed step.
#[derive(Resource, Default)]
struct StepHashes(Vec<(u64, u64)>);

fn hash_every_step(
    world_time: Res<WorldTime>,
    bodies: Query<
        (
            Entity,
            &Position,
            &Rotation,
            &LinearVelocity,
            &AngularVelocity,
        ),
        With<RigidBody>,
    >,
    mut out: ResMut<StepHashes>,
) {
    let mut states: Vec<_> = bodies.iter().collect();
    states.sort_by_key(|(entity, ..)| *entity);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (_, p, r, v, w) in states {
        for x in [
            p.x, p.y, p.z, r.x, r.y, r.z, r.w, v.x, v.y, v.z, w.x, w.y, w.z,
        ] {
            x.to_bits().hash(&mut hasher);
        }
    }
    let step = ((world_time.epoch - world_time.start_epoch) / crate::time::PHYS_DT).round() as u64;
    out.0.push((step, hasher.finish()));
}

fn hashing_app() -> App {
    let mut app = test_app();
    app.init_resource::<StepHashes>()
        .add_systems(ManualPhysics, hash_every_step.after(SimStep::Record));
    app
}

#[test]
fn the_step_cap_defaults_to_64_steps_per_update() {
    let app = test_app();
    assert_eq!(
        *app.world().resource::<StepCap>(),
        StepCap(MAX_STEPS_PER_UPDATE)
    );
    assert_eq!(StepCap::default().0, 64);
}

#[test]
fn a_cap_that_changes_every_update_runs_the_same_steps_as_a_fixed_cap() {
    const LAST_STEP: u64 = 320;
    // Warp 6 with the cap changing every update: 1, 6, 3, 6, 2, 5, 4, … steps.
    const CAPS: [u32; 7] = [1, 6, 3, 64, 2, 5, 4];
    let mut varying = hashing_app();
    q13::build(&mut varying);
    varying.world_mut().resource_mut::<WorldTime>().multiplier = 6;
    let mut update = 0;
    while current_step(&varying) < LAST_STEP {
        let cap = CAPS[update % CAPS.len()];
        varying.insert_resource(StepCap(cap));
        let before = current_step(&varying);
        tick(&mut varying);
        let ran = current_step(&varying) - before;
        assert_eq!(ran, u64::from(cap.min(6)), "update {update} with cap {cap}");
        assert_eq!(
            varying.world().resource::<WorldTime>().effective_multiplier,
            cap.min(6)
        );
        update += 1;
    }

    let mut fixed = hashing_app();
    q13::build(&mut fixed);
    while current_step(&fixed) < current_step(&varying) {
        tick(&mut fixed);
    }

    let (a, b) = (
        &fixed.world().resource::<StepHashes>().0,
        &varying.world().resource::<StepHashes>().0,
    );
    assert_eq!(a.len(), b.len(), "the same steps ran");
    assert!(a.len() as u64 >= LAST_STEP, "every step from 1 is hashed");
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert_eq!(x, y, "step entry {i}: fixed {x:?} vs varying {y:?}");
    }
    println!(
        "StepCap equivalence: {} steps compared ({} … {}), {update} updates with caps {CAPS:?} at warp 6; \
         first hash {:016x}, last hash {:016x}",
        a.len(),
        a[0].0,
        a[a.len() - 1].0,
        a[0].1,
        a[a.len() - 1].1
    );
}
