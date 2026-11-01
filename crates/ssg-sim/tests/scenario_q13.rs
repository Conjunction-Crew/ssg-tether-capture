//! The Q13 scenario (HST and the P0 chaser) through the public API and
//! `ssg_sim::test_support::q13::run` (P5 S7).
//!
//! Platform-independent: it checks the transition order and the direction of the
//! range, not the macOS goldens (those are `baseline_q13_to_capture`). Same-process
//! identity of two runs is `baseline_q13_fixed_epoch_short`, so it is not repeated.

use ssg_sim::test_support::q13;

const TICKS: u64 = 6_400;
/// The harness ticks once more than the CLI before the first sample (F-51), so
/// the first checkpoint, after 640 ticks, is at step 643.
const FIRST_CHECKPOINT_STEP: u64 = 643;
/// The last checkpoint at which the Q13 chaser is still in `rendezvous` on every
/// platform (the first transition fires at step 4,745 on Linux, 4,769 on macOS).
const LAST_RENDEZVOUS_CHECKPOINT_STEP: u64 = 3_843;

#[test]
fn q13_runs_through_the_first_transition_with_finite_states_and_a_closing_range() {
    let run = q13::run(TICKS, false);

    assert_eq!(run.ticks, TICKS, "the whole horizon ran");
    let order: Vec<(&str, &str)> = run
        .transitions
        .iter()
        .map(|(_, from, to)| (from.as_str(), to.as_str()))
        .collect();
    assert_eq!(order, [("rendezvous", "approach")]);
    assert!(
        (4_730..4_785).contains(&run.transitions[0].0), // F-66 (P7.5 S1): provisional until S4
        "the first transition fires near step 4,769 (4,745 on Linux): {:?}",
        run.transitions
    );

    assert_eq!(run.checkpoints.len(), 10, "a checkpoint every 640 ticks");
    for sample in run.checkpoints.iter().chain([&run.final_sample]) {
        assert!(
            sample.range_m.is_finite() && sample.range_m > 0.0,
            "range at step {}: {}",
            sample.step,
            sample.range_m
        );
    }
    assert!(run.final_root.is_finite(), "root {:?}", run.final_root);
    assert!(
        run.final_debris.is_finite(),
        "debris {:?}",
        run.final_debris
    );
    assert_eq!(run.final_sample.step, TICKS + 3, "3 steps of set-up (F-51)");
    assert_eq!(run.final_sample.state, "approach");

    // The range closes in during rendezvous (one run serves both checks: 6,400
    // ticks cost about 3 s in the dev profile).
    let rendezvous: Vec<_> = run
        .checkpoints
        .iter()
        .filter(|s| s.step <= LAST_RENDEZVOUS_CHECKPOINT_STEP)
        .collect();
    assert_eq!(rendezvous.len(), 6, "checkpoints 643, 1283, ..., 3843");
    assert_eq!(rendezvous[0].step, FIRST_CHECKPOINT_STEP);
    assert_eq!(
        rendezvous.last().unwrap().step,
        LAST_RENDEZVOUS_CHECKPOINT_STEP
    );
    for pair in rendezvous.windows(2) {
        assert!(
            pair[1].range_m < pair[0].range_m,
            "the range did not decrease from step {} ({} m) to step {} ({} m)",
            pair[0].step,
            pair[0].range_m,
            pair[1].step,
            pair[1].range_m
        );
        assert_eq!(pair[0].state, "rendezvous");
    }
    // The approach threshold of the example plan is 2,000 m.
    assert!(
        rendezvous[0].range_m > 3_000.0 && rendezvous.last().unwrap().range_m > 2_000.0,
        "ranges stay above the approach threshold: {:?}",
        rendezvous
    );
}
