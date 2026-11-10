---
sidebar_position: 8
---

# Determinism

The simulation is deterministic per build and platform. This page states what that covers, what changes a result, and how results compare across platforms.

## The contract

**Same build, same platform, same project and the same input sequence give bit-identical results:** the same `trajectory.csv`, byte for byte, and the same `state_hash` in `summary.json`.

The reference goldens hold on macOS/aarch64; CI on every platform checks run-to-run identity instead. They come from the reference scenario (`scenario_q13`: the Hubble Space Telescope as the target, a chaser placed near it, a fixed epoch, the example plan): `rendezvous → approach` at step 6,005, `approach → final_approach` at 31,281, `final_approach → terminal` at 68,707 and `terminal → capture` at 191,125 with a range of 4.999367 m, and a 6,400-step `state_hash` of `5496fcd1fb51cb11`. No tether body touches the debris before the capture phase; in the capture phase the rope wraps the bus.

## Across platforms

**Results are not bit-identical across platforms.** Linux, macOS and Windows each reproduce their own results exactly, but they differ from each other:

| What | macOS (aarch64) | Linux (x86_64) | Windows (x86_64) |
|---|---|---|---|
| First transition (`rendezvous → approach`), step | 4,318 | 4,315 | 4,315 |

These were measured on an earlier version of the reference scenario, before the rope tether: the first transition landed within 3 steps across the three platforms, and the range at the same step differed by up to 5.7 m over the first 6,400 steps (100 s of simulation time). With the rope, contacts amplify a last-bit difference sooner, so a contact-rich run diverges between platforms earlier than an orbit alone. The cause is the platform math library (see *Known divergence sources* below).

What this means for you:

- **Recordings play back the same everywhere.** Playback reads the recorded frames; it does not simulate.
- **Re-simulation is per platform.** `ssg-sim replay` re-simulates a recording and compares every frame. On the machine and build that recorded it, every frame matches. On another platform or build it reports the first and the largest difference and exits with code 3 ([Recordings](./recordings)).
- **Projects are portable, results are not bit-for-bit.** The same project runs on every platform; expect the trajectory to differ in the last bits at first and by metres later in a contact-rich run.

Bit-identical results across platforms are planned work after v0.4.0.

## What does not change the result

