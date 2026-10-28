---
sidebar_position: 7
---

# Running without a window (`ssg-sim`)

`ssg-sim` runs a capture scenario headless: no window, no rendering, no UI. It uses the same simulation core as the app and writes the result to files. It is built from the `crates/ssg-sim-cli` crate:

```bash
cargo run --release -p ssg-sim-cli -- run <project-dir> --target <NORAD> --chaser <NORAD> [options]
```

## Usage

```text
usage: ssg-sim run <project-dir> [options]

options:
  --plan ID            capture plan id (default: the only plan in <project-dir>, else example_plan)
  --target NORAD       NORAD id of the capture target (required)
  --chaser NORAD       NORAD id of the chaser (required)
  --dataset FILE       GP JSON dataset to search for the ids (repeatable; searched after
                       <project-dir>/datasets/*.json and <assets>/datasets/*.json)
  --assets DIR         assets directory (default ./assets)
  --epoch now|ISO      start epoch: `now` (default, the app path) or YYYY-MM-DDTHH:MM:SS[.fff]
  --steps N            number of updates to run (default 200000)
  --warp N             time-warp multiplier (default 1)
  --sample-every N     trajectory sample interval in updates (default 64)
  --out DIR            output directory (default <project-dir>/runs/<utc-timestamp>)
  --stop-at-capture    stop as soon as the state is `capture`
```

`<project-dir>` holds your own capture plans (`*.json`) and, optionally, a `datasets/` folder. The example plans in `<assets>/example_capture_plans` are always loaded.

## Output

- `summary.json`: the plan, target, chaser, epoch mode and start epoch, the number of updates, the warp, every state transition (`step`, `from`, `to`), the final state and range, and the wall-clock time.
- `trajectory.csv`: `step,epoch,state,range_m,rel_speed_mps`, one row every `--sample-every` updates.
- One summary line on stdout.

The exit code is 0 on success, 1 on a failure (missing plan, NORAD id not found, no capture with `--stop-at-capture`) and 2 for a usage error.

## Example

The Hubble scenario with a fixed start epoch, so that the run is repeatable:

```bash
ssg-sim run my-project --target 20580 --chaser 99901 \
  --dataset crates/ssg-sim/src/test_support/fixtures/q13_hubble_pair.json \
  --epoch 2026-04-03T16:03:47.002464 --stop-at-capture
```

On macOS (Apple Silicon) this ends in `capture` at step 156811 with the transitions at steps 4318, 34112 and 156811. With `--epoch now` (the default) the start epoch is the wall clock, as in the app, and the capture step varies from run to run.
