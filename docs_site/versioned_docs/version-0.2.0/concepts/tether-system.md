---
sidebar_position: 2
---

# Tether System

The space tether is modelled as a chain of rigid bodies connected by joints, simulated using [Avian3D](https://github.com/Jondolf/avian) physics.

## Configuration

Two constants in `src/constants.rs` control the tether geometry:

| Constant | Value | Description |
|---|---|---|
| `NUM_TETHER_JOINTS` | `15` | Number of rigid body segments in the tether chain |
| `DIST_BETWEEN_JOINTS` | `1.1` | Distance between adjacent joints (metres, in simulation scale) |

## Entity structure

The tether is spawned by `setup_tether` (called in the `Startup` system chain in `lib.rs`). Each joint in the chain is an entity with:

- An `Avian3D` `RigidBody`, `Position`, and `LinearVelocity` component for physics simulation.
- A `TetherNode { root: Entity }` component that holds a reference back to the root joint entity. This allows any joint to quickly identify the tether it belongs to.

The root joint also carries an `Orbital` component so that the entire tether chain is propagated as a single orbital body and then the physics simulation handles relative motion within the chain.

## Physics enable/disable radius

To manage performance, Avian3D physics is only active for entities within a certain distance of the camera target. This is controlled by `PHYSICS_ENABLE_RADIUS` and `PHYSICS_DISABLE_RADIUS` (defined in `constants.rs`):

- Entities within `PHYSICS_ENABLE_RADIUS` have their `RigidBodyDisabled` marker removed — physics is active.
- Entities outside `PHYSICS_DISABLE_RADIUS` have `RigidBodyDisabled` added — physics is suspended.

This is handled by the `ssg_propagate_keplerian` system, which checks distances each propagation step.

## Floating origin interaction

Because orbital distances are enormous (hundreds to thousands of kilometres), the simulation uses a **floating origin** — the Bevy world origin shifts to follow the camera target. See [Floating Origin](./floating-origin) for how this interacts with the tether physics.
