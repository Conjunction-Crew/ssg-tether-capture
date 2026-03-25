---
sidebar_position: 1
---

# Introduction

**SSG Tether Capture** is a real-time simulation built with [Bevy](https://bevyengine.org/) that models the capture of orbital debris using space tethers. It combines Keplerian orbital mechanics, rigid-body physics, and 3D Earth visualization into a single interactive application.

## What it does

- **Propagates orbits** — satellites, debris, and tether systems are positioned using Keplerian propagation driven by TLE data or classical orbital elements (COE).
- **Simulates tether dynamics** — a configurable multi-joint tether is modelled as a chain of rigid bodies using [Avian3D](https://github.com/Jondolf/avian) physics.
- **Renders in real time** — a 3D Earth scene with atmospheric scattering, a 2D orbital map view, and a separate UI layer all run simultaneously via Bevy's render layer system.
- **Supports time warp** — simulation time can be scaled up or down through a `TimeWarp` resource, useful for fast-forwarding orbital periods.

## Key capabilities

| Capability | Detail |
|---|---|
| Orbital propagation | Per-entity Keplerian propagation via [`brahe`](https://github.com/duncaneddy/brahe) |
| Orbit initialisation | TLE string, classical orbital elements, or raw position/velocity |
| Tether physics | Multi-joint rigid body chain (Avian3D) |
| Camera | Orbit camera with scene/map view toggle |
| UI | Bevy-native UI — project catalogue, project detail screen |
| Rendering | KTX2 textures, atmospheric scattering, HDR skybox, post-process bloom |

## Where to go next

- [Prerequisites](./prerequisites) — what you need installed before running the app
- [Running the App](./running-the-app) — how to build and launch
- [Architecture Overview](../architecture/overview) — how the major systems fit together
