//! Tests of `orbit::kepler` (178-P11 S1): known-answer bits of the two-body step
//! and both conversions (T1b), fidelity against brahe's `KeplerianPropagator`,
//! which serves as the test reference from here on (T2, T5), the exact state at
//! `dt = 0` (T3), the round trip through the elements without rounding (T4, F-69),
//! re-anchoring (T6), and the hyperbolic and error cases (T7).
//!
//! The known-answer constants were made on macOS (aarch64); `full-ci` asserts the
//! same constants on Linux and Windows. A failure on one platform is a finding,
//! never a constant to change.

use std::f64::consts::{FRAC_PI_2, PI};

use brahe::utils::DOrbitStateProvider;
use brahe::{AngleFormat, Epoch, GM_EARTH, KeplerianPropagator};
use nalgebra::{Vector3, Vector6};

use crate::orbit::kepler::{TwoBody, elements_to_state, state_to_elements};
use crate::test_support::fixed_epoch;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn from_bits6(bits: &[u64; 6]) -> Vector6<f64> {
    Vector6::from_fn(|i, _| f64::from_bits(bits[i]))
}

/// Position and velocity distance between two states (m, m/s).
fn dr(a: &Vector6<f64>, b: &Vector6<f64>) -> (f64, f64) {
    let d = a - b;
    (
        Vector3::new(d[0], d[1], d[2]).norm(),
        Vector3::new(d[3], d[4], d[5]).norm(),
    )
}

/// One line per component of `actual` whose bits differ from `expected`.
fn mismatches(what: &str, actual: &Vector6<f64>, expected: &[u64; 6]) -> Vec<String> {
    (0..6)
        .filter(|&i| actual[i].to_bits() != expected[i])
        .map(|i| {
            format!(
                "{what}[{i}]: got {:#018x} ({:e}), expected {:#018x} ({:e})",
                actual[i].to_bits(),
                actual[i],
                expected[i],
                f64::from_bits(expected[i])
            )
        })
        .collect()
}

fn period(a: f64) -> f64 {
    2.0 * PI * (a * a * a / GM_EARTH).sqrt()
}

// ---------------------------------------------------------------------------
// T1b: known-answer bits
// ---------------------------------------------------------------------------

/// The step offsets of T1b (s).
const DTS: [f64; 6] = [1.0 / 64.0, 1.0, 60.0, 5_400.0, 86_400.0, -600.0];

struct KnownOrbit {
    name: &'static str,
    /// The start state at `fixed_epoch()`.
    start: [u64; 6],
    /// `state_eci(fixed_epoch() + DTS[k])`.
    steps: [[u64; 6]; 6],
    /// `state_to_elements(start)`.
    elements: [u64; 6],
    /// `elements_to_state(state_to_elements(start))`.
    back: [u64; 6],
}

/// The reference scenario's HST start state: `hst.brahe_gcrf` of the Run 0 probe
/// on macOS (round A), as bit patterns.
const HST: KnownOrbit = KnownOrbit {
    name: "hst",
    start: [
        0xc151133927e23ad5,
        0xc153c9c3525455d4,
        0x40c6c876f38a826f,
        0x40b3e377a8f9170f,
        0xc0b11a55a9d8ba39,
        0x40ac5771be1b0b0d,
    ],
    steps: [
        [
            0xc1511325445f79c4,
            0xc153c9d46c9d244a,
            0x40c6e4ce6539c9d3,
            0x40b3e38dd9238929,
            0xc0b11a3bf30ffe86,
            0x40ac5771a06d97a4,
        ],
        [
            0xc1510e3f988774e9,
            0xc153ce0919fa0569,
            0x40cdde525dab8201,
            0x40b3e902e8651103,
            0xc0b113e74842018d,
            0x40ac576930b70e83,
        ],
        [
            0xc14fbebd1a119e51,
            0xc154bed1a827f134,
            0x410bf8a36ae4b07d,
            0x40b524b6804fc67e,
            0xc0af1e3fdf3b826f,
            0x40ac4587ea0ce0e9,
        ],
        [
            0xc1554887426c2c4a,
            0xc14d9271ebdefcdc,
            0xc12b8d1e0c1f7395,
            0x40ab443702126663,
            0xc0b6bdc1faf1288a,
            0x40ab3ba4e34012a7,
        ],
        [
            0x41542d38dffc412b,
            0xc1469808af33b2ce,
            0x414876a27e4fc53a,
            0x40af61d772cc59af,
            0x40b931a4c4c6f2da,
            0xc08430ec32173393,
        ],
        [
            0xc15836f14dd7a2b6,
            0xc138fad2be55b928,
            0xc13ea9008627109f,
            0x408c9972c0bb5208,
            0xc0bb641f29f941f1,
            0x40a652542075635d,
        ],
    ],
    elements: [
        0x415a2be0a0db61ce,
        0x3f58692db58b7829,
        0x3fdfb348c26bf815,
        0x400ffa7ad07cf2b9,
        0x3fdedfbfce40ecf0,
        0x40173911dc46fbaf,
    ],
    back: [
        0xc151133927e23ad4,
        0xc153c9c3525455d2,
        0x40c6c876f38a8280,
        0x40b3e377a8f9170f,
        0xc0b11a55a9d8ba39,
        0x40ac5771be1b0b10,
    ],
};

