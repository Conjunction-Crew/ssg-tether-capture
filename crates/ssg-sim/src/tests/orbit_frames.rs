//! Tests of `orbit::frames` (178-P11 S3): TEME → GCRF on portable math.
//!
//! F1 and F5 are known-answer tests: their constants were made on macOS (aarch64)
//! and `full-ci` asserts the same constants on Linux and Windows. A failure on one
//! platform is a finding (a platform difference), never a constant to change. F2
//! compares the port with brahe 1.3.4 (the C SOFA code it calls is fused with
//! multiply-add on macOS, so the two differ there by more than elsewhere). F3 pins
//! the shape of the generated `nut00a` tables, F4 checks the matrices are rotations.

use brahe::propagators::SGPPropagator;
use brahe::utils::{SOrbitStateProvider, SStateProvider};
use brahe::{Epoch, TimeSystem};
use nalgebra::{Matrix3, Vector3, Vector6};

use crate::orbit::catalog::{GpElements, gp_record_to_elements};
use crate::orbit::frames::{
    self, FrameTimes, bias_precession_nutation, earth_rotation, iau2006, polar_motion,
    teme_to_gcrf, teme_to_gcrf_at,
};
use crate::orbit::use_zero_eop;
use crate::test_support::fixed_epoch;
use crate::test_support::q13::{CHASER_NORAD_ID, HST_NORAD_ID, fixture_record};

/// The Julian date of MJD 0, as the chain passes it to ERFA's two-part dates.
const MJD_ZERO: f64 = 2400000.5;

/// The six epochs of F1 and F5, UTC: 2000-01-01, 2010-06-15, 2020-02-29 (each at
/// 00:00:00), the reference epoch 2026-04-03T16:03:47.002464 (`fixed_epoch()`),
/// 2033-11-11 and 2040-12-31 (00:00:00).
fn six_epochs() -> [Epoch; 6] {
    let day = |y, mo, d| Epoch::from_datetime(y, mo, d, 0, 0, 0.0, 0.0, TimeSystem::UTC);
    [
        day(2000, 1, 1),
        day(2010, 6, 15),
        day(2020, 2, 29),
        fixed_epoch(),
        day(2033, 11, 11),
        day(2040, 12, 31),
    ]
}

/// Index of the reference epoch in [`six_epochs`].
const REF: usize = 3;

/// The pairs `(e2, e1)` of [`six_epochs`] whose difference `e2 - e1` (s) F5 pins.
const PAIRS: [(usize, usize); 4] = [(1, 0), (3, 2), (5, 3), (0, 5)];

/// The TEME states of the two reference records at the reference epoch, from
/// Run 0's macOS `start.tsv` (`hst.sgp4_teme`, `chaser.sgp4_teme`), m and m/s.
const TEME_BITS: [[u64; 6]; 2] = [
    [
        0xc150f56bab5bc713,
        0xc153e357137f9e36,
        0x4022da20864c9509,
        0x40b3f3c655570c55,
        0xc0b0fc8d2e9ee737,
        0x40ac713297cf3236,
    ],
    [
        0xc150f797fa0832cf,
        0xc153e137be7c3515,
        0xc09a21ce958b4979,
        0x40b3f193147f2b4c,
        0xc0b0ff72f0714deb,
        0x40ac716bc9d79c89,
    ],
];

// ---- known answers (made on macOS) ----

/// `FrameTimes` (`jd_ut1`, `mjd_ut1`, `mjd_tt`) of [`six_epochs`] from brahe's
/// `Epoch` with zero Earth-orientation parameters (F5), and the inputs of F1.
const TIMES_BITS: [[u64; 3]; 6] = [
    [0x4142b42c40000000, 0x40e92b0000000000, 0x40e92b000615e97e],
    [0x4142bba140000000, 0x40eb084000000000, 0x40eb084006467510],
    [0x4142c28e40000000, 0x40ecc38000000000, 0x40ecc380068f4669],
    [0x4142c6e715ab6d16, 0x40edd9b56adb458b, 0x40edd9b5716a8bf5],
    [0x4142cc5440000000, 0x40ef350000000000, 0x40ef3500068f4669],
    [0x4142d16bc0000000, 0x40f03d7000000000, 0x40f03d700347a335],
];

