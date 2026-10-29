//! Tests for the catalog module (P5 S5): the f32 Kepler propagation the map uses,
//! the display names, the record conversion and the dataset loader with its log
//! lines. Loader tests write their datasets under `std::env::temp_dir()`.

use std::f64::consts::TAU;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use brahe::utils::DOrbitStateProvider;
use brahe::{AngleFormat, Epoch, GM_EARTH, KeplerianPropagator};
use nalgebra::Vector6;

use crate::orbit::catalog::{
    GpuComputeEpochOrigin, GpuElements, GpuOrbitalElements, JsonOrbital, SpaceCatalogEntry,
    SpaceObjectCatalog, gp_record_to_elements, load_dataset_entities, propagate_catalog_eci_state,
};
use crate::project::{AssetsDir, WorkingDirectory};
use crate::telemetry::{LogEvent, LogLevel};
use crate::test_support::{Lcg, LogTap, TempDir, assert_close, fixed_epoch, q13, repo_root};
use crate::time::WorldTime;

// ---------------------------------------------------------------------------
// propagate_catalog_eci_state
// ---------------------------------------------------------------------------

/// The Kepler state brahe gives for the same f32 elements widened to f64.
fn brahe_state(el: &GpuOrbitalElements, offset_s: f32) -> Vector6<f64> {
    let elements = Vector6::new(
        el.a as f64,
        el.e as f64,
        el.i as f64,
        el.raan as f64,
        el.argp as f64,
        el.mean_anomaly as f64,
    );
    let epoch = fixed_epoch() + el.epoch_offset_seconds as f64;
    KeplerianPropagator::from_keplerian(epoch, elements, AngleFormat::Radians, 1.0)
        .state_eci(fixed_epoch() + offset_s as f64)
        .expect("brahe Kepler propagation")
}

/// `GpuOrbitalElements` has no `Debug`.
fn show(el: &GpuOrbitalElements) -> String {
    format!(
        "[a {} e {} i {} raan {} argp {} M {} epoch {}]",
        el.a, el.e, el.i, el.raan, el.argp, el.mean_anomaly, el.epoch_offset_seconds
    )
}

fn elements(a: f32, e: f32, i: f32, raan: f32, argp: f32, mean_anomaly: f32) -> GpuOrbitalElements {
    GpuOrbitalElements {
        id: 1,
        a,
        e,
        i,
        raan,
        argp,
        mean_anomaly,
        epoch_offset_seconds: 0.0,
    }
}

/// The function's gravitational parameter (F-50: not brahe's `GM_EARTH`).
const MU_CATALOG: f64 = 3.986_004_4e14;

/// The along-track angle (rad) by which the function's own `μ` and brahe's
/// `GM_EARTH` drift apart over `offset_s`: `n·t·δμ/(2μ)`.
fn mu_drift_angle(a: f64, offset_s: f64) -> f64 {
    let n = (MU_CATALOG / (a * a * a)).sqrt();
    n * offset_s.abs() * (GM_EARTH - MU_CATALOG).abs() / (2.0 * MU_CATALOG)
}

/// Checks `propagate_catalog_eci_state` against brahe at `offset_s`.
///
/// Tolerance, from the two known differences: (1) the function returns f32
/// vectors, so each component carries up to half an f32 ulp (6e-8 relative); the
/// norm of three such errors is below `1.04e-7 |r|`; (2) its `μ = 3.9860044e14`
/// differs from brahe's `GM_EARTH` by 4.5e-9 relative (F-50), so its mean motion is
/// off by 2.3e-9 relative and the body drifts along-track by `n·t·2.3e-9` rad,
/// which moves the position by that angle times `|r|` and the velocity by that
/// angle times `|v|`. The bound is `2e-7` of the magnitude, plus 1.1 times the
/// drift, plus 1e-3.
#[track_caller]
fn assert_matches_brahe(el: &GpuOrbitalElements, offset_s: f32, what: &str) {
    let (r, v) = propagate_catalog_eci_state(el, offset_s)
        .unwrap_or_else(|| panic!("{what}: no state for {}", show(el)));
    let expected = brahe_state(el, offset_s);
    let (er, ev) = (
        bevy::math::DVec3::new(expected[0], expected[1], expected[2]),
        bevy::math::DVec3::new(expected[3], expected[4], expected[5]),
    );
    let dt = offset_s as f64 - el.epoch_offset_seconds as f64;
    let drift = 1.1 * mu_drift_angle(el.a as f64, dt);
    let dr = (r.as_dvec3() - er).length();
    let dv = (v.as_dvec3() - ev).length();
    assert!(
        dr <= 2e-7 * er.length() + drift * er.length() + 1e-3,
        "{what}: position off by {dr:e} m (|r| {:e}) for {} at {offset_s} s",
        er.length(),
        show(el)
    );
    assert!(
        dv <= 2e-7 * ev.length() + drift * ev.length() + 1e-3,
        "{what}: velocity off by {dv:e} m/s (|v| {:e}) for {} at {offset_s} s",
        ev.length(),
        show(el)
    );
}

