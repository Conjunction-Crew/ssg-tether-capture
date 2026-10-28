use bevy::math::DVec3;

use super::state_hash;
use crate::scenario::Sample;

/// The documented byte layout, checked against an independent FNV-1a 64 of the
/// same 104 bytes (Python `struct.pack('<Q', 643)` + 12 × `'<d'`).
#[test]
fn state_hash_follows_documented_layout() {
    assert_eq!(
        state_hash(&[]),
        0xcbf2_9ce4_8422_2325,
        "empty: the offset basis"
    );
    let sample = Sample {
        step: 643,
        epoch: String::new(),
        state: String::new(),
        range_m: 0.0,
        rel_speed_mps: 0.0,
        root_position: DVec3::new(1.0, -2.5, 3.25),
        root_velocity: DVec3::new(0.5, 0.0, -0.0),
        debris_position: DVec3::new(1e6, 2e-3, -7.0),
        debris_velocity: DVec3::new(0.125, 6.5, -1e-9),
    };
    assert_eq!(state_hash(&[sample]), 0xc1d8_7d67_f58d_7639);
}
