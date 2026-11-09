---
sidebar_position: 2
---

# Tether System

The tether is a rope of point masses between two end spheres, simulated with [avian3d](https://github.com/Jondolf/avian) at 64 fixed steps per second and 8 substeps per step. The **root** is the chaser and the **tail** is the free end. A capture plan's `device` block sets the rope's size and masses ([Project files](./project-files#capture-plans-schema-v1)), and its phases set how the capture controller pushes the two ends.

## The rope

| Part | Default | Plan key |
|---|---|---|
| Length, root centre to tail centre | 20 m | `device.tether_length_m` |
| Node spacing | 0.25 m | `device.segment_spacing_m` |
| Interior node | a 3.1 cm-radius sphere of 0.25 kg (1 kg per metre) | `device.segment_mass_kg` |
| Root and tail | 0.5 m-radius spheres of 2 kg (the root takes the chaser's `mass_kg` when `project.json` gives one) | `device.end_mass_kg` |
| Joint viscosity when slack | 1.0 | `device.joint_damping_linear` |
| Bodies | 77: 75 interior nodes, the root and the tail | |

The rope between the two end spheres' surfaces (19 m by default) is cut into equal intervals as close to the spacing as its length allows, with a node at each interval end. Neighbours are joined centre to centre by a distance joint that is **slack below its spawn length and stops at it**, so the rope can bunch and swing but never stretches. The linear joint damping is the rope's internal viscosity while it is slack.

**No body of the tether rotates.** The interior nodes are point masses, and the root and the tail have their rotation locked: the chaser holds its attitude, and a sphere's spin moves nothing. `device.joint_damping_angular` therefore has no effect on the rope. It is still accepted, so older plans load.

The chaser's `inertia_kg_m2` from `project.json` is loaded onto the root and stays **inert while the rotation is locked**, until the simulation has an attitude model. It is taken in the simulation's ECI-aligned axes at spawn.

## Orientation at spawn

`device.spawn_orientation` lays the rope out from both bodies' states at the start epoch:

| Value | The tail trails |
|---|---|
| `toward_target` (default) | along the line from the chaser to the target, away from the target |
| `v_bar` | behind the chaser along its velocity axis (−V-bar in its LVLH frame) |
| `r_bar` | below the chaser, toward the Earth (−R-bar) |

The orientation is a **spawn condition, not a held attitude**. The rope is inertial, while the chaser-to-target line and the LVLH axes turn at the orbital rate, about 3.8° per minute. So `toward_target` is exact at the start epoch and about 19° off after five minutes of idling before the capture starts. Of the three, only R-bar is a gravity-gradient equilibrium. Every body starts with the identity attitude, and the simulation has no attitude model.

## Contacts

Every tether body is on one collision layer and touches the debris and the rest of the rope. Jointed neighbours never pair. The interior nodes are 6.25 cm across and 0.25 m apart, so the rope can still pass between its own nodes.

The debris collider is owned by the simulation, so the command line and the app hold the same collider set (77 tether bodies, the debris body and its two array children). It is a set of boxes fitted to the Hubble model that the app draws, in the debris body's axes:

| Box | Size (m) | Centre (m) | Notes |
|---|---|---|---|
| The bus (the body's own collider) | 4.15 × 3.0 × 3.0 | (−0.585, 0, 0) | covers the aft ring, the nozzle and the array booms' roots |
| Array A (child collider) | 2.0 × 0.1 × 8.0 | (0, 0, 5.5) | the flat solar array |
| Array B (child collider) | 2.8 × 0.6 × 8.0 | (−0.18, −0.95, −5.4) | the broken array, turned 0.26 rad (14.9°) about the body's X |

The debris's mass is the target's `mass_kg` (2,500 kg by default), its centre of mass is the body origin, and its inertia tensor is that of the three boxes together at that mass (or the target's `inertia_kg_m2`). The array colliders add no mass of their own. The debris keeps the identity attitude. The boxes are fitted to this one model and apply to every target until the target's size becomes a project setting.

## The capture controller

When a capture starts, the controller pushes the **root in every phase**, and the **tail only in a phase with `tail_actuation: true`**. The interior nodes are never pushed. Each pushed end gets one force per step, bounded by the phase's `max_force_n`.

**Force bound and ramp.** `max_force_n` is an upper bound, not a constant push. The bound rises from 0 to it over 1 s (`controller.force_ramp_s`) at the capture start and at every phase entry. A change in the push's direction does not restart it.

**The capture sphere.** Each capture starts with a sphere of `capture_sphere.initial_radius_m` (25 m by default) about the target. A phase with `shrink_rate_mps` shrinks it at that rate, down to `capture_sphere.min_radius_m` (0.1 m by default). The root holds the sphere's radius and the tail holds the radius plus 1 m.

**The circling plane.** The ends circle the target about the debris body's Z, the axis its solar arrays span. The rope therefore wraps the bus between the arrays. The debris keeps the identity attitude, so this axis is the simulation's ECI Z for the whole run: the ends circle in a plane parallel to the equatorial plane, not the orbit plane.

### The push in each phase

| Phase | Law |
|---|---|
| Without `shrink_rate_mps` (rendezvous, approach) | **Three branches**, with a 2 % hysteresis band on each edge. Inside 0.8 R the end is pushed away from the target. Outside R it is pushed toward the target, and braked as well if it is moving away. Between the two it is pushed along the circle. |
| With `shrink_rate_mps` (terminal, capture) | **The circle law**: a continuous hold of the end's own radius (below). |
| `actuation: hold` | **The hold law**: every pushed end is braked to rest relative to the target. `max_velocity_mps` is read as 0. |

**The circle law.** For an end at distance r from the target, on a sphere of radius R that shrinks at s m/s, the push direction is

```text
direction = t × w_t + r_in × clamp(w_r + w_v, −1, 1)
w_r = clamp((r − R) / 2 m, −1, 1)
w_v = clamp((v_out + s) / 0.1 m/s, −1, 1)
```

Here `t` is the tangent of the circle in the sense of circling and `r_in` the unit vector toward the target. `v_out` is the end's outward speed, so `w_v` is zero when the end closes at exactly the sphere's shrink rate. The tangent weight w_t is 1 − |w_r|: the end swings onto the circle as its radius error vanishes. In a phase whose id is `capture`, a pushed tail runs the other way round the circle (the counter-tangent), which wraps the rope around the bus.

**The tail's spacing (`tail_spacing_m`).** In a phase that pushes the tail and sets `tail_spacing_m`, except in `capture` and `hold`, the tail's tangent term holds the chord from the root to the tail at that length, with the tail behind the root:

```text
w_t = clamp((c − c*) / 3 m − Δv / 0.3 m/s, −1, 1)
c   = 2 R_t sin(θ / 2)
c*  = min(tail_spacing_m, 2 R_t sin 75°)
```

`θ` is the tail's angle behind the root about the circling axis, `R_t` the tail's sphere radius, and `Δv` the tail's speed along its tangent less the root's. A tail that falls behind is pushed forward, and a tail that runs up on the root is held back. On a small sphere the chord is capped at 150° of arc. Without the key, the tail circles on its own sphere with no spacing held.

**The plane hold (`hold_plane`).** In a phase with `hold_plane: true`, each pushed end also gets a component toward the circling plane through the target. It is proportional to the end's height above the plane and reaches full weight at 1 m.

**The speed limit.** The push along its own direction fades to zero across a narrow band as the end's velocity along that direction reaches `max_velocity_mps`. A separate brake acts only on the speed above the limit. So an end at the limit is no longer pushed and braked on alternate steps. The band is 2 % of the limit, and at least two steps' change of velocity at the phase's force, 2 × `max_force_n` × (1/64 s) / mass. With `velocity_scales_with_sphere: true` the limit falls in proportion to the sphere's radius from the radius the phase was entered at.

**The magnitude rule.** The push keeps the magnitude of the composed direction, clamped to 1. A small proportional correction of the circle law or the spacing is a small force, not a full-force push in a new direction. The branch law's unit directions give full force as before.

**Transitions.** A phase's transitions check `distance_m` (the root's range to the target), `relative_speed_mps` (in the target's LVLH frame), `time_in_phase_s`, `closing_rate_mps` (how fast the root's range shrinks, positive while closing) and `tether_straightness` (the rope's largest distance from the root-to-tail line over the root-to-tail distance; 0 = straight). All conditions of a transition must hold ([Project files](./project-files#capture-plans-schema-v1)). The controller keeps running in an end phase.

### Controller gains

A phase's optional `controller` block sets the constants of the laws above for that phase. Each key is optional and defaults to the value the controller has always used. A plan without the block runs bit for bit as before.

| Key | Default | Rule | What it sets |
|---|---|---|---|
| `radius_hold_band_m` | 2.0 | greater than 0 | The circle law: the radius error at which the radial term w_r reaches full weight (the 2 m above). |
| `radial_rate_band_mps` | 0.1 | greater than 0 | The circle law: the outward speed against the sphere's shrink at which the rate term w_v reaches full weight (the 0.1 m/s above). |
| `spacing_band_m` | 3.0 | greater than 0 | The tail's spacing: the chord error at which the tail's tangent weight saturates (the 3 m above). |
| `spacing_rate_band_mps` | 0.3 | greater than 0 | The tail's spacing: the difference in the ends' tangent speeds at which the rate term saturates (the 0.3 m/s above). |
| `plane_hold_band_m` | 1.0 | greater than 0 | `hold_plane`: the height above or below the circling plane at which the push toward it reaches full weight. |
| `inner_edge_fraction` | 0.8 | greater than 0, less than 1 | The branch law's inner edge as a fraction of the sphere radius; inside it the end is pushed away from the target. |
| `dead_band_fraction` | 0.02 | at least 0, less than 0.5 | The hysteresis band on the branch law's two edges and the speed limit's band (at least two steps' change of velocity); 0 removes the hysteresis. |
| `force_ramp_s` | 1.0 | greater than 0 | Seconds the force bound takes to rise from 0 to `max_force_n` at the capture start and at every phase entry. |

A narrower band makes the law stiffer: full force for a smaller error. A wider one makes it gentler. In the plan form the gains are in each phase's collapsed **Controller** group, with the defaults as hints.

### The example plan

The shipped `example_plan` captures the Hubble debris in six phases:

| Phase | Limit, force | Sphere | Pushed | Leaves |
|---|---|---|---|---|
| `rendezvous` | 18 m/s, 25 N | 100 m | root | below 2,000 m |
| `approach` | 2 m/s, 3 N | | root | below 300 m |
| `final_approach` | 0.3 m/s, 3 N | | root | below 120 m and 0.35 m/s |
| `terminal` | 0.5 m/s, 2 N | shrinks 0.05 m/s | both ends, the tail 12 m behind, in the plane | below 5 m (back to `final_approach` above 120 m) |
| `capture` | 0.5 m/s, 2 N | shrinks 0.005 m/s to 4.5 m | both ends, the tail counter-circling, in the plane | after 90 s |
| `hold` (`end: success`) | brake to rest, 2 N | | both ends, in the plane | |

## Measured behaviour

On the reference Hubble scenario (`scenario_q13`, fixed epoch, macOS/aarch64), with the default rope:

| Measure | Value |
|---|---|
| 10 s after the capture start: fastest node relative to the root | 0.0426 m/s |
| the same: root-to-tail distance; tether–tether touching pairs | 20.0141 m (+0.07 %); 0 |
| the same: the chain's speed against F/m | −0.167 % |
| A 10-minute rest in R-bar: fastest node relative to the root | 0.0098 m/s |
| Terminal phase, sphere at least 20 m: the ends' separation, min / mean | 11.65 / 12.08 m |
| Terminal phase: root-to-tail touches while the rope turns onto the circle | 0 |
| Terminal phase: the root's radius error; its closing rate | +0.005 to +0.630 m; 0.014 to 0.073 m/s (the sphere shrinks at 0.05 m/s) |
| Terminal phase: force reversals, root / tail | 3 / 1 |
| First tether–debris contact | in the capture phase (none before) |
| Capture phase: the wrap | 1.25 turns, on the bus only |
| Hold phase: the ends' speed after the first minute | 0.003 m/s or less |
| Wall time per step (the test harness, Apple M1) | 0.50 ms |

The phase steps and the state hashes of this run are the reference goldens ([Determinism](./determinism#across-platforms)).

## Body count and recording size

A recording stores the target and every tether body: 78 bodies with the default rope, 9.0 KB per frame, 1.1 MB per simulated minute at the default frame interval ([Recordings](./recordings#sizes)). A plan whose tether has more than 1,000 bodies is refused, and one above 200 gets a warning.

## Physics bubble and floating origin

Only bodies near the simulation anchor (the tether root) are simulated by avian. The rest are propagated on their orbits ([Orbital Mechanics](./orbital-mechanics)). The bubble's origin follows the root on a free-falling orbit, so the bodies inside feel only the difference of Earth's gravity across the bubble. Because orbital distances are thousands of kilometres, the world origin shifts to follow the root ([Floating Origin](./floating-origin)).