#[test]
fn propagate_catalog_mu_differs_from_gm_earth_by_a_known_drift() {
    // F-50 (out of scope here): the function keeps its own μ. Over 300,000 s (about
    // 50 orbits) the ISS-like body is ahead or behind brahe's by the predicted
    // along-track drift; this documents the size instead of asserting equality.
    let el = elements(6_799_130.0, 0.00112, 0.90114, 3.54993, 1.51296, 4.77190);
    let (r, _) = propagate_catalog_eci_state(&el, 300_000.0).unwrap();
    let expected = brahe_state(&el, 300_000.0);
    let er = bevy::math::DVec3::new(expected[0], expected[1], expected[2]);
    let gap = (r.as_dvec3() - er).length();
    let predicted = mu_drift_angle(el.a as f64, 300_000.0) * er.length();
    assert!(
        (0.7 * predicted..1.3 * predicted).contains(&gap),
        "gap {gap} m vs predicted drift {predicted} m"
    );
    assert!(gap > 1.0, "the difference is metres after 50 orbits: {gap}");
    // At the elements' own epoch there is no drift: only the f32 rounding remains.
    let (r0, _) = propagate_catalog_eci_state(&el, 0.0).unwrap();
    let e0 = brahe_state(&el, 0.0);
    let gap0 = (r0.as_dvec3() - bevy::math::DVec3::new(e0[0], e0[1], e0[2])).length();
    assert!(gap0 < 1.0, "no drift at the epoch: {gap0}");
}

#[test]
fn propagate_catalog_matches_brahe_kepler_at_several_offsets() {
    let sets = [
        (
            "ISS-like",
            elements(6_799_130.0, 0.00112, 0.90114, 3.54993, 1.51296, 4.77190),
        ),
        ("circular", elements(7_000_000.0, 0.0, 0.5, 1.0, 0.0, 2.0)),
        ("moderate e", elements(1.2e7, 0.3, 1.1, 2.5, 4.0, 0.7)),
        ("Molniya-like", elements(2.6e7, 0.7, 1.1, 0.3, 4.7, 5.9)),
    ];
    // Zero, forward, an hour, a period or more, and 50 orbits (the mean anomaly
    // wraps many times).
    for (name, el) in sets {
        for offset in [0.0_f32, 600.0, 3_600.0, 5_400.0, 86_400.0, 300_000.0] {
            assert_matches_brahe(&el, offset, name);
        }
    }
}

#[test]
fn propagate_catalog_solves_kepler_for_highly_eccentric_orbits() {
    // e > 0.8 starts the Newton iteration at +-pi (the branch of the solver);
    // mean anomalies on both sides of the apsides.
    for e in [0.82_f32, 0.9] {
        for mean_anomaly in [0.2_f32, 1.0, 2.0, 3.0, 3.5, 5.0, 6.1] {
            let el = elements(3.0e7, e, 0.6, 1.0, 2.0, mean_anomaly);
            assert_matches_brahe(&el, 0.0, &format!("e {e}, M {mean_anomaly}"));
            assert_matches_brahe(&el, 7_200.0, &format!("e {e}, M {mean_anomaly}, 2 h"));
        }
    }
}

#[test]
fn propagate_catalog_measures_time_from_the_elements_epoch() {
    // The elements are valid at +100 s: at an offset of 100 s the body is at its
    // mean anomaly, and at 3,700 s it is one hour on.
    let mut el = elements(6_799_130.0, 0.00112, 0.90114, 3.54993, 1.51296, 4.77190);
    el.epoch_offset_seconds = 100.0;
    assert_matches_brahe(&el, 100.0, "at the elements' epoch");
    assert_matches_brahe(&el, 3_700.0, "one hour after the epoch");
    // Before the epoch the body is propagated backwards.
    assert_matches_brahe(&el, -1_800.0, "before the epoch");

    let (at_epoch, _) = propagate_catalog_eci_state(&el, 100.0).unwrap();
    let mut shifted = el;
    shifted.epoch_offset_seconds = 0.0;
    let (at_zero, _) = propagate_catalog_eci_state(&shifted, 0.0).unwrap();
    assert!(
        (at_epoch - at_zero).length() < 1e-3,
        "the same elements at their own epoch give the same state"
    );
}

#[test]
fn propagate_catalog_rejects_elements_that_are_not_an_ellipse() {
    let base = elements(7_000_000.0, 0.01, 0.5, 1.0, 1.0, 1.0);
    for (what, a, e) in [
        ("a = 0", 0.0, 0.01),
        ("a < 0", -7_000_000.0, 0.01),
        ("e = 1", 7_000_000.0, 1.0),
        ("e > 1", 7_000_000.0, 1.5),
        ("e < 0", 7_000_000.0, -0.1),
        ("e = NaN", 7_000_000.0, f32::NAN),
    ] {
        let el = GpuOrbitalElements { a, e, ..base };
        assert!(
            propagate_catalog_eci_state(&el, 0.0).is_none(),
            "{what} must give no state"
        );
    }
    assert!(propagate_catalog_eci_state(&base, 0.0).is_some());
}

