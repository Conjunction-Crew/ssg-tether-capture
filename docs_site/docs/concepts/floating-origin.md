---
sidebar_position: 5
---

# Floating Origin

Orbital distances are enormous — low Earth orbit is roughly 400 km altitude, geostationary orbit is ~36,000 km. Bevy (and most game engines) use 32-bit floats for world-space transforms, which have insufficient precision at these scales.

## How it works

The simulation is centred on the **simulation anchor** (`SimAnchor`), the tether root. After each physics step, `target_entity_reset_origin` computes the tether's centre of mass; when it is more than `MAX_ORIGIN_OFFSET` from the origin, every enabled body's `Position` and `LinearVelocity` are shifted by the centre of mass's state and every enabled body's orbit propagator is shifted by the same amount, so relative positions and the ECI states are preserved. The camera target (`CameraTarget`) is an app-side viewing marker: cycling it never moves the origin or the physics bubble.

The **physics bubble** is the anchor's Kepler orbit, translating, with ECI axes. Bodies within `PHYSICS_ENABLE_RADIUS` of the origin are simulated by Avian3D; their orbit propagator describes the frame origin, and their ECI state is that orbit plus their `Position`/`LinearVelocity`. Bodies beyond `PHYSICS_DISABLE_RADIUS` are disabled (`RigidBodyDisabled`) and propagated on their own orbit; their `Position`/`LinearVelocity` mirror their state relative to the anchor each step. A body leaving the bubble gets its own orbit (the frame state plus its relative state); a body entering it gets the frame origin's orbit (its own state minus its relative state).

**Relative dynamics.** The frame origin is in free fall on its Kepler orbit, so a body inside the bubble at `Position` `p` feels only the difference of Earth's gravity between `r_o + p` and `r_o` (`r_o` is the anchor's ECI position): `apply_relative_gravity` applies `μ·(r_o/|r_o|³ − (r_o + p)/|r_o + p|³)` to every enabled body in `SimStep::RelativeDynamics`. Because the bubble keeps ECI axes, no Coriolis or centrifugal term appears; to first order this is the Clohessy-Wiltshire gravity-gradient term, and `orbit::lvlh` holds the LVLH frame and the closed-form CW solution it is tested against. Avian's global `Gravity` stays zero.

## Relevant constants

| Constant | Description |
|---|---|
| `PHYSICS_ENABLE_RADIUS` | Entities within this distance (double precision, metres) have Avian3D physics enabled |
| `PHYSICS_DISABLE_RADIUS` | Entities beyond this distance have physics suspended (`RigidBodyDisabled`) |
| `MAX_ORIGIN_OFFSET` | Maximum allowable offset before a floating origin correction is applied |

## Interaction with the camera

`orbit_camera_track` is ordered **after** `floating_origin` (`.after(floating_origin)`) so that the camera transform is computed using the post-shift world positions, preventing a one-frame lag artefact.
