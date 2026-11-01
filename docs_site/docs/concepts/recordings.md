---
sidebar_position: 10
---

# Recordings

:::note Draft
This page describes recording and replay as of v0.4.0-rc.2. It is a draft; the full documentation pass comes later.
:::

A recording stores a segment of a running simulation in the project folder, under `recordings/<name>/`. You can play it back in the application (pause, step, rewind, slow down, speed up) as often as you like, and `ssg-sim replay` can check it against a fresh simulation of the same inputs.

A recording is a **viewing and analysis record**: playback shows the recorded states and never runs the physics again. Re-simulation is used only to verify a recording.

## What is recorded

| What | Where | Notes |
|---|---|---|
| The state of every recorded body: position, rotation, linear and angular velocity, in the simulation's local frame, as 64-bit floats | `keyframes.parquet` | One **frame** at the first step, every K steps after it, and at the last step. The recorded bodies are the target and every tether body (root, interior nodes, tail). |
| Per frame: step, epoch, capture phase, capture-sphere radius, the frame origin's orbit, the floating-origin shift since the previous frame | `frames.jsonl` | See [The frame origin and origin shifts](#the-frame-origin-and-origin-shifts). |
| The capture transitions inside the segment | `transitions.jsonl` | Step, epoch, `from`, `to` and the reason. Playback takes the exact phase at any step from them. |
| The log events of the segment | `log.jsonl` | The format of a saved [Capture Log](./capture-log). |
| The input log **from step 0** | `inputs.jsonl` | Every input that affects the simulation: first the spawn-update step count, then each capture start with the step it was applied after. |
| The scenario and the plan | `manifest.json` | A copy of `project.json` as the run used it and of the capture plan, so the recording does not depend on the project's current files. |

