---
sidebar_position: 7
---

# Running without a window (`ssg-sim`)

`ssg-sim` runs a capture scenario headless: no window, no rendering, no UI. It uses the same simulation core as the app and writes the result to files. It is built from the `crates/ssg-sim-cli` crate:

```bash
cargo run --release -p ssg-sim-cli -- run <project-dir> --target <NORAD> --chaser <NORAD> [options]
cargo run --release -p ssg-sim-cli -- run <project-dir> [options]      # from <project-dir>/project.json
cargo run --release -p ssg-sim-cli -- validate <project-dir>
cargo run --release -p ssg-sim-cli -- replay <recording-dir>
```

## Usage

```text
usage: ssg-sim run <project-dir> [options]
       ssg-sim validate <project-dir> [--assets DIR]
       ssg-sim replay <recording-dir> [--assets DIR] [--frames-only]

run options:
  --plan ID            capture plan id (default: project.json's active_plan, else the only
                       plan in <project-dir>, else example_plan)
  --target NORAD       NORAD id of the capture target (required without <project-dir>/project.json;
                       overrides it otherwise)
  --chaser NORAD       NORAD id of the chaser (required without project.json; overrides it)
  --dataset FILE       GP JSON dataset to search for the ids (repeatable; searched after
                       <project-dir>/datasets/*.json and <assets>/datasets/*.json)
  --assets DIR         assets directory (default ./assets)
  --epoch now|ISO      start epoch: `now` (default without project.json, the app path) or
                       YYYY-MM-DDTHH:MM:SS[.fff][Z]; overrides project.json's start_epoch_utc
  --steps N            fixed steps to run after the capture starts (default 200000;
                       a multiple of --warp)
  --warp N             time warp: fixed steps per update (default 1, at most 64)
  --sample-every N     trajectory sample interval in fixed steps (default 64; a multiple
                       of --warp)
  --out DIR            output directory (default <project-dir>/runs/<utc-timestamp>)
  --stop-at-capture    stop at the end of the update in which the state becomes `capture`
  --record NAME        record the run to <project-dir>/recordings/NAME/ (letters, digits, `-`, `_`);
                       starts with the capture command, or when the capture enters
                       project.json's recording.auto_start_phase; ends with the run
  --record-from-phase ID
                       start the recording when the capture enters phase ID (overrides
                       recording.auto_start_phase; needs --record)

validate checks project.json, every plan file, the active plan, the orbits and every
recording under <project-dir>/recordings; it prints
`error:`, `warning:` and `info:` lines and exits 0 without errors, 1 with errors.

replay checks a recording's files and re-simulates it from step 0 with its inputs,
comparing every frame bit for bit (--frames-only: check the files only). Exit 0: every
frame identical; 1: a difference with the same target and version; 3: a difference with
another target or version (expected, F-48); 2: usage error or invalid recording.
```

