//! Tests of `orbit::sgp4_start` (178-P11 S2): known-answer bits of the TEME state
//! from the portable SGP4 copy, the same state against brahe's `SGPPropagator`
//! (built as `orbit::sgp4_state_eci` builds it), and the error paths.
//!
//! The known-answer constants were made on macOS (aarch64); `full-ci` asserts the
//! same constants on Linux and Windows. A failure on one platform is a finding,
//! never a constant to change.

use brahe::propagators::SGPPropagator;
use brahe::utils::SStateProvider;
use nalgebra::{Vector3, Vector6};

use crate::orbit::catalog::{GpElements, JsonOrbital, gp_record_to_elements};
use crate::orbit::sgp4_start::teme_state;
use crate::orbit::use_zero_eop;
use crate::test_support::fixed_epoch;
use crate::test_support::q13::{CHASER_NORAD_ID, HST_NORAD_ID, fixture_record};

/// A deep-space record (period about 718 minutes, SGP4's deep-space branch):
/// MERIDIAN 7, a Molniya orbit, verbatim from `assets/datasets/dataset_2026_04_03.json`
/// (CelesTrak GP data, <https://celestrak.org/NORAD/elements/gp.php?GROUP=ACTIVE&FORMAT=JSON>).
const MERIDIAN_7: &str = r#"{"OBJECT_NAME": "MERIDIAN 7", "OBJECT_ID": "2014-069A", "EPOCH": "2026-04-03T09:43:43.015008", "MEAN_MOTION": 2.00582504, "ECCENTRICITY": 0.6675715, "INCLINATION": 63.4569, "RA_OF_ASC_NODE": 222.7303, "ARG_OF_PERICENTER": 271.0485, "MEAN_ANOMALY": 19.1891, "EPHEMERIS_TYPE": 0, "CLASSIFICATION_TYPE": "U", "NORAD_CAT_ID": 40296, "ELEMENT_SET_NO": 999, "REV_AT_EPOCH": 8373, "BSTAR": 0, "MEAN_MOTION_DOT": 1.47e-06, "MEAN_MOTION_DDOT": 0}"#;

/// Offsets from the reference epoch (`fixed_epoch()`, the HST record's epoch), s:
/// 0, 1 h, 1 d, 30 d, 219 d.
const OFFSETS: [f64; 5] = [0.0, 3_600.0, 86_400.0, 2_592_000.0, 18_921_600.0];

fn gp(record: &JsonOrbital) -> GpElements {
    gp_record_to_elements(record, fixed_epoch())
        .expect("record converts")
        .1
}