- **Frame rate and wall-clock time.** The simulation advances in fixed steps of 1/64 s. Each step advances the epoch by 1/64 s and runs one physics step, whatever the frame rate.
- **Time warp.** The warp sets how many fixed steps one update runs. Results depend only on the sequence of fixed steps: a run at 16× gives the same `trajectory.csv` as a run at 1×.
- **The warp cap.** At most a capped number of fixed steps run per update: 64 on the command line; in the app the cap adapts to the machine, from 8 to 512 ([Time warp](./time-warp#requested-and-effective-warp)). The cap changes how fast the simulation runs, not which steps it runs.
- **Pause.** Pausing stops the fixed steps; resuming continues from the same state.
- **Camera cycling.** The camera target is a viewing choice. The floating origin and the physics bubble follow the simulation anchor (the tether root).
- **Run-to-run timing of threads.** The parallel parts of the step are order-independent (see below).

## What does change the result

- **The number of fixed steps in the update that creates the bodies.** Those steps run before the orbits start, and they move the tether, which is not at rest when it is created. The command line always runs 1. The app runs 0 to 3 (more when the warp was raised before Start), depending on the frame timing at the click. A recording stores this number so that it can be re-simulated ([Recordings](./recordings#the-spawn-update-step-count)).

- **The start epoch.** Catalog objects start from an SGP4 state at the start epoch, so a different epoch gives a different geometry. The tether's orientation at spawn also depends on it: `toward_target` lays the rope along the chaser-to-target line at that epoch ([Tether System](./tether-system#orientation-at-spawn)). `--epoch now` (the CLI default) and the desktop app read the wall clock once for it; use a fixed `--epoch` for repeatable runs.
- **The capture plan** and **the catalog records** (the GP elements and their epochs).
- **Dependency versions:** Bevy (fork), avian3d, parry, brahe, nalgebra, as locked in `Cargo.lock`, and the Rust toolchain in `rust-toolchain.toml`.
- **The platform:** CPU architecture, operating system and math library. See [Across platforms](#across-platforms).

## Levers and their status

| Lever | Status | Why |
|---|---|---|
| avian `enhanced-determinism` | off | Measured on all three platforms: it changes every platform's numbers but makes no two platforms agree (the simulation still calls the platform math library outside avian, see below). It costs nothing measurable (+0.4 % wall time on macOS). |
| avian `simd` | on | Mutually exclusive with `enhanced-determinism` in parry3d 0.26. Removing it on its own was not measured; it would matter only if `enhanced-determinism` had reached identity. |
| avian `parallel` | on | Results are bit-identical with and without it on all three platforms (measured with `enhanced-determinism`); without it a run takes about 4 % longer. |
| `par_iter` in the simulation | order-independent | `cache_eci_states` (`orbit/mod.rs`) collects into a map keyed by entity under a mutex; the floating-origin shift (`floating_origin.rs`) writes each entity's own components. Neither depends on the order of the threads. |
| Hash maps | deterministic | Simulation maps are `bevy::platform::collections::HashMap` with a fixed hasher (fixed seed). The two `std` hash sets in `capture/plan.rs` only validate a plan and never order a result. |
| `lto = "fat"` (release) | no effect | Link-time optimisation in Rust does not relax floating-point semantics (no fast-math). The release build reproduces the dev-profile goldens bit for bit. |
| Earth orientation parameters | fixed | The simulation installs brahe's zero EOP provider (offline, deterministic) and never downloads EOP data. |

## How to verify

Run the same scenario twice and compare the `state_hash` lines:

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

A [recording](./recordings) is played back from its files, so **playback is identical on every platform**. `ssg-sim replay` re-simulates a recording from step 0 with its scenario, plan and input log, including the number of steps of the spawn update, and compares every frame bit for bit. On the machine and the build that recorded it, a recording re-simulates bit for bit, whether `ssg-sim run --record` or the app made it: the simulation owns the debris collider and its mass properties, so the command line and the app simulate the same world, contacts included. An app recording of the Hubble capture, through the wrap and the hold, replays with every frame identical. On another machine, platform or build, re-simulation may differ, and `ssg-sim replay` reports by how much (exit 3, or exit 1 when the target and version are the same but the C library or toolchain differs).

## Tests

| Test | What it checks | Where it runs |
|---|---|---|
| `cli_two_runs_bit_identical` | two CLI runs of 6,400 steps: `trajectory.csv` bytes and `summary.json` (without `wall_s`) equal | every CI run (Linux), `full-ci` on macOS and Windows |
| `cli_warp_16_equals_warp_1` | 6,400 steps at 1× and at 16×: `trajectory.csv`, transitions and `state_hash` equal | same |
| `cli_steps_must_be_multiple_of_warp` | `--steps` not a multiple of `--warp` is a usage error | same |
| `baseline_q13_fixed_epoch_short` | the reference scenario for 6,400 steps, twice in one process: bit-identical | same |
| `cli_three_runs_to_capture_identical` (ignored) | the scenario to `capture`: three runs at 1× and one at 16×, bit-identical | weekday schedule, manual CI runs and `full-ci` PRs (the release PR carries it), on Linux, macOS and Windows; on demand: `cargo test -p ssg-sim-cli --locked cli_three_runs_to_capture_identical -- --ignored --nocapture` |
| `baseline_q13_to_capture` (ignored) | two in-process runs to `capture`, bit-identical, and the macOS/aarch64 goldens | same runs, **macOS only** (the goldens hold on macOS/aarch64 only); on demand: `cargo test -p ssg-sim --locked baseline_q13_to_capture -- --ignored --nocapture` |
| `tether_to_capture_contacts`, `tether_rest_ten_minutes` (ignored) | the scenario to `capture` with no contact of the tether's ends with the debris, and a 10-minute rest of the rope in R-bar | weekday schedule, manual CI runs and `full-ci` PRs, on Linux, macOS and Windows; on demand: `cargo test -p ssg-sim --locked -- --ignored --nocapture tether_to_capture_contacts tether_rest_ten_minutes` |
| The eight capture behaviour tests (ignored) | what the walk sees, from the rendezvous to the hold: a taut rope, no overshoot, the ends apart and on the sphere, a clean orbit, the rope flung out, the wrap on the bus, the ends at rest in the hold | same runs; on demand: `cargo test -p ssg-sim --locked -- --ignored --nocapture capture_behaviour` |

## Known divergence sources

- **The platform math library.** brahe's Kepler propagation (every step: the orbital cache, the origin of the relative gravity, the LVLH frame) and the SGP4 start states call `sin`, `cos` and friends from the platform's math library. Apple's libm, glibc and the Microsoft C runtime may round them differently in the last bit. No avian feature changes this.
- **Amplification.** The capture controller applies full-force pushes in the early phases and the rope touches itself and the debris, so a last-bit difference grows to metres within seconds: the first sample, 10 s into the run, already differs by up to 2.9 m between platforms.
- **Dependency and toolchain updates.** A new version of Bevy, avian, parry, brahe or the Rust toolchain may change results on every platform; the goldens are then re-measured.
- **`--epoch now`.** Every run starts at a different epoch, so the start geometry differs.
