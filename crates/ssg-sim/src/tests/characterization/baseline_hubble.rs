//! Q13 Hubble (HST) capture baseline (P2 S4). The scenario lives in
//! `test_support::q13`; the measurement and P3's tolerance are in
//! plans/stage-2/52-p2-testing-baseline.md.

use crate::orbit::catalog::{GpuComputeEpochOrigin, GpuElements, SpaceObjectCatalog};
use crate::orbit::selection::EditableOrbitalElements;
use crate::test_support::q13::{
    self, CHASER_NORAD_ID, EpochMode, HST_NORAD_ID, Q13Run, app_elements, fixture_record,
    gpu_to_editable,
};
use crate::test_support::{assert_close, enter_sim, fixed_epoch, repo_root, test_app};

fn element_bits(elements: &EditableOrbitalElements) -> [u64; 7] {
    [
        elements.semi_major_axis_m.to_bits(),
        elements.eccentricity.to_bits(),
        elements.inclination_rad.to_bits(),
        elements.raan_rad.to_bits(),
        elements.arg_perigee_rad.to_bits(),
        elements.mean_anomaly_rad.to_bits(),
        (elements.epoch - fixed_epoch()).to_bits(),
    ]
}

/// Same-process determinism: two runs agree bit for bit (F-17 evidence).
#[track_caller]
fn assert_runs_identical(a: &Q13Run, b: &Q13Run, what: &str) {
    assert_eq!(a.ticks, b.ticks, "{what}: tick count");
    assert_eq!(a.transitions, b.transitions, "{what}: transitions");
    assert_eq!(
        a.checkpoints.len(),
        b.checkpoints.len(),
        "{what}: checkpoint count"
    );
    for (i, (x, y)) in a.checkpoints.iter().zip(&b.checkpoints).enumerate() {
        assert_eq!(
            (x.step, &x.state, x.range_m.to_bits()),
            (y.step, &y.state, y.range_m.to_bits()),
            "{what}: checkpoint {i}"
        );
    }
    assert_eq!(
        (
            a.final_sample.step,
            &a.final_sample.state,
            a.final_sample.range_m.to_bits()
        ),
        (
            b.final_sample.step,
            &b.final_sample.state,
            b.final_sample.range_m.to_bits()
        ),
        "{what}: final sample"
    );
    for (name, x, y) in [
        ("root", a.final_root, b.final_root),
        ("debris", a.final_debris, b.final_debris),
    ] {
        assert_eq!(
            x.to_array().map(f64::to_bits),
            y.to_array().map(f64::to_bits),
            "{what}: final {name} Position"
        );
    }
}

#[test]
fn baseline_q13_elements_match_app_loader() {
    let mut app = test_app();
    app.insert_resource(GpuElements::default());
    app.insert_resource(GpuComputeEpochOrigin(None));
    enter_sim(&mut app);

    // Precondition: the loader ran and took its reference epoch from the fixed
    // `WorldTime`, the reference `app_elements` uses.
    assert_eq!(
        app.world().resource::<GpuComputeEpochOrigin>().0,
        Some(fixed_epoch())
    );
    let hst: Vec<_> = app
        .world()
        .resource::<SpaceObjectCatalog>()
        .entries
        .iter()
        .filter(|entry| u64::from(entry.norad_id) == HST_NORAD_ID)
        .collect();
    assert_eq!(hst.len(), 1, "HST appears once in the catalog");
    let gpu = app.world().resource::<GpuElements>().0[hst[0].gpu_index];
    assert_eq!(u64::from(gpu.id), HST_NORAD_ID);

    // F-27: current behaviour, expected to change in P4
    assert_eq!(
        element_bits(&app_elements(&fixture_record(HST_NORAD_ID))),
        element_bits(&gpu_to_editable(&gpu)),
        "app_elements(HST) vs the loaded catalog entry"
    );

    // The fixture's HST record is the dataset's, verbatim.
    let dataset: Vec<serde_json::Value> = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("assets/datasets/dataset_2026_04_03.json"))
            .expect("dataset is readable"),
    )
    .expect("dataset parses");
    let fixture: Vec<serde_json::Value> =
        serde_json::from_str(q13::FIXTURE).expect("fixture parses");
    let norad = |record: &serde_json::Value| record["NORAD_CAT_ID"].as_u64();
    let dataset_hst: Vec<_> = dataset
        .iter()
        .filter(|record| norad(record) == Some(HST_NORAD_ID))
        .collect();
    assert_eq!(dataset_hst.len(), 1, "HST appears once in the dataset");
    assert_eq!(fixture.len(), 2);
    assert_eq!(&fixture[0], dataset_hst[0], "fixture HST record");
    assert_eq!(norad(&fixture[1]), Some(CHASER_NORAD_ID));
}

