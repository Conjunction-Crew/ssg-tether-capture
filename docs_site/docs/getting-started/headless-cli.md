---
sidebar_position: 7
---

# Running without a window (`ssg-sim`)

`ssg-sim` runs a capture scenario headless: no window, no rendering, no UI. It uses the same simulation core as the app and writes the result to files. It is built from the `crates/ssg-sim-cli` crate:

```bash
cargo run --release -p ssg-sim-cli -- run <project-dir> --target <NORAD> --chaser <NORAD> [options]
cargo run --release -p ssg-sim-cli -- run <project-dir> [options]      # from <project-dir>/project.json
cargo run --release -p ssg-sim-cli -- validate <project-dir>
```

## Usage

```text
usage: ssg-sim run <project-dir> [options]
       ssg-sim validate <project-dir> [--assets DIR]

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

validate checks project.json, every plan file, the active plan and the orbits; it prints
`error:`, `warning:` and `info:` lines and exits 0 without errors, 1 with errors.
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

## Checking a project

```bash
ssg-sim validate my-project [--assets DIR]
```

reads `project.json` and every plan file in the folder without running anything. It prints one line per finding, then the totals:

```text
warning: [project.json] /target/source/dataset: `datasets/gone.json` does not exist (the stored orbit is used)
error: [project.json] /active_plan: no plan `nope` in the project or the example plans
1 errors, 1 warnings
```

- `error:` lines are problems that stop a run: a file that is not valid JSON, an unknown key (named), a `schema_version` this build does not read, a value out of range, an orbit that gives no initial state, an `active_plan` that is neither a project plan nor an example plan, and the fields the manifest has but that are applied from a later release (`ssg-sim` says "applied from v0.4.0-rc.4").
- `warning:` lines are things that do not stop a run: a plan file in the old (v0) format that was migrated in memory, with what the migration could not carry over, and a `source.dataset` that no longer exists.
- `info:` lines say that the folder has no `project.json` (it is read as a v0 project), has no plan files, or that `project.json` was written by another build or platform (results are bit-identical only on the same build and platform).

The exit code is 0 without errors, 1 with errors and 2 for a usage error (no directory, an unknown option). Nothing is written or changed.

## Output

- `summary.json`: the plan, target and chaser (NORAD ids), epoch mode and start epoch, the number of fixed steps run (`steps`), the warp, every phase transition (`step`, the fixed step it fired in; `from`; `to`), the final sample (`final`: `step`, `phase`, `range_m`), the `state_hash` and the wall-clock time (`wall_s`).
- `trajectory.csv`: one row every `--sample-every` fixed steps with the columns `step` (fixed step), `epoch_utc` (`YYYY-MM-DDTHH:MM:SS.ffffffZ`, UTC, the format of `project.json`), `phase` (the capture phase), `range_m` (distance between the tether root and the target, m) and `rel_speed_mps` (the root's speed relative to the target in the target's LVLH frame, m/s). Before v0.4.0-rc.1 the columns were `step,epoch,state,range_m,rel_speed_mps` and `final` had `state` instead of `phase`.
- `ephemeris.oem`: a CCSDS Orbit Ephemeris Message (OEM 2.0, KVN text) with two segments, the target and then the chaser (the tether root), one state at every `--sample-every` fixed step. `REF_FRAME = GCRF`, `CENTER_NAME = EARTH`, `TIME_SYSTEM = UTC`; `OBJECT_NAME` and `OBJECT_ID` are the body's label; `CREATION_DATE` is the run's start epoch, so the same run writes the same bytes. Positions are in km with 6 decimals (1 mm), velocities in km/s with 9 decimals (1 µm/s). The tests read it back with brahe's OEM parser. A run that stops before its first sample writes no `ephemeris.oem`.
- `state_hash`: a 64-bit FNV-1a hash (16 hex digits) of the sampled state: per sample, the step and the tether root's and the debris's position and velocity, bit for bit. Two runs with the same hash sampled bit-identical states.

What a fixed `--epoch` guarantees, and what differs between platforms, is described in [Determinism](../concepts/determinism).
- One summary line on stdout.

The exit code is 0 on success, 1 on a failure (missing plan, NORAD id not found, an invalid `project.json`, no capture with `--stop-at-capture`) and 2 for a usage error.

## Example

The Hubble scenario with a fixed start epoch, so that the run is repeatable:

```bash
ssg-sim run my-project --target 20580 --chaser 99901 \
  --dataset crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json \
  --epoch 2026-04-03T16:03:47.002464 --stop-at-capture
```

On macOS (Apple Silicon) this ends in `capture` at step 162745 with the transitions at steps 4318, 34755 and 162745. With `--epoch now` (the default) the start epoch is the wall clock, as in the app, and the capture step varies from run to run.