A fixed step is 1/64 s of simulation time. `--steps` and `--sample-every` count fixed steps, whatever the warp, so runs at different warps sample the same steps and write the same `trajectory.csv`. One update runs `--warp` fixed steps (at most 64, the simulation's per-update cap). `--stop-at-capture` stops at the end of the update in which `capture` was reached, so at `--warp 16` the run may go on up to 15 steps past the capture step; the transition steps in `summary.json` are exact at any warp.

`<project-dir>` holds your own capture plans (`*.json`) and, optionally, a `project.json` and a `datasets/` folder. The example plans in `<assets>/example_capture_plans` are always loaded.

## Running from `project.json`

A project folder written by the app has a `project.json`: the start epoch, the active plan and the two bodies (a copy of their Space-Track record, or their orbital elements). With it, `--target`, `--chaser`, `--epoch` and `--plan` are optional:

```bash
ssg-sim run my-project --steps 6400
```

The run starts exactly from the file: the same epoch string, the same stored records, no dataset lookup. A flag you do give replaces its entry and says so on stderr (`ssg-sim: --epoch overrides project.json`). A `--target` or `--chaser` is looked up in the datasets as before and replaces only that body. Without a `project.json`, `--target` and `--chaser` are required, as before.

A manifest run and the flag run of the same scenario write byte-identical `trajectory.csv` files and equal `summary.json` files (except `wall_s`). When a body comes from an orbit without a NORAD id (elements), `summary.json` has `null` for its id.

## Recording a run

```bash
ssg-sim run my-project --steps 6400 --record r
```

writes the run as a [recording](../concepts/recordings) to `my-project/recordings/r/` (built in `r.partial/` and renamed when the run ends). The recording starts with the capture command, or, when `project.json` has a `recording.auto_start_phase` or you give `--record-from-phase ID`, on the step at which the capture enters that phase. It ends with the run (`end_reason` `session_end`) unless one of its own rules stops it first: the end event plus the stop delay, or the duration or size cap. The other settings come from the `recording` block of `project.json` ([Project files](../concepts/project-files#the-recording-block)). `NAME` may hold letters, digits, `-` and `_`; an existing recording of that name is an error.

A recording does not change the run: `trajectory.csv`, `summary.json` and the `state_hash` are the same with and without `--record`.

## Replaying a recording

```bash
ssg-sim replay my-project/recordings/r [--assets DIR] [--frames-only]
```

checks the recording's files, re-simulates it from step 0 with the scenario, plan and inputs stored in it (no dataset lookup; the number of steps the spawn update ran is reproduced), and compares every frame of every body bit for bit:

```text
recording my-project/recordings/r: 193 bodies, 5 frames, steps 2–130 (2.00 s), a frame every 32 steps
built with: this build
re-simulated steps 0–130 with k = 1: frames: 5, identical: 5
```

On a difference it also prints the first one and the largest:

```text
first difference: frame step 34, body 5 (tether_node), |Δposition| 0e0 m, |Δvelocity| 1.1102230246251565e-16 m/s
largest |Δposition|: 0e0 m (frame step 34, body 5)
same target and version: a defect, or a different C library or toolchain (F-48)
```

`--frames-only` checks the files and does not re-simulate. The exit code is 0 when every frame is identical; 1 for a difference although the recording was made by this target and version (a defect, or a different C library or toolchain); 3 for a difference when it was made by another target or version (expected across platforms and versions); 2 for a usage error or an invalid recording, including an app recording without the spawn-update step count.

On the machine that recorded it, a recording made by `ssg-sim run --record` replays with exit 0. A recording made in the app does not re-simulate bit for bit yet, even on that machine (F-65); see [Recordings](../concepts/recordings#what-re-simulation-promises).

## Checking a project

```bash
ssg-sim validate my-project [--assets DIR]
```

reads `project.json`, every plan file and every recording under `recordings/` without running anything. It prints one line per finding, then the totals:

```text
warning: [project.json] /target/source/dataset: `datasets/gone.json` does not exist (the stored orbit is used)
error: [project.json] /active_plan: no plan `nope` in the project or the example plans
1 errors, 1 warnings
```

- `error:` lines are problems that stop a run: a file that is not valid JSON, an unknown key (named), a `schema_version` this build does not read, a value out of range, an orbit that gives no initial state, an `active_plan` that is neither a project plan nor an example plan, and the fields the manifest has but that are applied from a later release (`ssg-sim` says "applied from v0.4.0-rc.4").
- `warning:` lines are things that do not stop a run: a plan file in the old (v0) format that was migrated in memory, with what the migration could not carry over, a `source.dataset` that no longer exists, a `recording.auto_start_phase` that names no phase of the active plan, and an unfinished recording (`<name>.partial`, left by a crash; it cannot be recovered).
- For each recording, `error:` lines name what makes it unreadable (a missing `keyframes.parquet`, a row or step count that does not match the manifest, frames out of order), and an `info:` line gives its frames, bodies and steps.
- `info:` lines say that the folder has no `project.json` (it is read as a v0 project), has no plan files, or that `project.json` was written by another build or platform (results are bit-identical only on the same build and platform).

The exit code is 0 without errors, 1 with errors and 2 for a usage error (no directory, an unknown option). Nothing is written or changed.

## Output

- `summary.json`: the plan, target and chaser (NORAD ids), epoch mode and start epoch, the number of fixed steps run (`steps`), the warp, every phase transition (`step`, the fixed step it fired in; `from`; `to`), the final sample (`final`: `step`, `phase`, `range_m`), the `state_hash` and the wall-clock time (`wall_s`).
- `trajectory.csv`: one row every `--sample-every` fixed steps with the columns `step` (fixed step), `epoch_utc` (`YYYY-MM-DDTHH:MM:SS.ffffffZ`, UTC, the format of `project.json`), `phase` (the capture phase), `range_m` (distance between the tether root and the target, m) and `rel_speed_mps` (the root's speed relative to the target in the target's LVLH frame, m/s). Before v0.4.0-rc.1 the columns were `step,epoch,state,range_m,rel_speed_mps` and `final` had `state` instead of `phase`.
- `ephemeris.oem`: a CCSDS Orbit Ephemeris Message (OEM 2.0, KVN text) with two segments, the target and then the chaser (the tether root), one state at every `--sample-every` fixed step. `REF_FRAME = GCRF`, `CENTER_NAME = EARTH`, `TIME_SYSTEM = UTC`; `OBJECT_NAME` is the body's label and `OBJECT_ID` the GP record's own (for Hubble `1990-037B`), else the label; `CREATION_DATE` is the run's start epoch, so the same run writes the same bytes. Positions are in km with 6 decimals (1 mm), velocities in km/s with 9 decimals (1 µm/s). The tests read it back with brahe's OEM parser. A run that stops before its first sample writes no `ephemeris.oem`.
- `state_hash`: a 64-bit FNV-1a hash (16 hex digits) of the sampled state: per sample, the step and the tether root's and the debris's position and velocity, bit for bit. Two runs with the same hash sampled bit-identical states.

What a fixed `--epoch` guarantees, and what differs between platforms, is described in [Determinism](../concepts/determinism).

The exit code is 0 on success, 1 on a failure (missing plan, NORAD id not found, an invalid `project.json`, no capture with `--stop-at-capture`) and 2 for a usage error.

## Example

The Hubble scenario with a fixed start epoch, so that the run is repeatable:

```bash
ssg-sim run my-project --target 20580 --chaser 99901 \
  --dataset crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json \
  --epoch 2026-04-03T16:03:47.002464 --stop-at-capture
```

On macOS (Apple Silicon) this ends in `capture` at step 162745 with the transitions at steps 4318, 34755 and 162745. With `--epoch now` (the default) the start epoch is the wall clock, as in the app, and the capture step varies from run to run.