const SHORT_TICKS: u64 = 6_400;
/// The transition order every platform logs in the first 6,400 ticks.
const SHORT_ORDER: &[(&str, &str)] = &[("rendezvous", "approach")];

// Golden values captured at 3369a3a on macos/aarch64 (P4 S4: relative dynamics and LVLH velocity, F-09/F-35)
// They hold on macOS/aarch64 only: Linux and Windows CI reach `approach` 5-6 steps
// later (F-17), so only `baseline_q13_to_capture` checks them (operator decision,
// 2026-10-26).
/// `(step, state, range_m)` every 640 ticks, the first 6,400 ticks of a
/// fixed-epoch run.
const SHORT_CHECKPOINTS: &[(u64, &str, f64)] = &[
    (643, "rendezvous", 3431.047008487971),
    (1_283, "rendezvous", 3184.419069275464),
    (1_923, "rendezvous", 2934.688970841489),
    (2_563, "rendezvous", 2684.60751014994),
    (3_203, "rendezvous", 2434.834817199054),
    (3_843, "rendezvous", 2185.184423735223),
    (4_483, "approach", 1938.1291044962209),
    (5_123, "approach", 1706.4027837179738),
    (5_763, "approach", 1501.845758355041),
    (6_403, "approach", 1325.6073130387854),
];
/// The fixed-epoch run to `capture`: every transition, and the final range.
const CAPTURE_TRANSITIONS: &[(u64, &str, &str)] = &[
    (4_318, "rendezvous", "approach"),
    (34_112, "approach", "terminal"),
    (156_811, "terminal", "capture"),
];
const CAPTURE_FINAL_RANGE_M: f64 = 4.999946993667034;
/// Baseline ranges carry 1e-6 m (A-3).
const RANGE_TOL_M: f64 = 1e-6;

fn transition_order(run: &Q13Run) -> Vec<(&str, &str)> {
    run.transitions
        .iter()
        .map(|(_, from, to)| (from.as_str(), to.as_str()))
        .collect()
}

/// CI guard on every platform: the fixed-epoch scenario for 6,400 ticks (100 s
/// of sim time, past the first transition), run twice in one process. It checks
/// same-process determinism and the transition order; the step indices and
/// ranges are platform-specific (F-17) and are checked by `baseline_q13_to_capture`.
#[test]
fn baseline_q13_fixed_epoch_short() {
    let first = q13::run(EpochMode::Fixed, SHORT_TICKS, false);
    let second = q13::run(EpochMode::Fixed, SHORT_TICKS, false);

    // Precondition: the whole horizon ran.
    assert_eq!(first.ticks, SHORT_TICKS);
    assert_runs_identical(&first, &second, "fixed epoch, 6,400 ticks");
    assert_eq!(transition_order(&first), SHORT_ORDER);
}

fn run_json(mode: EpochMode, index: usize, run: &Q13Run) -> serde_json::Value {
    serde_json::json!({
        "mode": mode.name(),
        "run": index,
        "transitions": run
            .transitions
            .iter()
            .map(|(step, from, to)| serde_json::json!({ "step": step, "from": from, "to": to }))
            .collect::<Vec<_>>(),
        "final": {
            "step": run.final_sample.step,
            "state": run.final_sample.state,
            "range_m": run.final_sample.range_m,
        },
        "ticks": run.ticks,
        "wall_s": run.wall.as_secs_f64(),
    })
}