/// `e2 - e1` (s) for [`PAIRS`] (F5).
const DIFF_BITS: [u64; 4] = [
    0x41b3a97f02000000, // 329875202.0
    0x41a6ec76c60142f6, // 192297827.002464
    0x41bbbbcd1cff5e85, // 465292572.997536
    0xc1d3479ba1400000, // -1293840005.0
];

/// `teme_to_gcrf_at` of [`TEME_BITS`] at the reference epoch's times (F1).
const GCRF_BITS: [[u64; 6]; 2] = [
    [
        0xc151133927e23a37,
        0xc153c9c35254565d,
        0x40c6c876f38a8162,
        0x40b3e377a8f91798,
        0xc0b11a55a9d8b99b,
        0x40ac5771be1b0b0c,
    ],
    [
        0xc151156356cfb2a5,
        0xc153c7a0bf6ebad2,
        0x40c38252ded90fad,
        0x40b3e14022fbf2c3,
        0xc0b11d38198ba2f8,
        0x40ac57ade64c5b66,
    ],
];

/// The nine entries of `bias_precession_nutation`, row by row, at [`six_epochs`] (F1).
const BPN_BITS: [[u64; 9]; 6] = [
    [
        0x3fefffffffcd9c89,
        0x3e44f1b294000000,
        0x3efc64da234e79a8,
        0xbe4691e24a000000,
        0x3fefffffffca49f6,
        0x3efd50acd87a373f,
        0xbefc64da1e52db4b,
        0xbefd50acdd4dc001,
        0x3fefffffff97e67f,
    ],
    [
        0x3feffffed9696785,
        0xbe4f4465e4c80000,
        0xbf5129dfd0ba0d94,
        0x3e4172e8a2380000,
        0x3feffffffffd6843,
        0xbed9c3593b4a38fb,
        0x3f5129dfd0bef4a7,
        0x3ed9c355f7260001,
        0x3feffffed966cfca,
    ],
    [
        0x3feffffc1b3e7a76,
        0x3e484bc11dd80000,
        0xbf5f92490e64b576,
        0xbe0651fb42800000,
        0x3feffffffffde536,
        0x3ed73636f3ed9fe1,
        0x3f5f92490e6709cb,
        0xbed73633c8d40001,
        0x3feffffc1b3c5fad,
    ],
    [
        0x3feffff91da0abc9,
        0xbe30e4a721e00000,
        0xbf64fd83cc191052,
        0xbe77a5c3a9160000,
        0x3fefffffffa75ae2,
        0xbf02d48a7bbec0c2,
        0x3f64fd83cbe3e341,
        0x3f02d48e3028c000,
        0x3feffff91d4806ba,
    ],
    [
        0x3feffff4a15d0cbf,
        0x3e3487e3f3400000,
        0xbf6af9a18962bf72,
        0x3e88d2902c040000,
        0x3fefffffff1be06c,
        0x3f0e3514dbfbc7ad,
        0x3f6af9a188ac22ef,
        0xbf0e351f0d1fc000,
        0x3feffff4a078ed75,
    ],
    [
        0x3fefffef932c1780,
        0x3e5c2df8c24d0000,
        0xbf70360c8e48b994,
        0xbe6bfeca38e70000,
        0x3feffffffffd0e8b,
        0xbedb72c60bbaef98,
        0x3f70360c8e443665,
        0x3edb72f0afb70000,
        0x3fefffef93292617,
    ],
];

/// `era00(MJD_ZERO, mjd_ut1)` at [`six_epochs`] (F1).
const ERA_BITS: [u64; 6] = [
    0x3ffbea914b4658f8,
    0x40125d041dd6e408,
    0x4006103404cec8d0,
    0x3ff446c2e9f6d100,
    0x3febefd00d7effc0,
    0x3ffbc9f83fc10f40,
];

/// `sp00(MJD_ZERO, mjd_tt)` at [`six_epochs`] (F1).
const SP_BITS: [u64; 6] = [
    0x3cec0dda5db73ea0,
    0xbdba2f7f40ff2730,
    0xbdc9411ee3afa649,
    0xbdd0719fc8c834cc,
    0xbdd5354e1b4aaff0,
    0xbdd9adc5b27f50b8,
];

// ---- helpers ----

fn v6(bits: &[u64; 6]) -> Vector6<f64> {
    Vector6::from_fn(|i, _| f64::from_bits(bits[i]))
}

