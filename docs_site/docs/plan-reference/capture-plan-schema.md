---
title: Capture plan schema (v1)
sidebar_position: 1
---

{/* Generated from the JSON Schema by the ssg-sim test schema_docs_are_current. Do not edit; regenerate with: SSG_UPDATE_SCHEMAS=1 cargo test -p ssg-sim schema_ */}

# Capture plan schema (v1)

A capture plan is a JSON file that describes the capture device, the capture sphere and the phases of a capture. This page lists every key of a plan with `schema_version: 1`. It is generated from the plan's JSON Schema, [`capture-plan.v1.schema.json`](https://conjunction-crew.github.io/ssg-tether-capture/schemas/capture-plan.v1.schema.json), which editors can use to check a plan as it is written; both are produced from the simulator's own types, so they match what the app and `ssg-sim` accept.

A key marked "no" under Required may be left out; its description gives the default.

## Plan file (top level)

A capture plan file, schema v1. Lengths are metres, speeds m/s, forces N, masses kg; there is no unit key.

| Key | Type | Required | Description |
|---|---|---|---|
| `capture_sphere` | [`CaptureSphere`](#capturesphere) or null | no | The capture sphere. Absent: every default applies. |
| `device` | [`CapturePlanDevice`](#captureplandevice) or null | no | The capture device. Absent: every device default applies. |
| `migrated_from_v0` | [`MigratedFromV0`](#migratedfromv0) or null | no | What a migrated v0 file held that v1 has no field for (written back unchanged). |
| `name` | string | yes | Display name shown to the user. Not used as a lookup key. Must not be blank. |
| `phases` | array of [`Phase`](#phase) | yes | The phases in order; a capture starts in the first. At least one. |
| `schema_version` | integer, exactly 1 | yes | File format version; `1`. A file without it is a v0 plan and is migrated on load. |
| `tether` | string | yes | Id the tether is registered under. Must not be blank. |

No other key is allowed.

## Actuation

The force law of a phase.

One of these strings:

- `circle`: The range and tangent branches, or in a phase with `shrink_rate_mps` the circle law: a continuous hold of the body's sphere radius (the default).
- `hold`: Brake every pushed body to rest relative to the target and hold the plane; `max_velocity_mps` is read as 0.

## CapturePlanDevice

The capture device. Every field is optional; an absent field takes its default.

| Key | Type | Required | Description |
|---|---|---|---|
| `end_mass_kg` | number or null | no | Mass of the tail, and of the root unless `project.json` gives the chaser a `mass_kg`, in kg. Default 2.0; greater than 0. |
| `joint_damping_angular` | number or null | no | Angular damping of each tether joint. Default 1.0; not negative. No effect on the rope model (point masses); kept for file compatibility. |
| `joint_damping_linear` | number or null | no | Linear damping of each tether joint. Default 1.0; not negative. |
| `segment_mass_kg` | number or null | no | Mass of each interior tether node in kg. Default 0.25; greater than 0. |
| `segment_spacing_m` | number or null | no | Distance between tether joints in metres. Default 0.25; greater than 0, and the tether may have at most 1,000 bodies (a warning above 200). |
| `spawn_orientation` | [`SpawnOrientation`](#spawnorientation) or null | no | How the tether is laid out at spawn, from both bodies' states at the start epoch: `toward_target` (the tail trails along the root-to-target line, away from the target), `v_bar` (behind the chaser along its velocity axis) or `r_bar` (below it, toward the Earth). Default `toward_target`. A spawn condition, not a held attitude. |
| `tether_length_m` | number or null | no | Physical length of the tether in metres. Default 20.0; greater than 0. |
| `type` | string or null | no | Device kind, free text. Default `"tether"`. |

No other key is allowed.

## CaptureSphere

The capture sphere around the target.

| Key | Type | Required | Description |
|---|---|---|---|
| `initial_radius_m` | number or null | no | Radius in metres at the start of every capture. Default 25.0; greater than 0. |
| `min_radius_m` | number or null | no | The radius the capture sphere stops shrinking at, m. Default 0.1. |

No other key is allowed.

## ConditionBounds

Bounds on one quantity; at least one, neither negative. Both strict.

| Key | Type | Required | Description |
|---|---|---|---|
| `greater_than` | number or null | no | Holds while the quantity is above this value. |
| `less_than` | number or null | no | Holds while the quantity is below this value. |

No other key is allowed.

## MigratedFromV0

The v0 content a migration could not map, so the file can be written back without losing it.

| Key | Type | Required | Description |
|---|---|---|---|
| `unmapped` | array of [`UnmappedValue`](#unmappedvalue) | yes | Every unmapped v0 value with its JSON Pointer into the v0 document. |

No other key is allowed.

## Phase

One phase of the plan: the controller's limits while in it and its transitions.

| Key | Type | Required | Description |
|---|---|---|---|
| `actuation` | [`Actuation`](#actuation) or null | no | `circle`: the range and tangent branches, or in a phase with `shrink_rate_mps` the circle law (the default); `hold`: brake every pushed body to rest relative to the target and hold the plane; `max_velocity_mps` is read as 0. |
| `controller` | [`PhaseController`](#phasecontroller) or null | no | The controller's gains in this phase; every key optional, each defaulting to the controller's built-in value. Absent: all defaults. |
| `end` | [`PhaseEnd`](#phaseend) or null | no | Marks an end phase (`success` or `abort`); an end phase has no transitions. |
| `hold_plane` | boolean or null | no | Hold the pushed bodies in the circling plane through the target: a proportional force component toward the plane, full at `PLANE_HOLD_BAND_M` of height. Default false. |
| `id` | string | yes | Phase id, unique in the plan and not blank. |
| `max_force_n` | number | yes | Force applied to each tether node in N; not negative. |
| `max_velocity_mps` | number | yes | Speed limit of the tether nodes in m/s; not negative. |
| `shrink_rate_mps` | number or null | no | Rate at which the capture sphere shrinks in m/s; not negative. Absent: no shrink. |
| `tail_actuation` | boolean or null | no | Push the tail in this phase as well as the root. Default false. |
| `tail_spacing_m` | number or null | no | The chord from the root to the tail the controller holds, in m, the tail behind the root in the sense of circling, in a phase that pushes the tail. Without it the tail circles on its own sphere with no spacing held (the pre-O5 law). Greater than 0 and less than `device.tether_length_m`. |
| `transitions` | array of [`Transition`](#transition) or null | no | Checked in order every step; the first whose conditions all hold fires. |
| `velocity_scales_with_sphere` | boolean or null | no | The phase's speed limit is `max_velocity_mps` at the capture-sphere radius the phase was entered at and falls in proportion as the sphere shrinks. A no-op in a phase without `shrink_rate_mps` (the radius stays where it was entered). Default false. |

No other key is allowed.

## PhaseController

A phase's controller gains. Every field is optional; an absent field takes the controller's built-in default.

| Key | Type | Required | Description |
|---|---|---|---|
| `dead_band_fraction` | number or null | no | The relative width of the hysteresis band on the branch law's inner and outer edges, and of the velocity kill band (at least two steps' Δv (2 × `max_force_n` × step / mass)); 0 removes the hysteresis. Default 0.02; at least 0 and less than 0.5. |
| `force_ramp_s` | number or null | no | Seconds the force bound takes to rise from 0 to `max_force_n` at the capture start and at every phase entry. Default 1.0; greater than 0. |
| `inner_edge_fraction` | number or null | no | The branch law's inner edge as a fraction of the sphere radius: inside it the body is pushed away from the target. Default 0.8; greater than 0 and less than 1. |
| `plane_hold_band_m` | number or null | no | `hold_plane`: the height above or below the circling plane, in m, at which the component toward the plane reaches full weight. Default 1.0; greater than 0. |
| `radial_rate_band_mps` | number or null | no | The circle law: the outward speed against the sphere's own shrink, in m/s, at which the rate term reaches full weight. Default 0.1; greater than 0. |
| `radius_hold_band_m` | number or null | no | The circle law: the radius error, in m, at which the radial term that holds a body on its sphere reaches full weight; proportional below it. Default 2.0; greater than 0. |
| `spacing_band_m` | number or null | no | The tail's lag law: the error in the root-to-tail chord, in m, at which the tail's tangent weight saturates. Default 3.0; greater than 0. |
| `spacing_rate_band_mps` | number or null | no | The tail's lag law: the difference in the ends' speeds along their forward tangents, in m/s, at which the rate term saturates the tail's tangent weight. Default 0.3; greater than 0. |

No other key is allowed.

## PhaseEnd

How an end phase ends the capture.

One of these strings: `success`, `abort`.

## SpawnOrientation

The axis the tether is laid along at spawn.

One of these strings:

- `toward_target`: The tail trails along the root-to-target line, away from the target.
- `v_bar`: The tail trails along the chaser's −V-bar (its LVLH along-track axis).
- `r_bar`: The tail hangs along the chaser's −R-bar (its LVLH radial axis).

## Transition

A transition to another phase.

| Key | Type | Required | Description |
|---|---|---|---|
| `all_of` | [`TransitionConditions`](#transitionconditions) | yes | The conditions; all must hold (AND). At least one. |
| `to` | string | yes | Id of the phase to enter. |

No other key is allowed.

## TransitionConditions

The conditions of a transition.

| Key | Type | Required | Description |
|---|---|---|---|
| `closing_rate_mps` | [`ConditionBounds`](#conditionbounds) or null | no | Rate at which the range between the tether root and the target is shrinking, m/s; positive while closing. The same root and target as `distance_m`; does not hold while the range is 0. |
| `distance_m` | [`ConditionBounds`](#conditionbounds) or null | no | Range between the tether root and the target in metres. |
| `relative_speed_mps` | [`ConditionBounds`](#conditionbounds) or null | no | Magnitude of the LVLH relative velocity in m/s. |
| `tether_straightness` | [`ConditionBounds`](#conditionbounds) or null | no | How far the tether bows away from the straight line between its root and its tail: the largest distance of any tether body from that line divided by the root-to-tail distance; 0 = straight, dimensionless. `less_than: 0.15` holds while no body is farther than 15 % of the root-to-tail distance from that line. Does not hold while the root and the tail are within 1e-6 m. |
| `time_in_phase_s` | [`ConditionBounds`](#conditionbounds) or null | no | Seconds since the current phase was entered. |

No other key is allowed.

## UnmappedValue

One unmapped v0 value.

| Key | Type | Required | Description |
|---|---|---|---|
| `pointer` | string | yes | JSON Pointer into the v0 document, e.g. `/states/2/transitions/0/distance/units`. |
| `value` | any JSON value | yes | The value as the v0 file had it. |

No other key is allowed.