/// Per transition: `(from, to, min step, max step)`; plus the final range's
/// `(min, max)`. Assumes every run logged the same transition order.
fn spread(runs: &[Q13Run]) -> serde_json::Value {
    let order = &runs[0].transitions;
    let transitions: Vec<_> = order
        .iter()
        .enumerate()
        .map(|(i, (_, from, to))| {
            let steps: Vec<u64> = runs.iter().map(|run| run.transitions[i].0).collect();
            let (min, max) = (*steps.iter().min().unwrap(), *steps.iter().max().unwrap());
            serde_json::json!({ "from": from, "to": to, "min": min, "max": max, "spread": max - min })
        })
        .collect();
    let ranges = runs.iter().map(|run| run.final_sample.range_m);
    let min = ranges.clone().fold(f64::INFINITY, f64::min);
    let max = ranges.fold(f64::NEG_INFINITY, f64::max);
    serde_json::json!({
        "runs": runs.len(),
        "same_order": runs.iter().all(|run| {
            run.transitions.len() == order.len()
                && run.transitions.iter().zip(order).all(|(a, b)| (&a.1, &a.2) == (&b.1, &b.2))
        }),
        "transitions": transitions,
        "final_range_m": { "min": min, "max": max, "spread": max - min },
    })
}

/// Opt-in measurement (E1): the scenario to `capture`, `P2_BASELINE_RUNS` times
/// on the app path (default 2, Q17) and twice with the fixed epoch. It also
/// checks the fixed-epoch goldens, which hold on macOS/aarch64 only.
#[test]
#[ignore = "P2/P3 baseline measurement (~5 min): cargo test --locked baseline_q13_to_capture -- --ignored --nocapture; see plans/stage-2/52-p2-testing-baseline.md"]
fn baseline_q13_to_capture() {
    const MAX_TICKS: u64 = 200_000;
    let app_runs: usize = std::env::var("P2_BASELINE_RUNS")
        .ok()
        .map(|runs| runs.parse().expect("P2_BASELINE_RUNS is a count"))
        .unwrap_or(2);

    let mut results = Vec::new();
    for (mode, count) in [(EpochMode::AppPath, app_runs), (EpochMode::Fixed, 2)] {
        let runs: Vec<Q13Run> = (0..count)
            .map(|index| {
                let run = q13::run(mode, MAX_TICKS, true);
                println!("{}", run_json(mode, index, &run));
                run
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "summary": mode.name(), "spread": spread(&runs) })
        );
        results.push((mode, runs));
    }

    for (mode, runs) in &results {
        for (index, run) in runs.iter().enumerate() {
            assert_eq!(
                run.final_sample.state,
                "capture",
                "{} run {index} did not reach capture in {MAX_TICKS} ticks",
                mode.name()
            );
        }
    }
    let fixed = &results[1].1;
    assert_runs_identical(&fixed[0], &fixed[1], "fixed epoch, to capture");

    // The macOS/aarch64 goldens (P3's exact fixed-epoch check, baseline doc §4).
    let run = &fixed[0];
    let transitions: Vec<(u64, &str, &str)> = run
        .transitions
        .iter()
        .map(|(step, from, to)| (*step, from.as_str(), to.as_str()))
        .collect();
    assert_eq!(transitions, CAPTURE_TRANSITIONS);
    assert_eq!(run.final_sample.step, CAPTURE_TRANSITIONS[2].0);
    assert_close(
        run.final_sample.range_m,
        CAPTURE_FINAL_RANGE_M,
        RANGE_TOL_M,
        "final range at capture",
    );
    for (sample, &(step, state, range_m)) in run.checkpoints.iter().zip(SHORT_CHECKPOINTS) {
        assert_eq!((sample.step, sample.state.as_str()), (step, state));
        assert_close(
            sample.range_m,
            range_m,
            RANGE_TOL_M,
            &format!("range at step {step}"),
        );
    }
}