fn times(bits: &[u64; 3]) -> FrameTimes {
    FrameTimes {
        jd_ut1: f64::from_bits(bits[0]),
        mjd_ut1: f64::from_bits(bits[1]),
        mjd_tt: f64::from_bits(bits[2]),
    }
}

/// Row-by-row entries of a matrix.
fn entries(m: &Matrix3<f64>) -> [f64; 9] {
    [
        m[(0, 0)],
        m[(0, 1)],
        m[(0, 2)],
        m[(1, 0)],
        m[(1, 1)],
        m[(1, 2)],
        m[(2, 0)],
        m[(2, 1)],
        m[(2, 2)],
    ]
}

/// One line per value whose bits differ from `expected`, naming both.
fn mismatches(name: &str, actual: &[f64], expected: &[u64]) -> Vec<String> {
    actual
        .iter()
        .zip(expected)
        .enumerate()
        .filter(|(_, (a, e))| a.to_bits() != **e)
        .map(|(i, (a, e))| {
            format!(
                "{name}[{i}]: got {:#018x} ({a:e}), expected {e:#018x} ({:e})",
                a.to_bits(),
                f64::from_bits(*e)
            )
        })
        .collect()
}

fn assert_none(found: Vec<String>) {
    assert!(
        found.is_empty(),
        "known-answer mismatches:\n{}",
        found.join("\n")
    );
}

fn max_abs_diff(a: &Matrix3<f64>, b: &Matrix3<f64>) -> f64 {
    (a - b).abs().max()
}

/// Fifty epochs spread over 2000 to 2040 (UTC), each as the GP epoch string brahe
/// parses and as the `Epoch` brahe builds from it.
fn fifty_epochs() -> Vec<(String, Epoch)> {
    (0..50u32)
        .map(|k| {
            let year = 2000 + (k * 40) / 49;
            let month = 1 + (k * 7) % 12;
            let day = 1 + (k * 11) % 28;
            let hour = (k * 5) % 24;
            let minute = (k * 17) % 60;
            let second = (k * 31) % 60;
            let micro = (k * 123_457) % 1_000_000;
            let text = format!(
                "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{micro:06}"
            );
            let epoch = Epoch::from_datetime(
                year,
                month as u8,
                day as u8,
                hour as u8,
                minute as u8,
                second as f64 + (micro * 1000) as f64 / 1e9,
                0.0,
                TimeSystem::UTC,
            );
            (text, epoch)
        })
        .collect()
}