#[test]
fn propagate_catalog_conserves_the_two_body_invariants() {
    // Property-style (A-4): random ellipses at random offsets. The state must keep
    // the radius between the apsides, the vis-viva energy and the angular momentum,
    // using the function's own μ, to the f32 output precision (1e-6 relative).
    const MU: f64 = 3.986_004_4e14;
    const SEED: u64 = 0x0B17_5EED;
    let mut rng = Lcg::new(SEED);
    for case in 0..300 {
        let el = elements(
            rng.range(6.6e6, 4.0e7) as f32,
            rng.range(0.0, 0.7) as f32,
            rng.range(0.0, 3.1) as f32,
            rng.range(0.0, TAU) as f32,
            rng.range(0.0, TAU) as f32,
            rng.range(0.0, TAU) as f32,
        );
        let offset = rng.range(-90_000.0, 90_000.0) as f32;
        let ctx = format!("seed {SEED:#x} case {case}: {} at {offset} s", show(&el));
        let (r, v) =
            propagate_catalog_eci_state(&el, offset).unwrap_or_else(|| panic!("no state: {ctx}"));
        let (r, v) = (r.as_dvec3(), v.as_dvec3());
        let (a, e) = (el.a as f64, el.e as f64);

        assert!(
            r.length() >= a * (1.0 - e) * (1.0 - 1e-6)
                && r.length() <= a * (1.0 + e) * (1.0 + 1e-6),
            "radius {} outside [{}, {}]: {ctx}",
            r.length(),
            a * (1.0 - e),
            a * (1.0 + e)
        );
        let energy = v.length_squared() / 2.0 - MU / r.length();
        assert!(
            (energy / (-MU / (2.0 * a)) - 1.0).abs() < 1e-5,
            "vis-viva energy {energy} vs {}: {ctx}",
            -MU / (2.0 * a)
        );
        let h = r.cross(v).length();
        let h_expected = (MU * a * (1.0 - e * e)).sqrt();
        assert!(
            (h / h_expected - 1.0).abs() < 1e-5,
            "angular momentum {h} vs {h_expected}: {ctx}"
        );
        assert_matches_brahe(&el, offset, &ctx);
    }
}

#[test]
fn propagate_catalog_returns_to_its_start_after_one_period() {
    let el = elements(6_799_130.0, 0.00112, 0.90114, 3.54993, 1.51296, 4.77190);
    let period = TAU * (el.a as f64).powi(3).sqrt() / 3.986_004_4e14_f64.sqrt();
    let (r0, v0) = propagate_catalog_eci_state(&el, 0.0).unwrap();
    // f32 offsets: 8 ms resolution at one period, which is 60 m of arc; compare to
    // the exact brahe state at the same f32 offset instead of to the start.
    let offset = period as f32;
    assert_matches_brahe(&el, offset, "one period");
    let (r1, v1) = propagate_catalog_eci_state(&el, offset).unwrap();
    assert!(
        (r1 - r0).length() < 80.0,
        "back to the start within the f32 offset resolution: {}",
        (r1 - r0).length()
    );
    assert!((v1 - v0).length() < 0.1, "velocity {}", (v1 - v0).length());
}

// ---------------------------------------------------------------------------
// GpElements, gp_record_to_elements
// ---------------------------------------------------------------------------

fn record(json: &str) -> JsonOrbital {
    serde_json::from_str(json).expect("record JSON")
}

/// A complete record; `{extra}` is spliced in as additional fields.
fn full_record(norad: u64, extra: &str) -> String {
    format!(
        r#"{{"OBJECT_NAME": "OBJ {norad}", "OBJECT_ID": "2000-001A", "EPOCH": "2026-04-03T16:03:47.002464",
 "MEAN_MOTION": 15.5, "ECCENTRICITY": 0.001, "INCLINATION": 51.6, "RA_OF_ASC_NODE": 120.0,
 "ARG_OF_PERICENTER": 90.0, "MEAN_ANOMALY": 45.0, "NORAD_CAT_ID": {norad}{extra}}}"#
    )
}

#[test]
fn gp_semi_major_axis_follows_keplers_third_law() {
    let (_, gp, _) =
        gp_record_to_elements(&record(&full_record(25544, "")), fixed_epoch()).unwrap();
    // T = 86400 / 15.5 s; a³ = μ T² / 4π².
    let period = 86_400.0 / 15.5;
    let expected = (GM_EARTH * period * period / (4.0 * std::f64::consts::PI.powi(2))).cbrt();
    assert_close(gp.semi_major_axis_m(), expected, 1e-3, "semi-major axis");
    assert!(
        (6.7e6..6.9e6).contains(&gp.semi_major_axis_m()),
        "a 15.5 rev/day orbit is a low Earth orbit"
    );
}

