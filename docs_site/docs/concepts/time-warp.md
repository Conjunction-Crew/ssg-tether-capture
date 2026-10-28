---
sidebar_position: 4
---

# Time Warp

The `WorldTime` resource (`crates/ssg-sim/src/time.rs`) holds the simulation epoch and the time-warp multiplier.

## Start epoch

The simulation never reads the wall clock. Its start epoch is an explicit input, the `SimConfig` resource:

- the desktop app sets it at startup and again when a project is opened, so re-running a project in one session reuses the same epoch;
- the headless CLI sets it from `--epoch` (`now` is read once, before the run);
- the test harness uses a fixed epoch.

On entering the simulation, `setup_time` starts the clock at `SimConfig.start_epoch`, and every orbit's elements are taken at that epoch.

## The warp ladder

| Key | Effect |
|---|---|
| `.` (period) | Double the warp, up to 8192× (`MAX_TIME_WARP`); resume if paused |
| `,` (comma) | Halve the warp; at 1× it pauses the simulation |

Pause is a state (`PauseState::Paused`), not a 0× warp: the multiplier keeps its value and the simulation resumes at it.

## Requested and effective warp

The multiplier is the **requested** warp: the number of fixed steps (`1/64` s each) to run per `FixedUpdate` run. One run executes at most `MAX_STEPS_PER_UPDATE` (64) steps, so the app stays responsive; the number actually run is the **effective** warp. The HUD shows `8192x (effective 64x)` when the cap holds the warp back, and `Paused` when paused. No step debt is carried over, so lowering the warp takes effect at once. Bevy's catch-up window is lowered to three fixed steps, so one slow frame cannot multiply the work.

## Invariant

Each fixed step advances the epoch by `1/64` s and runs one physics step. Results depend only on the sequence of fixed steps and user inputs, never on the wall clock or the frame rate: 8 ticks at 1× and 1 tick at 8× give the same state.