/// The three records, in the order of [`KNOWN`].
fn records() -> [(&'static str, GpElements); 3] {
    let meridian: JsonOrbital = serde_json::from_str(MERIDIAN_7).expect("record parses");
    [
        ("hst", gp(&fixture_record(HST_NORAD_ID))),
        ("chaser", gp(&fixture_record(CHASER_NORAD_ID))),
        ("meridian_7", gp(&meridian)),
    ]
}

/// brahe's SGP4 propagator for `gp`, with the arguments `orbit::sgp4_state_eci`
/// passes it.
fn brahe_sgp4(gp: &GpElements) -> SGPPropagator {
    use_zero_eop();
    SGPPropagator::from_omm_elements(
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
    .expect("brahe SGP4")
}

/// `teme_state` bits per record (rows of [`records`]) and offset (rows of
/// [`OFFSETS`]), made on macOS.
const KNOWN: [[[u64; 6]; 5]; 3] = [
    // hst
    [
        [
            0xc150f56bab5bc713,
            0xc153e357137f9e36,
            0x4022da20864c9509,
            0x40b3f3c655570c55,
            0xc0b0fc8d2e9ee737,
            0x40ac713297cf3236,
        ],
        [
            0xc1233ac56f9b7188,
            0x41583a367cf8349d,
            0xc1431c9d6e06cbe6,
            0xc0bba3f96cf4e967,
            0xc099550afa27af26,
            0xc0a23ef6baa215fc,
        ],
        [
            0x4156eb5cc38f9295,
            0xc138ca6dc93e3e01,
            0x4145ca0cd420c84b,
            0x40a52e606ec8c06c,
            0x40bb04a55422ffd3,
            0xc09b7dd9da3b6cf1,
        ],
        [
            0x415750ec341ae667,
            0x41478dc47d24eb56,
            0x410059d7f877159f,
            0xc0a82a7ca23088a8,
            0x40b74410da42ad8f,
            0x40ac6c8da6a5d7cb,
        ],
        [
            0x4156549976d0bc3c,
            0xc1495fb5262fae6a,
            0x4131e75009990120,
            0x40ae5d86ef36f50a,
            0x40b6015c4453ba63,
            0xc0aa8f2ec8af509d,
        ],
    ],
    // chaser
    [
        [
            0xc150f797fa0832cf,
            0xc153e137be7c3515,
            0xc09a21ce958b4979,
            0x40b3f193147f2b4c,
            0xc0b0ff72f0714deb,
            0x40ac716bc9d79c89,
        ],
        [
            0xc123258f6a36d50c,
            0x41583b22d09cb8ad,
            0xc1431b172a3f1b2f,
            0xc0bba3ecce2d0d73,
            0xc0994875fd9e81ae,
            0xc0a24126c0aa86d3,
        ],
        [
            0x4156ea5b45e08e4d,
            0xc138d2d472df6ae3,
            0x4145cafb9d574995,
            0x40a533d1ca953b8e,
            0x40bb041fa45a660e,
            0xc09b7902ca5fa860,
        ],
        [
            0x415751ddfa686c32,
            0x414788cddee1b1d6,
            0x41002bedc7934464,
            0xc0a824e98f22e353,
            0x40b745d0bb0fbf07,
            0x40ac6d032fe2b6f1,
        ],
        [
            0x4156587dc3f0ad78,
            0xc1495570f33324ed,
            0x4131dab9ef513f55,
            0x40ae4ef516155d0e,
            0x40b60525281f08a6,
            0xc0aa91bebfdffa78,
        ],
    ],
    // meridian_7
    [
        [
            0x417139bd15126441,
            0xc161e821c4e3b325,
            0x418248d991491a73,
            0x408f0692d5bd36dc,
            0x40958b0cb7d2ae07,
            0xc0855a406c95a0c3,
        ],
        [
            0x41740681d8d801ce,
            0xc14fd36a2f7cc3be,
            0x41808518a3e05847,
            0x4083481c2e02a09a,
            0x40978d4a70c65503,
            0xc0959c33d23f9dc6,
        ],
        [
            0x417171830a23770e,
            0xc1614e0d2f7fb759,
            0x418232f4b8a11972,
            0x408e68bea7bb207f,
            0x4095b27569556044,
            0xc086d2c492097e73,
        ],
        [
            0x41754dc4e668c91d,
            0x412705879e9441f2,
            0x417a23c65ba8ed01,
            0x40537c536c432cfd,
            0x4098068308c698fa,
            0xc0a1bf62a8989be3,
        ],
        [
            0x4172208d5182aa89,
            0x414794dafeb97e3c,
            0x415d8521791cc854,
            0xc09dac106a77cc23,
            0x4097a49fe31917ba,
            0xc0b05821e8f69e1b,
        ],
    ],
];

#[test]
fn teme_state_known_answer_bits() {
    let t0 = fixed_epoch();
    let mut found = Vec::new();
    for (r, (name, gp)) in records().iter().enumerate() {
        for (k, dt) in OFFSETS.iter().enumerate() {
            let state = teme_state(gp, t0 + *dt).expect("teme_state");
            for i in 0..6 {
                if state[i].to_bits() != KNOWN[r][k][i] {
                    found.push(format!(
                        "{name} dt {dt} [{i}]: got {:#018x} ({:e}), expected {:#018x} ({:e})",
                        state[i].to_bits(),
                        state[i],
                        KNOWN[r][k][i],
                        f64::from_bits(KNOWN[r][k][i])
                    ));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "known-answer mismatches:\n{}",
        found.join("\n")
    );
}

#[test]
fn teme_state_matches_brahe() {
    let t0 = fixed_epoch();
    let (mut worst_m, mut worst_mps) = (0.0f64, 0.0f64);
    for (name, gp) in records() {
        let reference = brahe_sgp4(&gp);
        for dt in OFFSETS {
            let ours = teme_state(&gp, t0 + dt).expect("teme_state");
            let theirs: Vector6<f64> = SStateProvider::state(&reference, t0 + dt).expect("brahe");
            let d = ours - theirs;
            let dp = Vector3::new(d[0], d[1], d[2]).norm();
            let dv = Vector3::new(d[3], d[4], d[5]).norm();
            // Low orbits: 1e-8 m. The deep-space record is 2e7 to 4.3e7 m from the
            // Earth, where 1e-8 m is about one ULP of a position component, so its
            // position is bounded relative to its distance (operator decision,
            // 178-P11 Run 1).
            let r = Vector3::new(ours[0], ours[1], ours[2]).norm();
            let bound_m = if name == "meridian_7" {
                1e-15 * r
            } else {
                1e-8
            };
            assert!(
                dp <= bound_m && dv <= 1e-11,
                "{name} dt {dt}: {dp:e} m (bound {bound_m:e}), {dv:e} m/s from brahe's SGP4"
            );
            worst_m = worst_m.max(dp);
            worst_mps = worst_mps.max(dv);
        }
    }
    println!("SGP4 largest difference from brahe: {worst_m:.3e} m, {worst_mps:.3e} m/s");
}

#[test]
fn a_hyperbolic_record_is_an_error() {
    let mut gp = gp(&fixture_record(HST_NORAD_ID));
    gp.eccentricity = 1.5;
    let err = teme_state(&gp, fixed_epoch()).expect_err("e = 1.5 must fail");
    assert!(err.contains("20580"), "the error names the NORAD id: {err}");
}

#[test]
fn an_unparsable_epoch_is_an_error() {
    let mut gp = gp(&fixture_record(HST_NORAD_ID));
    gp.epoch_str = "2026-04-03 16:03:47".to_string();
    let err = teme_state(&gp, fixed_epoch()).expect_err("a bad epoch must fail");
    assert!(
        err.contains("20580") && err.contains("invalid epoch"),
        "{err}"
    );
}

#[test]
fn the_epoch_without_a_fraction_and_the_classifications_are_read() {
    let base = gp(&fixture_record(HST_NORAD_ID));
    let t = fixed_epoch() + 600.0;
    // An epoch without fractional seconds is read by the second format.
    let mut whole = base.clone();
    whole.epoch_str = "2026-04-03T16:03:47".to_string();
    assert!(teme_state(&whole, t).is_ok());
    // 'U', ' ', 'C', 'S' and no classification give the same state (SGP4 does
    // not use it); any other character is an error.
    let expected = teme_state(&base, t).expect("base");
    for c in [Some('U'), Some(' '), Some('C'), Some('S'), None] {
        let mut g = base.clone();
        g.classification = c;
        assert_eq!(
            teme_state(&g, t).expect("classification").map(f64::to_bits),
            expected.map(f64::to_bits)
        );
    }
    let mut bad = base;
    bad.classification = Some('X');
    let err = teme_state(&bad, t).expect_err("'X' must fail");
    assert!(
        err.contains("20580") && err.contains("classification"),
        "{err}"
    );
}