/// A transfer orbit, e = 0.73 (a = 24,400 km).
const TRANSFER: KnownOrbit = KnownOrbit {
    name: "transfer_e0.73",
    start: [
        0xc170fdeaa89ff221,
        0x416a0720b129db80,
        0xc1672e539ca13ddc,
        0x4074f0e2a0d89264,
        0xc0abfc1abf71941d,
        0x4095daa644e66137,
    ],
    steps: [
        [
            0xc170fdea54dc30ef,
            0x416a0719b222d7e1,
            0xc1672e50e14c2aa6,
            0x4074f0fdf812ba86,
            0xc0abfc1d5d85fd8d,
            0x4095daaaee79fbb0,
        ],
        [
            0xc170fdd5b4525342,
            0x416a0560ea41b3f0,
            0xc1672da4c2c56d96,
            0x4074f7b8b4353bb5,
            0xc0abfcc245fd9849,
            0x4095dbd0b1ea9e58,
        ],
        [
            0xc170f8d1d6daf9b9,
            0x41699de59528d431,
            0xc1670517cb58cadb,
            0x40768edf4e4231c5,
            0xc0ac23727e3ac1ce,
            0x4096210abd02c7d7,
        ],
        [
            0x410a90e267567d8e,
            0xc157a2ad647426cb,
            0x41415177f5e2b986,
            0x40c28aacd86acffd,
            0x409ee6298f144154,
            0x40a92ac1cb18fc8e,
        ],
        [
            0x4169105ddf360d94,
            0x417385b7a1955bb2,
            0xc13b2c1066346530,
            0xc07fefbf2a8dfcf6,
            0x40ad01c5254c7f2b,
            0xc097b52a364dc4be,
        ],
        [
            0xc1711d514316cd6b,
            0x416e04337ad06d78,
            0xc168af85f57cd005,
            0x40592918e815b534,
            0xc0aa7bb7badf8ce2,
            0x4093493def335354,
        ],
    ],
    elements: [
        0x4177450800000001,
        0x3fe75c28f5c28f5a,
        0x3fdfcd35a858793d,
        0x4010067381d7dbf5,
        0x3fed02de00d1b718,
        0x401582f837b4a233,
    ],
    back: [
        0xc170fdeaa89ff223,
        0x416a0720b129db86,
        0xc1672e539ca13de0,
        0x4074f0e2a0d89242,
        0xc0abfc1abf719419,
        0x4095daa644e66130,
    ],
};

/// A near-circular orbit, e = 1e-9 (a = 6,918 km).
const NEAR_CIRCULAR: KnownOrbit = KnownOrbit {
    name: "near_circular_e1e-9",
    start: [
        0xc15119ae6734a275,
        0xc15419a1b1442a1a,
        0x40b0efa057c67e00,
        0x40b3dbbd0816373d,
        0xc0b0e20c3adaefb6,
        0x40ac44f47a4c80cd,
    ],
    steps: [
        [
            0xc151199a8b6ccf34,
            0xc15419b29343b507,
            0x40b1282a40b05a78,
            0x40b3dbd29e6909dc,
            0xc0b0e1f2dafb9555,
            0x40ac44f46f8a04b1,
        ],
        [
            0xc15114b6cb5059a5,
            0xc1541dd96946072c,
            0x40bf1219ba4d460d,
            0x40b3e121d74d5949,
            0xc0b0dbb39b4295e4,
            0x40ac44f0b0a436b8,
        ],
        [
            0xc14fcd1363582b9a,
            0xc1550b8c01a723c3,
            0x410b02f0b78a99e7,
            0x40b5144d7e686924,
            0xc0aeb8b0912c20d8,
            0x40ac34a6e36e323d,
        ],
        [
            0xc15635f418d340fd,
            0xc14b1d7f5759ba22,
            0xc13193ba5c5e4036,
            0x40a7ba1c8c5f9b98,
            0xc0b7ba6577573063,
            0x40aa7d1cf4a0b161,
        ],
        [
            0xc13513423671b932,
            0xc158ff23ba7c67f5,
            0x413a7c6175e0d1dd,
            0x40bafd719a101945,
            0xc0840b028a019ccc,
            0x40a809677e476c4b,
        ],
        [
            0xc15856fc95777386,
            0xc13ad336364479fc,
            0xc13ebbf057f26419,
            0x408fa086aab65158,
            0xc0bb2be48d920b54,
            0x40a66252bb261666,
        ],
    ],
    elements: [
        0x415a63dc00000004,
        0x3e112e0c2c05071b,
        0x3fdfcd35a858793e,
        0x4010067381d7dbf4,
        0x3fed02ddc6508b90,
        0x401582f83f04c7a5,
    ],
    back: [
        0xc15119ae6734a27e,
        0xc15419a1b1442a14,
        0x40b0efa057c67100,
        0x40b3dbbd08163734,
        0xc0b0e20c3adaefbf,
        0x40ac44f47a4c80cc,
    ],
};