/// brahe's SGP4 propagator for `gp` with its epoch replaced by `epoch_str`, with
/// the arguments `orbit::sgp4_state_eci` passes it.
fn brahe_sgp4_at(gp: &GpElements, epoch_str: &str) -> SGPPropagator {
    SGPPropagator::from_omm_elements(
        epoch_str,
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

fn reference_records() -> [GpElements; 2] {
    let gp = |id| {
        gp_record_to_elements(&fixture_record(id), fixed_epoch())
            .expect("record converts")
            .1
    };
    [gp(HST_NORAD_ID), gp(CHASER_NORAD_ID)]
}

// ---- F1 ----

#[test]
fn f1_teme_to_gcrf_known_answer_bits() {
    let t = times(&TIMES_BITS[REF]);
    let mut found = Vec::new();
    for (k, name) in ["hst", "chaser"].iter().enumerate() {
        let got = teme_to_gcrf_at(v6(&TEME_BITS[k]), &t);
        found.extend(mismatches(name, got.as_slice(), &GCRF_BITS[k]));
    }
    assert_none(found);
}

#[test]
fn f1_matrices_and_angles_known_answer_bits() {
    let mut found = Vec::new();
    for (k, bits) in TIMES_BITS.iter().enumerate() {
        let t = times(bits);
        let bpn = entries(&bias_precession_nutation(t.mjd_tt));
        found.extend(mismatches(&format!("bpn@{k}"), &bpn, &BPN_BITS[k]));
        found.extend(mismatches(
            &format!("era00@{k}"),
            &[iau2006::era00(MJD_ZERO, t.mjd_ut1)],
            &ERA_BITS[k..=k],
        ));
        found.extend(mismatches(
            &format!("sp00@{k}"),
            &[iau2006::sp00(MJD_ZERO, t.mjd_tt)],
            &SP_BITS[k..=k],
        ));
    }
    assert_none(found);
}

// ---- F2 ----

#[test]
fn f2_matrices_match_brahe() {
    use_zero_eop();
    let (mut w_bpn, mut w_er, mut w_pm) = (0.0f64, 0.0f64, 0.0f64);
    for (text, epoch) in fifty_epochs() {
        let t = FrameTimes::from_epoch(epoch);
        let d_bpn = max_abs_diff(
            &bias_precession_nutation(t.mjd_tt),
            &brahe::frames::bias_precession_nutation(epoch),
        );
        let d_er = max_abs_diff(
            &earth_rotation(t.mjd_ut1),
            &brahe::frames::earth_rotation(epoch),
        );
        let d_pm = max_abs_diff(&polar_motion(t.mjd_tt), &brahe::frames::polar_motion(epoch));
        assert!(
            d_bpn <= 1e-13 && d_er <= 1e-13 && d_pm <= 1e-15,
            "{text}: bpn {d_bpn:e} (bound 1e-13), er {d_er:e} (1e-13), pm {d_pm:e} (1e-15)"
        );
        w_bpn = w_bpn.max(d_bpn);
        w_er = w_er.max(d_er);
        w_pm = w_pm.max(d_pm);
    }
    println!(
        "FRAMES matrices, largest entry difference from brahe over 50 epochs: \
         bpn {w_bpn:.3e}, er {w_er:.3e}, pm {w_pm:.3e}"
    );
}

#[test]
fn f2_state_matches_brahe() {
    use_zero_eop();
    let (mut w_m, mut w_mps) = (0.0f64, 0.0f64);
    for gp in reference_records() {
        for (text, epoch) in fifty_epochs() {
            let reference = brahe_sgp4_at(&gp, &text);
            for dt in [0.0, 2_700.0] {
                let at = epoch + dt;
                let teme: Vector6<f64> = SStateProvider::state(&reference, at).expect("brahe TEME");
                let theirs = SOrbitStateProvider::state_eci(&reference, at).expect("brahe GCRF");
                let ours = teme_to_gcrf(teme, at);
                let d = ours - theirs;
                let dp = Vector3::new(d[0], d[1], d[2]).norm();
                let dv = Vector3::new(d[3], d[4], d[5]).norm();
                assert!(
                    dp <= 1e-6 && dv <= 1e-9,
                    "{} at {text} + {dt} s: {dp:e} m (bound 1e-6), {dv:e} m/s (bound 1e-9)",
                    gp.norad_id
                );
                w_m = w_m.max(dp);
                w_mps = w_mps.max(dv);
            }
        }
    }
    println!(
        "FRAMES state, largest difference from brahe's state_eci (2 records x 50 epochs x 2): \
         {w_m:.3e} m, {w_mps:.3e} m/s"
    );
}

// ---- F3 ----

#[test]
fn f3_nut00a_tables_have_erfas_shape() {
    let (xls, xpl) = frames::nut00a_tables();
    assert_eq!((xls.len(), xpl.len()), (678, 687));

    // Column sums as the generator printed them (first row to last; the f64
    // columns summed as f64).
    let ls_n: Vec<i64> = (0..5)
        .map(|j| xls.iter().map(|r| r.n[j] as i64).sum())
        .collect();
    assert_eq!(ls_n, [43, -15, 724, 419, 789]);
    let ls_f: Vec<u64> = [
        xls.iter().fold(0.0, |s, r| s + r.sp),
        xls.iter().fold(0.0, |s, r| s + r.spt),
        xls.iter().fold(0.0, |s, r| s + r.cp),
        xls.iter().fold(0.0, |s, r| s + r.ce),
        xls.iter().fold(0.0, |s, r| s + r.cet),
        xls.iter().fold(0.0, |s, r| s + r.se),
    ]
    .iter()
    .map(|x: &f64| x.to_bits())
    .collect();
    assert_eq!(
        ls_f,
        [
            0xc1a5ea49c6000000, // -183837923.0
            0xc105e94800000000, // -179497.0
            0x40de32c000000000, // 30923.0
            0x41977142f4000000, // 98324669.0
            0x40b5120000000000, // 5394.0
            0x40c0d58000000000, // 8619.0
        ]
    );

    let pl_n: Vec<i64> = (0..13)
        .map(|j| xpl.iter().map(|r| r.n[j] as i64).sum())
        .collect();
    assert_eq!(
        pl_n,
        [-49, 156, -34, 263, -1, -54, 551, 80, -19, -7, 2, 8, 481]
    );
    let pl_c: [i64; 4] = [
        xpl.iter().map(|r| r.sp as i64).sum(),
        xpl.iter().map(|r| r.cp as i64).sum(),
        xpl.iter().map(|r| r.se as i64).sum(),
        xpl.iter().map(|r| r.ce as i64).sum(),
    ];
    assert_eq!(pl_c, [-5366, 10547, 819, 1121]);
}

// ---- F4 ----

#[test]
fn f4_matrices_are_rotations_and_the_radius_is_kept() {
    let mut all_times: Vec<FrameTimes> = TIMES_BITS.iter().map(times).collect();
    use_zero_eop();
    all_times.extend(
        fifty_epochs()
            .into_iter()
            .map(|(_, e)| FrameTimes::from_epoch(e)),
    );
    let mut worst_orth = 0.0f64;
    let mut worst_r = 0.0f64;
    for t in &all_times {
        for (name, m) in [
            ("bpn", bias_precession_nutation(t.mjd_tt)),
            ("er", earth_rotation(t.mjd_ut1)),
            ("pm", polar_motion(t.mjd_tt)),
        ] {
            let off = max_abs_diff(&(m * m.transpose()), &Matrix3::identity());
            assert!(off <= 1e-15, "{name} at {t:?}: |M·Mᵀ − I| = {off:e}");
            worst_orth = worst_orth.max(off);
        }
        for bits in &TEME_BITS {
            let teme = v6(bits);
            let gcrf = teme_to_gcrf_at(teme, t);
            let dr = (gcrf.fixed_rows::<3>(0).norm() - teme.fixed_rows::<3>(0).norm()).abs();
            assert!(dr <= 1e-6, "|r| changed by {dr:e} m at {t:?}");
            worst_r = worst_r.max(dr);
        }
    }
    println!("FRAMES F4: largest |M·Mᵀ − I| {worst_orth:.3e}, largest |r| change {worst_r:.3e} m");
}

// ---- F5 ----

#[test]
fn f5_what_stays_with_brahes_epoch_known_answer_bits() {
    use_zero_eop();
    let epochs = six_epochs();
    let mut found = Vec::new();
    for (k, e) in epochs.iter().enumerate() {
        let got = [
            e.jd_as_time_system(TimeSystem::UT1),
            e.mjd_as_time_system(TimeSystem::UT1),
            e.mjd_as_time_system(TimeSystem::TT),
        ];
        found.extend(mismatches(&format!("times@{k}"), &got, &TIMES_BITS[k]));
        let ft = FrameTimes::from_epoch(*e);
        found.extend(mismatches(
            &format!("FrameTimes@{k}"),
            &[ft.jd_ut1, ft.mjd_ut1, ft.mjd_tt],
            &TIMES_BITS[k],
        ));
    }
    let diffs: Vec<f64> = PAIRS.iter().map(|&(b, a)| epochs[b] - epochs[a]).collect();
    found.extend(mismatches("e2-e1", &diffs, &DIFF_BITS));
    assert_none(found);
}

// ---- the routines the chain does not call ----

#[test]
fn routines_off_the_chain() {
    // fane03 is ERFA's Neptune longitude: at t = 0 it is its constant term.
    assert_eq!(iau2006::fane03(0.0), 5.311886287);
    // rxr and tr agree with nalgebra's product and transpose bit for bit.
    let a = iau2006::pnm06a(MJD_ZERO, 60_000.0);
    let b = iau2006::pom00(1e-6, -2e-6, iau2006::sp00(MJD_ZERO, 60_000.0));
    let to_na = |r: &iau2006::Mat| Matrix3::from_fn(|i, j| r[i][j]);
    let ab = iau2006::rxr(&a, &b);
    assert_eq!(to_na(&ab), to_na(&a) * to_na(&b));
    assert_eq!(to_na(&iau2006::tr(&a)), to_na(&a).transpose());
    // c2ixys at the pole (x = y = 0) takes the branch without atan2.
    let r = iau2006::c2ixys(0.0, 0.0, 0.0);
    assert_eq!(to_na(&r), Matrix3::identity());
}