#[test]
fn gp_kepler_elements_are_in_metres_and_radians() {
    let (_, gp, _) =
        gp_record_to_elements(&record(&full_record(25544, "")), fixed_epoch()).unwrap();
    let kepler = gp.kepler_elements_rad();
    assert_eq!(kepler[0], gp.semi_major_axis_m());
    assert_eq!(kepler[1], 0.001);
    assert_close(kepler[2], 51.6_f64.to_radians(), 1e-15, "inclination");
    assert_close(kepler[3], 120.0_f64.to_radians(), 1e-15, "raan");
    assert_close(
        kepler[4],
        90.0_f64.to_radians(),
        1e-15,
        "argument of perigee",
    );
    assert_close(kepler[5], 45.0_f64.to_radians(), 1e-15, "mean anomaly");
}

#[test]
fn gp_mean_motion_and_semi_major_axis_are_consistent() {
    // sqrt(μ / a³) must give back the mean motion, for any rev/day.
    const SEED: u64 = 0x6E0_0001;
    let mut rng = Lcg::new(SEED);
    for case in 0..300 {
        let rev_per_day = rng.range(0.5, 16.5);
        let json = full_record(1, "").replace(
            "\"MEAN_MOTION\": 15.5",
            &format!("\"MEAN_MOTION\": {rev_per_day}"),
        );
        let (_, gp, _) = gp_record_to_elements(&record(&json), fixed_epoch()).unwrap();
        let a = gp.semi_major_axis_m();
        let n = (GM_EARTH / a.powi(3)).sqrt();
        let expected = rev_per_day * TAU / 86_400.0;
        assert!(
            (n / expected - 1.0).abs() < 1e-12,
            "seed {SEED:#x} case {case}: n {n:e} vs {expected:e} for {rev_per_day} rev/day"
        );
        assert_eq!(gp.kepler_elements_rad()[0], a);
    }
}

#[test]
fn gp_record_converts_every_field() {
    let reference = fixed_epoch();
    let json = full_record(
        25544,
        r#", "BSTAR": 0.0001, "MEAN_MOTION_DOT": 0.0002, "MEAN_MOTION_DDOT": 0.5,
 "EPHEMERIS_TYPE": 0, "CLASSIFICATION_TYPE": "U""#,
    )
    .replace("2026-04-03T16:03:47.002464", "2026-04-03T16:05:47.5");
    let (id, gp, gpu) = gp_record_to_elements(&record(&json), reference).expect("complete record");

    assert_eq!(id, 25544);
    assert_eq!(gp.norad_id, 25544);
    assert_eq!(gp.epoch_str, "2026-04-03T16:05:47.5");
    assert_eq!(gp.mean_motion_rev_per_day, 15.5);
    assert_eq!(gp.eccentricity, 0.001);
    assert_eq!(gp.inclination_deg, 51.6);
    assert_eq!(gp.raan_deg, 120.0);
    assert_eq!(gp.arg_perigee_deg, 90.0);
    assert_eq!(gp.mean_anomaly_deg, 45.0);
    assert_eq!(gp.bstar, Some(0.0001));
    assert_eq!(gp.mean_motion_dot, Some(0.0002));
    assert_eq!(gp.mean_motion_ddot, Some(0.5));
    assert_eq!(gp.ephemeris_type, Some(0));
    assert_eq!(gp.classification, Some('U'));
    // 16:05:47.5 is 120 s and 0.497536 s after the reference epoch.
    assert_close(gp.epoch - reference, 120.497536, 1e-6, "record epoch");

    // The buffer entry is the same elements as f32, the epoch offset against the
    // reference.
    let kepler = gp.kepler_elements_rad();
    assert_eq!(gpu.id, 25544);
    assert_eq!(gpu.a, kepler[0] as f32);
    assert_eq!(gpu.e, kepler[1] as f32);
    assert_eq!(gpu.i, kepler[2] as f32);
    assert_eq!(gpu.raan, kepler[3] as f32);
    assert_eq!(gpu.argp, kepler[4] as f32);
    assert_eq!(gpu.mean_anomaly, kepler[5] as f32);
    assert_eq!(gpu.epoch_offset_seconds, (gp.epoch - reference) as f32);
}