#[test]
fn t1b_known_answer_bits() {
    let t0 = fixed_epoch();
    let mut found = Vec::new();
    for orbit in [&HST, &TRANSFER, &NEAR_CIRCULAR] {
        let start = from_bits6(&orbit.start);
        let body = TwoBody::from_eci(t0, start);
        for (k, dt) in DTS.iter().enumerate() {
            let state = body.state_eci(t0 + *dt).expect("step");
            found.extend(mismatches(
                &format!("{} state_eci(dt {dt})", orbit.name),
                &state,
                &orbit.steps[k],
            ));
        }
        let elements = state_to_elements(start);
        found.extend(mismatches(
            &format!("{} state_to_elements", orbit.name),
            &elements,
            &orbit.elements,
        ));
        found.extend(mismatches(
            &format!("{} elements_to_state", orbit.name),
            &elements_to_state(from_bits6(&orbit.elements)),
            &orbit.back,
        ));
    }
    assert!(
        found.is_empty(),
        "known-answer mismatches:\n{}",
        found.join("\n")
    );
}

// ---------------------------------------------------------------------------
// T2: against brahe's KeplerianPropagator
// ---------------------------------------------------------------------------

#[test]
fn t2_matches_brahe_to_one_orbit_and_one_day() {
    let t0 = fixed_epoch();
    let mut worst_orbit = (0.0f64, 0.0f64);
    let mut worst_day = 0.0f64;
    for (a, e) in [
        (6_918_000.0, 1e-3),
        (8_000_000.0, 0.1),
        (24_400_000.0, 0.73),
    ] {
        let koe = Vector6::new(a, e, 0.4969, 4.0063, 0.9066, 5.3779);
        // The reference starts from the elements themselves (no conversion).
        let reference = KeplerianPropagator::from_keplerian(t0, koe, AngleFormat::Radians, 1.0);
        let start = reference.state_eci(t0).expect("brahe start");
        let body = TwoBody::from_eci(t0, start);
        let p = period(a);
        let mut dts = vec![1.0 / 64.0, 1.0, 60.0, 600.0];
        dts.extend((1..=32).map(|k| p * f64::from(k) / 32.0));
        for dt in dts {
            let t = t0 + dt;
            let (dp, dv) = dr(
                &body.state_eci(t).expect("step"),
                &reference.state_eci(t).expect("brahe"),
            );
            assert!(
                dp <= 1e-7 && dv <= 1e-10,
                "a {a} e {e} dt {dt}: {dp:e} m, {dv:e} m/s from brahe"
            );
            worst_orbit = (worst_orbit.0.max(dp), worst_orbit.1.max(dv));
        }
        let t = t0 + 86_400.0;
        let (dp, _) = dr(
            &body.state_eci(t).expect("step"),
            &reference.state_eci(t).expect("brahe"),
        );
        assert!(dp <= 1e-5, "a {a} e {e} one day: {dp:e} m from brahe");
        worst_day = worst_day.max(dp);
    }
    println!(
        "T2 largest difference from brahe: to one orbit {:.3e} m, {:.3e} m/s; at one day {worst_day:.3e} m",
        worst_orbit.0, worst_orbit.1
    );
}

// ---------------------------------------------------------------------------
// T3: dt = 0 returns the stored state
// ---------------------------------------------------------------------------