K is the frame interval, `keyframe_interval_steps` (default 32 steps, two frames per simulated second). The file layout and every key are described in [Project files](./project-files#recordings); the manifest's schema is [`recording-manifest.v1.schema.json`](https://conjunction-crew.github.io/ssg-tether-capture/schemas/recording-manifest.v1.schema.json) and the `recording` block's is part of [`project.v1.schema.json`](https://conjunction-crew.github.io/ssg-tether-capture/schemas/project.v1.schema.json).

### The spawn-update step count

The update that creates the bodies may run some fixed steps before the orbits start. The desktop app runs 0 to 3 of them when you click Start (more when the warp was raised before Start). The command line and the test harness always run 1. Those steps move the tether, which is not at rest when it is created, so they change everything after them. The input log's first line records their number k:

```json
{"step":1,"command":{"session_start":{"steps_before_propagators":1}}}
{"step":215,"command":{"start_capture":{"target":"target","plan_id":"example_plan"}}}
```

Warp, pause and camera changes are not inputs: they do not change the result ([Determinism](./determinism)).

### The frame origin and origin shifts

The bodies are stored in the simulation's local frame, whose origin follows the tether ([Floating origin](./floating-origin)). Each frame stores that origin's position and velocity in ECI, `frame_origin_eci`, so the Earth can be placed around the bodies. When the floating origin is reset, every body's local position jumps by up to 50 m. The frame after a reset stores the total shift in `origin_shift_m` and `origin_shift_mps` (zero in most frames), and playback uses it so that neither the scene nor the Earth jumps.

## Starting and stopping

- **By hand.** In the app, **Record** and **Stop** in the **Recording** window on the sim screen. On the command line, `ssg-sim run … --record NAME` records from the capture command to the end of the run ([Running without a window](../getting-started/headless-cli#recording-a-run)).
- **At a phase.** With `recording.auto_start_phase` set in `project.json` (or `--record-from-phase ID` on the command line), the recording starts on the step at which the capture enters that phase. Its first frame is that step.
- **After the end event.** A capture plan's phase can carry `end: "success"` or `end: "abort"`. When the capture enters such a phase, the simulation writes the log line `Capture ended: success (phase capture)` (or `abort`). Nothing else changes: the controller keeps running in that phase. With `auto_stop_on_end` (the default) the recording stops `stop_after_end_s` (default 60) simulated seconds later, so it still holds the motion after the end.
- **At a cap.** At `max_duration_s` (default 1,800 s, 30 minutes) or when the next frame would make the keyframe table larger than `max_size_mb` (default 100 MB). A warning goes to the log and the Recording window at 80 % of either cap.
- **When the session ends.** Leaving the sim screen, or the end of an `ssg-sim run`, stops the recording.

The manifest's `end_reason` says which rule stopped it: `manual`, `end_success`, `end_abort`, `max_duration`, `max_size` or `session_end`.

A recording is refused, with a log line, when another one is already running or when the tether has more than `max_tether_bodies` bodies (default 200).

The settings are the optional `recording` block of `project.json` ([Project files](./project-files#the-recording-block)). The app's Recording window shows them, and **Save** writes them into `project.json`. A project without the block records only by hand.

The app names a recording `<plan-id>-<UTC time>`, for example `example_plan-20261031T161300Z`.

## Sizes

A frame takes 116 bytes per body (the Parquet file adds about 0.2 %). The default 20 m tether has 77 bodies, so with the target a frame is 9.0 KB. With a 200-body tether and the target (201 bodies):

| Frame interval K | Frames per simulated second | MB per simulated minute | 30-minute segment | Longest segment under 100 MB |
|---|---|---|---|---|
| 1 step | 64 | 89.5 | 2.7 GB | 67 s |
| 8 steps | 8 | 11.2 | 336 MB | 8.9 min |
| 16 steps | 4 | 5.6 | 168 MB | 17.9 min |
| **32 steps (default)** | **2** | **2.8** | **84 MB** | **30 min (the cap)** |
| 64 steps | 1 | 1.4 | 42 MB | 30 min |

A smaller K gives finer playback and larger files. For a short step-exact segment, set K to 1; the size cap then stops the recording after about a minute. Growth is linear, and memory holds at most 64 frames before they are written.

## Playing a recording

In the app, open the **Recordings** window on the sim screen before you start a simulation. It lists `<project>/recordings/` with the name, duration, bodies, size and how the recording's build differs from the running one. **Open** starts a replay in the detail view, following the tether root. The **Transport** window has:

- jump to the start or end, step to the previous or next frame, play and pause, reverse;
- speeds of 1/8×, 1/4×, 1/2×, 1×, 2×, 4×, 8×, 16×, 32× and 64× (simulated seconds per second);
- a scrub slider over the whole segment;
- the step, epoch, phase, range, relative speed and capture-sphere radius, with a marker when the state shown lies between two frames;
- buttons to switch between the map and the detail view and to follow the debris or the tether root, and **Exit**, back to the setup screen.

Step and the jump buttons **snap to frames**, so what you see after a step is a recorded state. Play and scrub show states between frames by **origin-aware interpolation**: the earlier frame is first moved into the later frame's origin, using the later frame's origin shift, and then positions and velocities are interpolated linearly and rotations by normalised linear interpolation. The Transport window marks these states as interpolated. At K = 32 the interpolation error is about 3 cm for the controller's accelerations. The phase is exact at every step, taken from the transitions. The capture-sphere radius is interpolated between frames, and held at a transition.

A replay runs no physics. The capture button, plan editing and the warp keys are off, and nothing you do in a replay changes the recording.

**Playback is identical on every platform**, because it only reads the files.

## Verifying a recording

```bash
ssg-sim replay my-project/recordings/r
```

checks the recording's files, re-simulates the run from step 0 with the recorded scenario, plan and inputs (including the spawn-update step count k), and compares every frame of every body bit for bit:

```text
recording my-project/recordings/r: 78 bodies, 5 frames, steps 2–130 (2.00 s), a frame every 32 steps
built with: this build
re-simulated steps 0–130 with k = 1: frames: 5, identical: 5
```

On a difference it prints the first one (frame step, body, |Δposition|, |Δvelocity|) and the largest |Δposition|. `--frames-only` checks the files without re-simulating. A recording that has no `project` in its manifest still plays, but it cannot be re-simulated.

| Exit code | Meaning |
|---|---|
| 0 | Every frame is identical. |
| 1 | A difference, although the recording was made by this target and version: **"same target and version: a defect, or a different C library or toolchain"**. The same target does not pin the platform's C math library: two Linux/x86_64 machines with different C library versions may legitimately differ. |
| 3 | A difference, and the recording was made by another target or version. This is expected across platforms and versions ([Determinism](./determinism#across-platforms)). |
| 2 | Usage error or invalid recording, including an app recording whose input log lacks the spawn-update step count (a command-line recording without it is re-simulated with k = 1). |

### What re-simulation promises

Playback is identical on every platform. On the machine that recorded it, a recording made by `ssg-sim run --record` re-simulates bit for bit.

**An app recording does not re-simulate bit for bit yet.** The app's run already differs from the headless re-simulation before any tether–debris contact, for a reason not yet found (F-65). Expect `ssg-sim replay` to exit 1 for a recording made in the app, even on the machine that made it. Beyond that, the headless debris has no collider and no collider-derived mass or inertia; the app builds both when the glTF scene has loaded, at a wall-clock-dependent step (F-64). So the command line never simulates tether–debris contact, and the app's contact begins at a step that varies from run to run.

On another machine, platform or build, re-simulation may differ, and `ssg-sim replay` reports by how much.

## Unfinished recordings

A recording is written to `recordings/<name>.partial/` and renamed to `recordings/<name>/` when it is complete. If the application or `ssg-sim` stops while recording (a crash, a killed process), the `.partial` folder stays behind. Its Parquet file has no footer, which is written last, so **the whole recording is lost, not only its last part**. It cannot be recovered and must be made again. The Recordings window does not list `.partial` folders, and `ssg-sim validate` warns about each one:

```text
warning: recordings/crashed.partial: unfinished (an interrupted run or a crash); its Parquet file has no footer and cannot be recovered: make the recording again
```

`ssg-sim validate` also checks every finished recording: the manifest, that `keyframes.parquet` exists, one row per body per frame, the frame steps (strictly increasing, and a multiple of K after the first frame, except the last), `frames.jsonl` against the table, the input log (starting with the spawn-update step count, ordered by step) and the transitions (inside the segment, ordered). See [Running without a window](../getting-started/headless-cli#checking-a-project).
