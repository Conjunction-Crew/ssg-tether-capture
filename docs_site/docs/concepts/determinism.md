---
sidebar_position: 8
---

# Determinism

:::note Draft
This page states the determinism contract measured in P4 (v0.3.2). It is a draft; the full documentation pass comes later.
:::

## The contract

**Same build, same platform, same project and the same input sequence give bit-identical results:** the same `trajectory.csv`, byte for byte, and the same `state_hash` in `summary.json`.

**Across platforms:** results are **not** bit-identical. Linux, macOS and Windows each reproduce their own results exactly, but they differ from each other. Measured in P4 on the Q13 reference scenario (fixed epoch, the shipped features): the first transition (`rendezvous → approach`) lands within **3 steps** across the three platforms (macOS/aarch64 step 4318; Linux/x86_64 and Windows/x86_64 step 4315), and the range at the same step differs by up to **5.7 m** over the first 6,400 steps (100 s of simulation time). Later transitions were measured on macOS only. The reference goldens therefore hold on macOS/aarch64 only; CI on every platform checks run-to-run identity instead.

## What does not change the result

- **Frame rate and wall-clock time.** The simulation advances in fixed steps of 1/64 s. Each step advances the epoch by 1/64 s and runs one physics step, whatever the frame rate.
- **Time warp.** The warp sets how many fixed steps one update runs. Results depend only on the sequence of fixed steps: a run at 16× gives the same `trajectory.csv` as a run at 1×.
- **The warp cap.** At most 64 fixed steps run per update. The cap changes how fast the simulation runs, not which steps it runs.
- **Pause.** Pausing stops the fixed steps; resuming continues from the same state.
- **Camera cycling.** The camera target is a viewing choice. The floating origin and the physics bubble follow the simulation anchor (the tether root).
- **Run-to-run timing of threads.** The parallel parts of the step are order-independent (see below).

## What does change the result

- **The start epoch.** Catalog objects start from an SGP4 state at the start epoch, so a different epoch gives a different geometry. `--epoch now` (the CLI default) and the desktop app read the wall clock once for it; use a fixed `--epoch` for repeatable runs.
- **The capture plan** and **the catalog records** (the GP elements and their epochs).
- **Dependency versions:** Bevy (fork), avian3d, parry, brahe, nalgebra, as locked in `Cargo.lock`, and the Rust toolchain in `rust-toolchain.toml`.
- **The platform:** CPU architecture, operating system and math library. See *Across platforms* above.

## Levers and their status

| Lever | Status | Why |
|---|---|---|
| avian `enhanced-determinism` | off | Measured in P4 on all three platforms: it changes every platform's numbers but makes no two platforms agree (the simulation still calls the platform math library outside avian, see below). It costs nothing measurable (+0.4 % wall time on macOS). |
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

## Tests

| Test | What it checks | Where it runs |
|---|---|---|
| `cli_two_runs_bit_identical` | two CLI runs of 6,400 steps: `trajectory.csv` bytes and `summary.json` (without `wall_s`) equal | every CI run (Linux), `full-ci` on macOS and Windows |
| `cli_warp_16_equals_warp_1` | 6,400 steps at 1× and at 16×: `trajectory.csv`, transitions and `state_hash` equal | same |
| `cli_steps_must_be_multiple_of_warp` | `--steps` not a multiple of `--warp` is a usage error | same |
| `baseline_q13_fixed_epoch_short` | the Q13 scenario for 6,400 steps, twice in one process: bit-identical | same |
| `cli_three_runs_to_capture_identical` (ignored) | the scenario to `capture`: three runs at 1× and one at 16×, bit-identical | on demand: `cargo test -p ssg-sim-cli --locked cli_three_runs_to_capture_identical -- --ignored --nocapture` |
| `baseline_q13_to_capture` (ignored) | two in-process runs to `capture`, bit-identical, and the macOS/aarch64 goldens | on demand: `cargo test -p ssg-sim --locked baseline_q13_to_capture -- --ignored --nocapture` |

## Known divergence sources

- **The platform math library.** brahe's Kepler propagation (every step: the orbital cache, the origin of the relative gravity, the LVLH frame) and the SGP4 start states call `sin`, `cos` and friends from the platform's math library. Apple's libm, glibc and the Microsoft C runtime may round them differently in the last bit. No avian feature changes this.
- **Amplification.** The capture controller applies normalised, saturated forces, so a last-bit difference grows to metres within seconds: the first sample, 10 s into the run, already differs by up to 2.9 m between platforms.
- **Dependency and toolchain updates.** A new version of Bevy, avian, parry, brahe or the Rust toolchain may change results on every platform; the goldens are then re-measured.
- **`--epoch now`.** Every run starts at a different epoch, so the start geometry differs.