#[test]
fn t3_state_at_initial_epoch_is_the_stored_state() {
    let t0 = fixed_epoch();
    for orbit in [&HST, &TRANSFER, &NEAR_CIRCULAR] {
        let start = from_bits6(&orbit.start);
        let body = TwoBody::from_eci(t0, start);
        assert_eq!(body.initial_epoch(), t0);
        let at = body.state_eci(body.initial_epoch()).expect("dt = 0");
        assert!(
            mismatches(orbit.name, &at, &orbit.start).is_empty(),
            "{}: state at dt = 0 is not the stored state",
            orbit.name
        );
        // `elements_at` is `state_to_elements` of that state.
        let el = body.elements_at(t0).expect("elements");
        assert_eq!(
            el.map(f64::to_bits),
            state_to_elements(start).map(f64::to_bits)
        );
    }
    // `from_elements` stores `elements_to_state` of its elements.
    let koe = Vector6::new(6_918_000.0, 2.132e-4, 0.4969, 4.0063, 0.9066, 5.3779);
    let body = TwoBody::from_elements(t0, koe);
    assert_eq!(
        body.state_eci(t0).expect("dt = 0").map(f64::to_bits),
        elements_to_state(koe).map(f64::to_bits)
    );
}

// ---------------------------------------------------------------------------
// T4: the round trip through the elements does not round (F-69)
// ---------------------------------------------------------------------------

#[test]
fn t4_round_trip_without_rounding() {
    let mut worst = (0.0f64, 0.0f64);
    for inc in [0.4969, 0.0, 1.0e-9, FRAC_PI_2, 3.0] {
        for (a, e) in [
            (6_918_000.0, 0.0),
            (6_918_000.0, 1.0e-12),
            (6_918_000.0, 1.0e-9),
            (6_918_000.0, 1.0e-7),
            (6_918_000.0, 1.0e-5),
            (6_918_000.0, 9.0e-5),
            (6_918_000.0, 2.132e-4),
            (6_918_000.0, 1.0e-3),
            (8_000_000.0, 0.1),
            (24_400_000.0, 0.73),
            (100_000_000.0, 0.95),
        ] {
            for k in 0..48 {
                let m = 0.13 + f64::from(k) * 0.1309;
                let state = elements_to_state(Vector6::new(a, e, inc, 4.0063, 0.9066, m));
                let back = elements_to_state(state_to_elements(state));
                let (dp, dv) = dr(&back, &state);
                assert!(
                    dp <= 1e-6 && dv <= 1e-9,
                    "i {inc} a {a} e {e} M {m}: round trip misses by {dp:e} m, {dv:e} m/s"
                );
                worst = (worst.0.max(dp), worst.1.max(dv));
            }
        }
    }
    println!(
        "T4 largest round-trip miss: {:.3e} m, {:.3e} m/s",
        worst.0, worst.1
    );
}

// ---------------------------------------------------------------------------
// T5: state_to_elements against brahe where brahe does not round
// ---------------------------------------------------------------------------

#[test]
fn t5_elements_match_brahe_where_brahe_does_not_round() {
    let mut worst_a = 0.0f64;
    let mut worst_angle = 0.0f64;
    for inc in [0.4969, FRAC_PI_2, 3.0] {
        for (a, e) in [
            (6_918_000.0, 1.0e-3),
            (8_000_000.0, 0.1),
            (24_400_000.0, 0.73),
            (100_000_000.0, 0.95),
        ] {
            for k in 0..24 {
                let m = 0.2 + f64::from(k) * 0.25;
                let state = elements_to_state(Vector6::new(a, e, inc, 4.0063, 0.9066, m));
                let ours = state_to_elements(state);
                let theirs = brahe::coordinates::state_eci_to_koe(state, AngleFormat::Radians);
                let da = (ours[0] - theirs[0]).abs();
                assert!(da <= 1e-6, "i {inc} e {e} M {m}: a differs by {da:e} m");
                worst_a = worst_a.max(da);
                for j in 1..6 {
                    let d = (ours[j] - theirs[j]).abs();
                    assert!(
                        d <= 1e-11,
                        "i {inc} e {e} M {m}: element {j} differs by {d:e}"
                    );
                    worst_angle = worst_angle.max(d);
                }
            }
        }
    }
    println!(
        "T5 largest difference from brahe's state_eci_to_koe: a {worst_a:.3e} m, others {worst_angle:.3e}"
    );
}

// ---------------------------------------------------------------------------
// T6: re-anchoring (what the origin reset does)
// ---------------------------------------------------------------------------

