---
sidebar_position: 8
---

# Determinism

**The same version gives the same result on Linux, macOS and Windows, run to run and platform to platform, for the same inputs.** This page states what "the same result" covers, how it is achieved and kept, what changes a result, and what is not promised.

## Across platforms

Same version, same project and the same input sequence give bit-identical results, on any of the three platforms:

- **the same step count**: every state transition at the same fixed step;
- **the same state**: the same `trajectory.csv`, byte for byte, and the same `state_hash` in `summary.json`;
- **every recorded frame**: a recording made on one platform re-simulates bit for bit on the others (`ssg-sim replay` exits 0, see [Recordings](#recordings)).

The reference numbers ("goldens") are one set, asserted bit for bit on all three platforms. They come from the reference scenario (`scenario_q13`: the Hubble Space Telescope as the target, a chaser placed near it, a fixed epoch, the example plan): `rendezvous → approach` at step 6,005, `approach → final_approach` at 31,281, `final_approach → terminal` at 68,707 and `terminal → capture` at 191,125 with a range of 4.999367 m, and a 6,400-step `state_hash` of `5496fcd1fb51cb11`. No tether body touches the debris before the capture phase; in the capture phase the rope wraps the bus.

## How it is achieved

Each platform ships its own math library (Apple's libm, glibc, the Microsoft C runtime), and they may round `sin`, `cos`, `pow` and friends differently in the last bit. The rope's contacts amplify such a difference to metres within seconds. So no step of the simulation calls the platform's math library: every transcendental function comes from the [`libm`](https://crates.io/crates/libm) crate, plain Rust that gives the same bits everywhere. `sqrt`, `abs`, `floor` and the four basic operations are exact in IEEE 754 and give the same bits anyway.

| Layer | What computes it |
|---|---|
| The simulation's own math (LVLH frame, differential gravity, capture forces, catalog conversions) | `crate::math`, on `libm` |
| The Kepler step and the state ↔ orbital elements conversions | `orbit::kepler` (`TwoBody`), on `libm` |
| The start states from the catalog's GP records (SGP4) | `sgp4-portable`, a copy of the `sgp4` crate with its math on `libm` |
| The frame conversion TEME → GCRF (IAU 2006/2000A, zero Earth orientation parameters) | `orbit::frames`, on `libm` |
| The physics engine (avian3d, with parry, glam and nalgebra) | avian3d with `enhanced-determinism` on, `simd` off and `parallel` on: parry, glam and nalgebra's `simba` use `libm` |

The SGP4 code derives from the `sgp4` crate 2.3.0 (MIT) and the frame conversion from ERFA 2.0.1 (BSD-3-Clause); their licence notices are in [THIRD-PARTY-NOTICES.md](https://github.com/Conjunction-Crew/ssg-tether-capture/blob/main/THIRD-PARTY-NOTICES.md).

## How it is kept

- **A guard in CI** (`sh scripts/check_platform_math.sh`, in the Linux job of every pull request that changes the code): the library code may not call the `std` float math methods or the brahe functions that compute with them, may name only an allow-list of brahe items (time, constants, the export types), and the dependency features above must hold.
- **One set of goldens, asserted on three platforms**: the 6,400-step `state_hash` on every `full-ci` run, and the to-capture transitions, final range and state hash in the long tests (see [Tests](#tests)).
- **A committed recording**: `crates/ssg-sim-cli/tests/fixtures/q13-6400`, the reference scenario's first 6,400 steps recorded on macOS, re-simulated with every frame identical on every platform by a test build, and in `full-ci` also by a release build on each platform.

## What is not promised

- **Other versions or commits.** A new version may change any number; `ssg-sim replay` says so with exit code 3. Dependency updates (Bevy, avian, parry, brahe, nalgebra, as locked in `Cargo.lock`) and the Rust toolchain in `rust-toolchain.toml` count as a new version.
- **Other platforms.** CI tests x86-64 Linux, arm64 macOS and x86-64 Windows. Other platforms (Linux on arm64, Windows on arm64, macOS on Intel) are expected to agree, but are not tested.
- **Two residuals.** One copy of glam (0.31.1, inside avian's broad-phase tree) still uses the platform library; it measured identical on the three platforms all the way to capture. brahe's `Epoch` (time scales and Julian dates) stays; it does no transcendental math, and a known-answer test pins its results bit for bit on the three platforms.

## What does not change the result

- **The platform**, among the three above.
- **Frame rate and wall-clock time.** The simulation advances in fixed steps of 1/64 s. Each step advances the epoch by 1/64 s and runs one physics step, whatever the frame rate.
- **Time warp.** The warp sets how many fixed steps one update runs. Results depend only on the sequence of fixed steps: a run at 16× gives the same `trajectory.csv` as a run at 1×.
- **The warp cap.** At most a capped number of fixed steps run per update: 64 on the command line; in the app the cap adapts to the machine, from 8 to 512 ([Time warp](./time-warp#requested-and-effective-warp)). The cap changes how fast the simulation runs, not which steps it runs.
- **Pause.** Pausing stops the fixed steps; resuming continues from the same state.
- **Camera cycling.** The camera target is a viewing choice. The floating origin and the physics bubble follow the simulation anchor (the tether root).
- **Run-to-run timing of threads.** The parallel parts of the step are order-independent (see below).
- **Release or debug build.** A release build reproduces the dev-profile goldens bit for bit.

## What does change the result

- **The number of fixed steps in the update that creates the bodies.** Those steps run before the orbits start, and they move the tether, which is not at rest when it is created. The command line always runs 1. The app runs 0 to 3 (more when the warp was raised before Start), depending on the frame timing at the click. A recording stores this number so that it can be re-simulated ([Recordings](./recordings#the-spawn-update-step-count)).
- **The start epoch.** Catalog objects start from an SGP4 state at the start epoch, so a different epoch gives a different geometry. The tether's orientation at spawn also depends on it: `toward_target` lays the rope along the chaser-to-target line at that epoch ([Tether System](./tether-system#orientation-at-spawn)). `--epoch now` (the CLI default) and the desktop app read the wall clock once for it; use a fixed `--epoch` for repeatable runs.
- **The capture plan** and **the catalog records** (the GP elements and their epochs).
- **The version**: see [What is not promised](#what-is-not-promised).

## Levers and their status

| Lever | Status | Why |
|---|---|---|
| avian `enhanced-determinism` | on | Puts parry, glam and nalgebra's `simba` on `libm`. Measured: with it alone the platforms still differed, because the orbit code called the platform library; with the layers above on `libm` as well, they agree. Wall time to capture on macOS is within 1 % of the previous version. |
| avian `simd` | off | Mutually exclusive with `enhanced-determinism` in parry3d 0.26. |
| avian `parallel` | on | Results are bit-identical with and without it on all three platforms (measured with `enhanced-determinism`); without it a run takes about 4 % longer. |
| `par_iter` in the simulation | order-independent | `cache_eci_states` (`orbit/mod.rs`) collects into a map keyed by entity under a mutex; the floating-origin shift (`floating_origin.rs`) writes each entity's own components. Neither depends on the order of the threads. |
| Hash maps | deterministic | Simulation maps are `bevy::platform::collections::HashMap` with a fixed hasher (fixed seed). The two `std` hash sets in `capture/plan.rs` only validate a plan and never order a result. |
| `lto = "fat"` (release) | no effect | Link-time optimisation in Rust does not relax floating-point semantics (no fast-math). The release build reproduces the dev-profile goldens bit for bit. |
| Earth orientation parameters | fixed | The simulation installs brahe's zero EOP provider (offline, deterministic) and never downloads EOP data. |

## How to verify

Run the same scenario twice, on one machine or on two different platforms, and compare the `state_hash` lines:

```bash
ssg-sim run my-project --target 20580 --chaser 99901 \
  --dataset crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json \
  --epoch 2026-04-03T16:03:47.002464 --steps 163840 --out run-a
ssg-sim run my-project --target 20580 --chaser 99901 \
  --dataset crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json \
  --epoch 2026-04-03T16:03:47.002464 --steps 163840 --out run-b
grep state_hash run-a/summary.json run-b/summary.json
```

Add `--warp 16` to one of them: the hash stays the same.

## Recordings

A [recording](./recordings) is played back from its files, so **playback is identical on every platform**. `ssg-sim replay` re-simulates a recording from step 0 with its scenario, plan and input log, including the number of steps of the spawn update, and compares every frame bit for bit. A recording made by this version (and, when both name one, this commit) re-simulates bit for bit on Linux, macOS and Windows, whether `ssg-sim run --record` or the app made it: the simulation owns the debris collider and its mass properties, so the command line and the app simulate the same world, contacts included. A difference is then a defect (exit 1). A recording made by another version or commit may differ, and `ssg-sim replay` reports by how much (exit 3).

## Tests

| Test | What it checks | Where it runs |
|---|---|---|
| `cli_two_runs_bit_identical` | two CLI runs of 6,400 steps: `trajectory.csv` bytes and `summary.json` (without `wall_s`) equal | every CI run (Linux), `full-ci` on macOS and Windows |
| `cli_warp_16_equals_warp_1` | 6,400 steps at 1× and at 16×: `trajectory.csv`, transitions and `state_hash` equal | same |
| `cli_steps_must_be_multiple_of_warp` | `--steps` not a multiple of `--warp` is a usage error | same |
| `baseline_q13_fixed_epoch_short` | the reference scenario for 6,400 steps, twice in one process: bit-identical | same |
| `a_6400_step_recording_has_the_design_layout_and_keeps_the_reference_hash` | the 6,400-step `state_hash` equals `Q13_6400_HASH` | same |
| `the_committed_recording_replays_on_every_platform` | the recording made on macOS re-simulates with every frame identical | same; in `full-ci` a release build replays it too, on each platform |
| `cli_three_runs_to_capture_identical` (ignored) | the scenario to `capture`: three runs at 1× and one at 16×, bit-identical, each with the state hash `CAPTURE_196608_HASH` | weekday schedule, manual CI runs and `full-ci` PRs (the release PR carries it), on Linux, macOS and Windows; on demand: `cargo test -p ssg-sim-cli --locked cli_three_runs_to_capture_identical -- --ignored --nocapture` |
| `baseline_q13_to_capture` (ignored) | two in-process runs to `capture`, bit-identical, and the goldens (transition steps, final range) bit for bit | same runs, on Linux, macOS and Windows; on demand: `cargo test -p ssg-sim --locked baseline_q13_to_capture -- --ignored --nocapture` |
| `tether_to_capture_contacts`, `tether_rest_ten_minutes` (ignored) | the scenario to `capture` with no contact of the tether's ends with the debris, and a 10-minute rest of the rope in R-bar | same runs; on demand: `cargo test -p ssg-sim --locked -- --ignored --nocapture tether_to_capture_contacts tether_rest_ten_minutes` |
| The eight capture behaviour tests (ignored) | what the walk sees, from the rendezvous to the hold: a taut rope, no overshoot, the ends apart and on the sphere, a clean orbit, the rope flung out, the wrap on the bus, the ends at rest in the hold | same runs; on demand: `cargo test -p ssg-sim --locked -- --ignored --nocapture capture_behaviour` |
| `f5_what_stays_with_brahes_epoch_known_answer_bits` | brahe's `Epoch` times at fixed epochs, bit for bit | every CI run (Linux), `full-ci` on macOS and Windows |