#[test]
fn gp_record_without_optional_fields_leaves_them_empty() {
    let (_, gp, _) = gp_record_to_elements(&record(&full_record(7, "")), fixed_epoch()).unwrap();
    assert_eq!(gp.bstar, None);
    assert_eq!(gp.mean_motion_dot, None);
    assert_eq!(gp.mean_motion_ddot, None);
    assert_eq!(gp.ephemeris_type, None);
    assert_eq!(gp.classification, None);
    // An ephemeris type that does not fit a u8 is dropped, not wrapped.
    let (_, gp, _) = gp_record_to_elements(
        &record(&full_record(7, r#", "EPHEMERIS_TYPE": 300"#)),
        fixed_epoch(),
    )
    .unwrap();
    assert_eq!(gp.ephemeris_type, None);
}

#[test]
fn gp_record_epoch_formats() {
    let reference = fixed_epoch();
    let with_epoch = |epoch: &str| {
        let json = full_record(7, "").replace("2026-04-03T16:03:47.002464", epoch);
        gp_record_to_elements(&record(&json), reference).unwrap().1
    };
    // Whole seconds, a trailing 'Z' and a bare trailing dot all read as :47.
    let whole = with_epoch("2026-04-03T16:03:47").epoch;
    assert_close(
        with_epoch("2026-04-03T16:03:47Z").epoch - whole,
        0.0,
        0.0,
        "Z suffix",
    );
    assert_close(
        with_epoch("2026-04-03T16:03:47.").epoch - whole,
        0.0,
        0.0,
        "bare dot",
    );
    assert_close(
        with_epoch("2026-04-03T16:03:47.25").epoch - whole,
        0.25,
        1e-9,
        "fraction",
    );
    // A space instead of the 'T' takes brahe's parser.
    assert_close(
        with_epoch("2026-04-03 16:03:47").epoch - whole,
        0.0,
        1e-9,
        "space separator",
    );
    // An unparsable epoch falls back to the reference epoch, offset 0.
    let (_, gp, gpu) = gp_record_to_elements(
        &record(&full_record(7, "").replace("2026-04-03T16:03:47.002464", "not a date")),
        reference,
    )
    .unwrap();
    assert_eq!(gp.epoch, reference);
    assert_eq!(gp.epoch_str, "not a date");
    assert_eq!(gpu.epoch_offset_seconds, 0.0);
    // So does a record without an epoch, whose epoch string is empty.
    let no_epoch = full_record(7, "").replace(r#""EPOCH": "2026-04-03T16:03:47.002464","#, "");
    let (_, gp, _) = gp_record_to_elements(&record(&no_epoch), reference).unwrap();
    assert_eq!(gp.epoch, reference);
    assert_eq!(gp.epoch_str, "");
}

#[test]
fn gp_record_missing_or_non_numeric_field_gives_none() {
    for field in [
        "NORAD_CAT_ID",
        "MEAN_MOTION",
        "ECCENTRICITY",
        "INCLINATION",
        "RA_OF_ASC_NODE",
        "ARG_OF_PERICENTER",
        "MEAN_ANOMALY",
    ] {
        let value = if field == "NORAD_CAT_ID" { "99" } else { "1.0" };
        let complete = full_record(99, "");
        assert!(
            gp_record_to_elements(&record(&complete), fixed_epoch()).is_some(),
            "precondition: the complete record converts"
        );
        // Remove the field.
        let pattern = format!("\"{field}\": ");
        let start = complete.find(&pattern).expect("field present");
        let end = start + complete[start..].find([',', '}']).expect("field end");
        let mut removed = complete.clone();
        removed.replace_range(start..end, &format!("\"X_{field}\": 0"));
        assert!(
            gp_record_to_elements(&record(&removed), fixed_epoch()).is_none(),
            "{field} missing must give none"
        );
        // A string where a number belongs.
        let mut text = complete.clone();
        text.replace_range(start..end, &format!("\"{field}\": \"{value}x\""));
        assert!(
            gp_record_to_elements(&record(&text), fixed_epoch()).is_none(),
            "{field} as a string must give none"
        );
    }
}

// ---------------------------------------------------------------------------
// SpaceCatalogEntry display
// ---------------------------------------------------------------------------

fn entry(name: &str, id: &str, norad: u32) -> SpaceCatalogEntry {
    let (_, gp, _) = gp_record_to_elements(&record(&full_record(norad as u64, "")), fixed_epoch())
        .expect("record");
    SpaceCatalogEntry {
        gpu_index: 0,
        norad_id: norad,
        object_name: name.to_string(),
        object_id: id.to_string(),
        search_blob: String::new(),
        gp,
    }
}

#[test]
fn display_name_falls_back_from_name_to_id_to_placeholder() {
    assert_eq!(entry("HST", "1990-037B", 1).display_name(), "HST");
    assert_eq!(entry("", "1990-037B", 1).display_name(), "1990-037B");
    assert_eq!(entry("", "", 1).display_name(), "Unnamed Object");
}

#[test]
fn display_label_includes_the_norad_id_and_the_object_id_when_there_is_one() {
    assert_eq!(
        entry("HST", "1990-037B", 20580).display_label(),
        "HST · NORAD 20580 · 1990-037B"
    );
    assert_eq!(entry("HST", "", 20580).display_label(), "HST · NORAD 20580");
    assert_eq!(entry("", "", 7).display_label(), "Unnamed Object · NORAD 7");
    // With no name the id is the name and is shown again as the id.
    assert_eq!(
        entry("", "1990-037B", 20580).display_label(),
        "1990-037B · NORAD 20580 · 1990-037B"
    );
}

// ---------------------------------------------------------------------------
// load_dataset_entities
// ---------------------------------------------------------------------------

struct Loaded {
    catalog: Vec<SpaceCatalogEntry>,
    gpu: Option<Vec<GpuOrbitalElements>>,
    origin: Option<Option<Epoch>>,
    logs: Vec<(LogLevel, &'static str, String)>,
}

/// Runs the loader on `assets`' `datasets` folder with the sim clock at
/// [`fixed_epoch`]. `gpu_buffer` / `origin` add the optional resources.
fn load(
    assets: &std::path::Path,
    existing: Vec<SpaceCatalogEntry>,
    gpu_buffer: bool,
    origin: Option<Option<Epoch>>,
) -> Loaded {
    load_in_project(assets, None, existing, gpu_buffer, origin)
}

/// [`load`] with the project folder `project` (the working directory) set.
fn load_in_project(
    assets: &std::path::Path,
    project: Option<&std::path::Path>,
    existing: Vec<SpaceCatalogEntry>,
    gpu_buffer: bool,
    origin: Option<Option<Epoch>>,
) -> Loaded {
    let mut app = App::new();
    if let Some(project) = project {
        let path = project.to_string_lossy().into_owned();
        app.insert_resource(WorkingDirectory {
            path: path.clone(),
            pending_path: path,
        });
    }
    app.add_message::<LogEvent>()
        .insert_resource(SpaceObjectCatalog { entries: existing })
        .insert_resource(WorldTime::starting_at(fixed_epoch()))
        .insert_resource(AssetsDir(assets.to_path_buf()));
    if gpu_buffer {
        app.insert_resource(GpuElements(vec![elements(1.0, 0.0, 0.0, 0.0, 0.0, 0.0)]));
    }
    if let Some(origin) = origin {
        app.insert_resource(GpuComputeEpochOrigin(origin));
    }
    let mut tap = LogTap::new(app.world());
    app.world_mut()
        .run_system_once(load_dataset_entities)
        .expect("load_dataset_entities failed to run");
    Loaded {
        catalog: app.world().resource::<SpaceObjectCatalog>().entries.clone(),
        gpu: app
            .world()
            .get_resource::<GpuElements>()
            .map(|g| g.0.clone()),
        origin: app
            .world()
            .get_resource::<GpuComputeEpochOrigin>()
            .map(|o| o.0),
        logs: tap.drain(app.world()),
    }
}

#[test]
fn loader_without_a_dataset_folder_clears_the_catalog_and_loads_nothing() {
    let dir = TempDir::new("loader-no-folder");
    let loaded = load(dir.path(), vec![entry("OLD", "", 1)], true, None);
    assert!(loaded.catalog.is_empty(), "the old entries are cleared");
    assert_eq!(
        loaded.gpu.map(|g| g.len()),
        Some(0),
        "the buffer is cleared"
    );
    assert!(
        loaded.logs.is_empty(),
        "no dataset-loaded line: {:?}",
        loaded.logs
    );
}

#[test]
fn loader_reads_json_datasets_in_both_formats_and_skips_other_files() {
    let dir = TempDir::new("loader-formats");
    // A bare array and a wrapped dataset.
    dir.write("datasets/bare.json", &format!("[{}]", full_record(100, "")));
    dir.write(
        "datasets/wrapped.json",
        &format!(r#"{{"name": "set", "data": [{}]}}"#, full_record(200, "")),
    );
    dir.write("datasets/notes.txt", &format!("[{}]", full_record(300, "")));
    dir.write("datasets/README.md", "not a dataset");

    let loaded = load(dir.path(), Vec::new(), false, None);

    let mut ids: Vec<u32> = loaded.catalog.iter().map(|e| e.norad_id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![100, 200], "only the .json files are read");
    assert_eq!(
        loaded.logs,
        vec![(
            LogLevel::Info,
            "propagation",
            "Dataset loaded: 2 objects".to_string()
        )]
    );
    assert!(loaded.gpu.is_none(), "no buffer resource, none created");
}

#[test]
fn loader_also_reads_the_projects_datasets_before_the_assets() {
    let assets = TempDir::new("loader-project-assets");
    let project = TempDir::new("loader-project-folder");
    assets.write(
        "datasets/shipped.json",
        &format!("[{}]", full_record(100, "")),
    );
    project.write(
        "datasets/b_mine.json",
        &format!("[{}]", full_record(300, "")),
    );
    project.write(
        "datasets/a_mine.json",
        &format!("[{}]", full_record(200, "")),
    );
    project.write("datasets/notes.txt", &format!("[{}]", full_record(400, "")));
    // A plan next to the datasets folder is not a dataset.
    project.write("plan.json", "[]");

    let loaded = load_in_project(assets.path(), Some(project.path()), Vec::new(), true, None);

    // The gpu buffer keeps load order: the project's files by name, then the assets'.
    let order: Vec<u32> = loaded.gpu.unwrap().iter().map(|g| g.id).collect();
    assert_eq!(order, vec![200, 300, 100]);
    let mut ids: Vec<u32> = loaded.catalog.iter().map(|e| e.norad_id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![100, 200, 300]);
    assert_eq!(
        loaded.logs,
        vec![(
            LogLevel::Info,
            "propagation",
            "Dataset loaded: 3 objects".to_string()
        )]
    );
}

#[test]
fn loader_reads_the_projects_datasets_when_the_assets_have_none() {
    let assets = TempDir::new("loader-project-only-assets");
    let project = TempDir::new("loader-project-only");
    project.write("datasets/mine.json", &format!("[{}]", full_record(200, "")));

    let loaded = load_in_project(assets.path(), Some(project.path()), Vec::new(), false, None);

    let ids: Vec<u32> = loaded.catalog.iter().map(|e| e.norad_id).collect();
    assert_eq!(
        ids,
        vec![200],
        "no assets/datasets folder: the project's file still loads"
    );
}

#[test]
fn loader_ignores_a_project_without_a_datasets_folder() {
    let assets = TempDir::new("loader-project-none-assets");
    let project = TempDir::new("loader-project-none");
    assets.write(
        "datasets/shipped.json",
        &format!("[{}]", full_record(100, "")),
    );

    let loaded = load_in_project(assets.path(), Some(project.path()), Vec::new(), false, None);

    assert_eq!(loaded.catalog.len(), 1);
}

#[test]
fn loader_fills_the_catalog_entry_names_and_search_blob() {
    let dir = TempDir::new("loader-names");
    let records = [
        // name and id
        r#"{"OBJECT_NAME": "Zeta SAT", "OBJECT_ID": "2000-001A", "NORAD_CAT_ID": 11}"#,
        // no name: the id names it
        r#"{"OBJECT_ID": "2000-002B", "NORAD_CAT_ID": 12}"#,
        // empty name: the id names it
        r#"{"OBJECT_NAME": "", "OBJECT_ID": "2000-003C", "NORAD_CAT_ID": 13}"#,
        // neither: "NORAD n"
        r#"{"NORAD_CAT_ID": 14}"#,
        // name only: empty object id
        r#"{"OBJECT_NAME": "Alpha", "OBJECT_ID": "", "NORAD_CAT_ID": 15}"#,
    ];
    let body: Vec<String> = records
        .iter()
        .map(|r| {
            // add the elements every record needs
            r.replacen(
                '{',
                r#"{"MEAN_MOTION": 15.5, "ECCENTRICITY": 0.001, "INCLINATION": 51.6, "RA_OF_ASC_NODE": 120.0, "ARG_OF_PERICENTER": 90.0, "MEAN_ANOMALY": 45.0, "#,
                1,
            )
        })
        .collect();
    dir.write("datasets/names.json", &format!("[{}]", body.join(",")));

    let loaded = load(dir.path(), Vec::new(), true, None);

    // Sorted by display name; `gpu_index` is the load order and the buffer is not
    // sorted, so the index still points at the entry's own elements.
    let names: Vec<(&str, &str, &str, usize)> = loaded
        .catalog
        .iter()
        .map(|e| {
            (
                e.object_name.as_str(),
                e.object_id.as_str(),
                e.search_blob.as_str(),
                e.gpu_index,
            )
        })
        .collect();
    assert_eq!(
        names,
        vec![
            ("2000-002B", "2000-002B", "2000-002b 2000-002b 12", 1),
            ("2000-003C", "2000-003C", "2000-003c 2000-003c 13", 2),
            ("Alpha", "", "alpha  15", 4),
            ("NORAD 14", "", "norad 14  14", 3),
            ("Zeta SAT", "2000-001A", "zeta sat 2000-001a 11", 0),
        ]
    );
    let buffer = loaded.gpu.expect("buffer");
    assert_eq!(buffer.len(), 5);
    for entry in &loaded.catalog {
        assert_eq!(
            buffer[entry.gpu_index].id, entry.norad_id,
            "gpu_index {} points at {}'s elements",
            entry.gpu_index, entry.norad_id
        );
    }
}

#[test]
fn loader_epoch_origin_is_set_once_and_offsets_are_against_it() {
    let dir = TempDir::new("loader-origin");
    let late =
        full_record(1, "").replace("2026-04-03T16:03:47.002464", "2026-04-03T16:05:47.002464");
    let no_epoch = full_record(2, "").replace(r#""EPOCH": "2026-04-03T16:03:47.002464","#, "");
    dir.write("datasets/o.json", &format!("[{late},{no_epoch}]"));

    // No origin yet: the sim clock's epoch becomes the origin.
    let loaded = load(dir.path(), Vec::new(), true, Some(None));
    assert_eq!(loaded.origin, Some(Some(fixed_epoch())));
    let buffer = loaded.gpu.unwrap();
    assert_close(
        buffer[0].epoch_offset_seconds as f64,
        120.0,
        1e-3,
        "late record",
    );
    assert_eq!(buffer[1].epoch_offset_seconds, 0.0);

    // An origin that is already set is kept, and offsets are against it.
    let earlier = fixed_epoch() - 1_000.0;
    let loaded = load(dir.path(), Vec::new(), true, Some(Some(earlier)));
    assert_eq!(loaded.origin, Some(Some(earlier)));
    let buffer = loaded.gpu.unwrap();
    assert_close(
        buffer[0].epoch_offset_seconds as f64,
        1_120.0,
        1e-2,
        "late record",
    );
    assert_close(
        buffer[1].epoch_offset_seconds as f64,
        0.0,
        0.0,
        "record without epoch",
    );
}

#[test]
fn loader_logs_an_error_for_a_dataset_that_does_not_parse() {
    let dir = TempDir::new("loader-bad-json");
    dir.write("datasets/broken.json", "{ this is not json");
    // The loader logs the path `read_dir` yields: components joined with the
    // platform separator (`dir.write("datasets/...")` would mix `\` and `/` on Windows).
    let path = dir.path().join("datasets").join("broken.json");
    dir.write("datasets/good.json", &format!("[{}]", full_record(5, "")));

    let loaded = load(dir.path(), Vec::new(), false, None);

    assert_eq!(loaded.catalog.len(), 1, "the good file still loads");
    assert!(loaded.logs.contains(&(
        LogLevel::Error,
        "dataset",
        format!("Failed to parse dataset JSON: {}", path.display())
    )));
    assert!(loaded.logs.contains(&(
        LogLevel::Info,
        "propagation",
        "Dataset loaded: 1 objects".to_string()
    )));
}

#[test]
fn loader_names_the_first_missing_field_of_a_rejected_record() {
    let dir = TempDir::new("loader-missing");
    let all = [
        ("NORAD_CAT_ID", "99"),
        ("MEAN_MOTION", "15.5"),
        ("ECCENTRICITY", "0.001"),
        ("INCLINATION", "51.6"),
        ("RA_OF_ASC_NODE", "120.0"),
        ("ARG_OF_PERICENTER", "90.0"),
        ("MEAN_ANOMALY", "45.0"),
    ];
    // One dataset per case, so the log lines are in a known order per run.
    let mut expected = Vec::new();
    for (missing, _) in all {
        let fields: Vec<String> = all
            .iter()
            .filter(|(name, _)| *name != missing)
            .map(|(name, value)| format!("\"{name}\": {value}"))
            .collect();
        let sub = TempDir::new(&format!("loader-missing-{missing}"));
        sub.write("datasets/one.json", &format!("[{{{}}}]", fields.join(", ")));
        let loaded = load(sub.path(), Vec::new(), false, None);
        assert!(
            loaded.catalog.is_empty(),
            "{missing}: the record is skipped"
        );
        expected.push((missing, loaded.logs));
    }
    let message = |name: &str| -> (LogLevel, &'static str, String) {
        match name {
            "NORAD_CAT_ID" => (
                LogLevel::Error,
                "dataset",
                "Failed to parse norad_cat_id for an object".to_string(),
            ),
            "MEAN_MOTION" => (
                LogLevel::Error,
                "dataset",
                "Failed to parse mean_motion for object Number(99)".to_string(),
            ),
            other => {
                let field = match other {
                    "ECCENTRICITY" => "eccentricity",
                    "INCLINATION" => "inclination",
                    "RA_OF_ASC_NODE" => "ra_of_asc_node",
                    "ARG_OF_PERICENTER" => "arg_of_pericenter",
                    _ => "mean_anomaly",
                };
                (
                    LogLevel::Error,
                    "propagation",
                    format!("Failed to parse {field} for object 99"),
                )
            }
        }
    };
    for (missing, logs) in expected {
        assert_eq!(logs, vec![message(missing)], "missing {missing}");
    }
    drop(dir);
}

#[test]
fn loader_skips_a_record_with_a_non_numeric_field_without_a_log_line() {
    // A field that is present but not a number: skipped, and nothing says why.
    let dir = TempDir::new("loader-non-numeric");
    for bad in [
        "NORAD_CAT_ID",
        "MEAN_MOTION",
        "ECCENTRICITY",
        "MEAN_ANOMALY",
    ] {
        let record = full_record(99, "").replace(
            &format!("\"{bad}\": "),
            &format!("\"{bad}\": \"text\", \"X_{bad}\": "),
        );
        dir.write("datasets/one.json", &format!("[{record}]"));
        let loaded = load(dir.path(), Vec::new(), false, None);
        assert!(loaded.catalog.is_empty(), "{bad}: skipped");
        assert!(
            loaded.logs.is_empty(),
            "{bad}: no log line, got {:?}",
            loaded.logs
        );
    }
}

#[test]
fn loader_reads_the_repository_dataset() {
    // The shipped dataset: every entry comes from one record, the catalog is sorted
    // by display name and the count is what the loader reports.
    let loaded = load(&repo_root().join("assets"), Vec::new(), true, Some(None));
    assert!(
        loaded.catalog.len() > 1_000,
        "{} entries",
        loaded.catalog.len()
    );
    assert_eq!(loaded.gpu.as_ref().unwrap().len(), loaded.catalog.len());
    assert!(
        loaded
            .catalog
            .windows(2)
            .all(|w| w[0].display_name() <= w[1].display_name()),
        "sorted by display name"
    );
    assert_eq!(
        loaded.logs.last(),
        Some(&(
            LogLevel::Info,
            "propagation",
            format!("Dataset loaded: {} objects", loaded.catalog.len())
        ))
    );
    // The Hubble record of the Q13 fixture is in it.
    assert!(
        loaded
            .catalog
            .iter()
            .any(|e| e.norad_id == q13::HST_NORAD_ID as u32 && e.object_name.contains("HST")),
        "HST is in the dataset"
    );
}