#[test]
fn t6_re_anchoring_matches_one_long_step() {
    let t0 = fixed_epoch();
    let mut worst = 0.0f64;
    for orbit in [&HST, &TRANSFER, &NEAR_CIRCULAR] {
        let body = TwoBody::from_eci(t0, from_bits6(&orbit.start));
        for (t1, t2) in [(1_200.0, 2_400.0), (3_600.0, 3_600.0), (37.25, 5_000.0)] {
            let mid: Epoch = t0 + t1;
            let anchored = TwoBody::from_eci(mid, body.state_eci(mid).expect("t1"));
            let end = t0 + (t1 + t2);
            let (dp, _) = dr(
                &anchored.state_eci(end).expect("t2"),
                &body.state_eci(end).expect("t1 + t2"),
            );
            assert!(dp <= 1e-6, "{} t1 {t1} t2 {t2}: {dp:e} m", orbit.name);
            worst = worst.max(dp);
        }
    }
    println!("T6 largest re-anchoring difference: {worst:.3e} m");
}

// ---------------------------------------------------------------------------
// T7: hyperbolic orbits and the error path
// ---------------------------------------------------------------------------

#[test]
fn t7_hyperbolic_orbit_keeps_energy_and_angular_momentum() {
    let t0 = fixed_epoch();
    // Periapsis of an e = 1.3 hyperbola, r_p = 7,000 km, inclined 0.5 rad.
    let (e, rp) = (1.3, 7_000_000.0);
    let vp = (GM_EARTH * (1.0 + e) / rp).sqrt();
    let start = Vector6::new(rp, 0.0, 0.0, 0.0, vp * 0.5f64.cos(), vp * 0.5f64.sin());
    let invariants = |s: &Vector6<f64>| {
        let r = Vector3::new(s[0], s[1], s[2]);
        let v = Vector3::new(s[3], s[4], s[5]);
        (v.dot(&v) / 2.0 - GM_EARTH / r.norm(), r.cross(&v).norm())
    };
    let (energy0, h0) = invariants(&start);
    assert!(energy0 > 0.0, "the test orbit must be hyperbolic");
    let body = TwoBody::from_eci(t0, start);
    for k in -10..=10 {
        let dt = f64::from(k) * 100.0;
        let (energy, h) = invariants(&body.state_eci(t0 + dt).expect("hyperbolic step"));
        assert!(
            ((energy - energy0) / energy0).abs() <= 1e-9,
            "dt {dt}: energy {energy:e} vs {energy0:e}"
        );
        assert!(((h - h0) / h0).abs() <= 1e-9, "dt {dt}: h {h:e} vs {h0:e}");
    }
}

#[test]
fn t7_a_state_that_cannot_converge_is_an_error() {
    let t0 = fixed_epoch();
    let mut state = from_bits6(&HST.start);
    state[1] = f64::NAN;
    let err = TwoBody::from_eci(t0, state)
        .state_eci(t0 + 60.0)
        .expect_err("a NaN state must not converge");
    assert!(err.contains("no convergence"), "unexpected error: {err}");
}

/// `TRANSFER.start` stepped 5,037.25 s: Newton's iteration ends in a two-value
/// cycle a few ULP wide, above the 4 eps test, and the step accepts it after the
/// loop (macOS bits).
const TRANSFER_ULP_CYCLE: [u64; 6] = [
    0xc14807d9ec45573b,
    0xc15848781d8dc946,
    0x412cb155c286c050,
    0x40c14677cd33319e,
    0xc08d3b2bcf7f67c8,
    0x40af1596d0bf46a2,
];

#[test]
fn t7_a_last_ulp_cycle_is_accepted() {
    let t0 = fixed_epoch();
    let start = from_bits6(&TRANSFER.start);
    let state = TwoBody::from_eci(t0, start)
        .state_eci(t0 + 5_037.25)
        .expect("a last-ULP cycle converges");
    let found = mismatches(
        "transfer state_eci(dt 5037.25)",
        &state,
        &TRANSFER_ULP_CYCLE,
    );
    assert!(
        found.is_empty(),
        "known-answer mismatches:\n{}",
        found.join("\n")
    );
    // And it is the right state: brahe's propagator from the same elements.
    let koe = state_to_elements(start);
    let reference = KeplerianPropagator::from_keplerian(t0, koe, AngleFormat::Radians, 1.0);
    let (dp, dv) = dr(&state, &reference.state_eci(t0 + 5_037.25).expect("brahe"));
    assert!(dp <= 1e-6 && dv <= 1e-9, "{dp:e} m, {dv:e} m/s from brahe");
}
