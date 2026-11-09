---
sidebar_position: 1
---

# Introduction

**SSG Tether Capture** simulates the capture of orbital debris with a space tether. A researcher chooses a target and a chaser, writes a capture plan, and watches or records the capture in a desktop application built with [Bevy](https://bevyengine.org/), or runs the same simulation headless from the command line.

## What it does

- **Starts real orbits.** Targets and chasers come from Space-Track GP (OMM) records, started from their SGP4 state at the start epoch, or from Keplerian elements; a chaser can be placed on the target's V-bar or R-bar.
- **Simulates a rope tether.** The tether is a rope of point masses between two end spheres, jointed so it can go slack but never stretch, with contacts against itself and the debris ([Avian3D](https://github.com/Jondolf/avian) physics, in a physics bubble around the chaser).
- **Runs a capture plan.** A plan is a sequence of phases, each with its speed and force limits and its transitions; the capture controller pushes the tether's ends to close in, circle the target and wrap it.
- **Is deterministic.** The same build, platform, project and inputs give bit-identical results, whatever the frame rate or time warp.
- **Records and replays.** A recording stores the run's states and inputs; it plays back in the app and can be re-simulated to check it.

## Key capabilities

| Capability | Detail |
|---|---|
| Desktop app | Installers for Windows, macOS and Linux; scene and orbital map views; the plan form; the capture log; plots; CSV and CCSDS OEM exports |
| Headless CLI | `ssg-sim run`, `validate`, `replay`, built from source ([Headless CLI](./headless-cli)) |
| Projects | A folder with `project.json`, capture plans (schema v1) and recordings, all with published JSON Schemas ([Project files](../concepts/project-files)) |
| Orbits | SGP4 start states, then two-body propagation ([brahe](https://github.com/duncaneddy/brahe)); relative gravity inside the physics bubble |
| Time | Fixed 1/64 s steps; a time-warp ladder (1×, 2×, 4×, …) limited by what the machine can run ([Time Warp](../concepts/time-warp)); pause |
| Recordings | Frames every K steps, origin-aware playback, bit-for-bit re-simulation on the same build ([Recordings](../concepts/recordings)) |

## Where to go next

### Install the app

- [Installing on Windows](./install-windows)
- [Installing on macOS](./install-macos)
- [Installing on Linux](./install-linux)

### Run and build from source

- [Prerequisites](./prerequisites) — what you need installed before building the app from source
- [Running the App](./running-the-app) — how to build and launch

### Learn how it works

- [Architecture Overview](../architecture/overview) — how the major systems fit together
- [Determinism](../concepts/determinism) — what reproduces bit for bit, and across platforms
