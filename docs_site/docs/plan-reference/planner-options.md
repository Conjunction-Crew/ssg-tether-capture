---
title: Planner options
sidebar_position: 2
---

# Planner options

Every option a researcher sets for a capture, in one table: the scenario in the project's `project.json`, and the capture plan in `<plan-id>.json`. Units are the stored units: files are always metric. In the app, lengths, speeds and forces follow the **Metric / Imperial** toggle, and masses show in kg or lb ([Running the App](../getting-started/running-the-app#the-simulation-screen)). The keys are described in full on [Capture plan schema (v1)](./capture-plan-schema) and [Project files](../concepts/project-files).

"In the app" says where the option is set: on the sim screen (**Orbital Selection**, the **Recording** window) or in the plan form (**+ New Plan** or **edit** on Home, **View / Edit Plan** on the sim screen), by section and field. "File only" means the app has no control for it; edit the file.

| File | Key | Unit | Default | Range | In the app | What it does |
|---|---|---|---|---|---|---|
| project | `start_epoch_utc` | UTC | The wall clock when a folder without `project.json` is opened | `YYYY-MM-DDTHH:MM:SS.ffffffZ` | Shown in Project Information as "Sim start epoch"; file only | The simulation start. Every run starts from exactly this string, so a fixed epoch makes a run repeatable ([Determinism](../concepts/determinism)). |
| project | `active_plan` | | The project's only plan, else `example_plan` | A plan id of the project or an example plan | The plan card you open on Home (Start writes it) | Which capture plan runs. |
| project | `target.orbit`, `chaser.orbit` | m, rad | Required | `gp`: a Space-Track record. `elements`: semi-major axis greater than 0, eccentricity at least 0 and below 1 | Orbital Selection: **Select**, click an object on the map, **Use Selected** (`gp`); or **Create Custom** and the six element sliders (`elements`) | Where each body starts: from SGP4 for a catalog record, from osculating Keplerian elements otherwise. |
| project | `chaser.orbit.relative_to_target` | m | In the app: V-bar, +30 m | `preset` `v_bar` or `r_bar`; `offset_m` not 0, positive ahead (V-bar) or above (R-bar). A start the orbit propagator would move by more than 1 mm is refused | The chaser card's third row: **V-bar** / **R-bar**, the offset, **Relative to Target** (once the target has an orbit) | Places the chaser at rest relative to the target, on a line of the target's LVLH frame at the start epoch ([A chaser relative to the target](../concepts/project-files#a-chaser-relative-to-the-target)). |
| project | `target.mass_kg` | kg | 2,500 | Greater than 0 | File only | The debris's mass. |
| project | `chaser.mass_kg` | kg | The plan's `device.end_mass_kg` | Greater than 0 | File only | The tether root's mass (the root is the chaser). |
| project | `target.inertia_kg_m2`, `chaser.inertia_kg_m2` | kg m² | The collider's inertia scaled to the mass | Row-major 3 × 3; finite, symmetric, positive definite | File only | Inertia in the simulation's ECI-aligned axes at spawn. The chaser's has no effect while the root's rotation is locked. |
| project | `recording.auto_start_phase` | | None: recording by hand only | A phase id of the active plan | Recording window → Settings → **Start at phase** | Starts a recording when the capture enters this phase. |
| project | `recording.auto_stop_on_end` | | true | | **Stop after the end event** | Stops the recording `stop_after_end_s` after the capture enters an end phase. |
| project | `recording.stop_after_end_s` | s | 60 | Greater than 0 | **Seconds after the end** | Simulated seconds recorded after the end event. |
| project | `recording.keyframe_interval_steps` | steps | 32 | At least 1 | **Frame every (steps)** | Steps between two recorded frames; smaller is finer and larger ([Sizes](../concepts/recordings#sizes)). |
| project | `recording.max_duration_s` | s | 1,800 | Greater than 0 | **Longest (s)** | The longest recording. |
| project | `recording.max_size_mb` | MB | 100 | Greater than 0 | **Largest (MB)** | The largest keyframe table. |
| project | `recording.max_tether_bodies` | | 200 | At least 1 | **Most tether bodies** | A tether with more bodies is not recorded. |
| plan | `name` | | Required | Not blank | General → **Plan Name** | Display name; the file name is made from it. |
| plan | `tether` | | Required | Not blank | Tether → **Tether Name** | Id the tether is registered under. |
| plan | `device.type` | | `"tether"` | Free text | Tether → **Tether Type** (Tether; Net is not available yet) | The device kind. |
| plan | `device.tether_length_m` | m | 20.0 | Greater than 0 | Tether → **Tether Length** | The rope's physical length. |
| plan | `device.segment_spacing_m` | m | 0.25 | Greater than 0; at most 1,000 tether bodies (a warning above 200) | Tether → **Segment Spacing** | Distance between rope nodes, so the body count. |
| plan | `device.segment_mass_kg` | kg | 0.25 | Greater than 0 | Tether → **Segment Mass** | Mass of each interior node. |
| plan | `device.end_mass_kg` | kg | 2.0 | Greater than 0 | Tether → **End Mass** | Mass of the tail, and of the root unless `chaser.mass_kg` is set. |
| plan | `device.joint_damping_linear` | | 1.0 | At least 0 | Tether → **Linear Damping** | The rope's viscosity when slack. |
| plan | `device.joint_damping_angular` | | 1.0 | At least 0 | Tether → **Angular Damping** | No effect on the rope model; kept so older plans load. |
| plan | `device.spawn_orientation` | | `toward_target` | `toward_target`, `v_bar`, `r_bar` | Tether → **Spawn Orientation** | How the rope is laid out at spawn: away from the target along the chaser–target line, behind the chaser along its velocity, or below it toward the Earth. A spawn condition, not a held attitude. |
| plan | `capture_sphere.initial_radius_m` | m | 25.0 | Greater than 0 | Capture Sphere → **Initial Radius** | The capture sphere's radius at the start of every capture. |
| plan | `capture_sphere.min_radius_m` | m | 0.1 | Greater than 0 and less than `initial_radius_m` | Capture Sphere → **Min Radius** | The radius the sphere stops shrinking at. |
| plan | `phases[].id` | | Required | Unique, not blank | The section title **Phase · id** (read-only) | The phase's name; transitions point at it. The capture starts in the first phase. |
| plan | `phases[].max_velocity_mps` | m/s | Required | At least 0 | Phase → **Max Velocity** | The speed limit of the pushed tether ends. |
| plan | `phases[].max_force_n` | N | Required | At least 0 | Phase → **Max Force** | The bound on the force applied to each pushed end. |
| plan | `phases[].shrink_rate_mps` | m/s | None: no shrink | At least 0 | Phase → **Shrink Rate** | How fast the capture sphere shrinks; a phase with it runs the circle law ([Tether System](../concepts/tether-system#the-push-in-each-phase)). |
| plan | `phases[].end` | | None | `success`, `abort` | Phase → **End** | Marks an end phase: the capture ends there (log line `Capture ended: …`), and a running recording stops after its delay. An end phase has no transitions. |
| plan | `phases[].actuation` | | `circle` | `circle`, `hold` | Phase → **Actuation** | `circle`: the branch law, or the circle law with a shrink rate. `hold`: brake every pushed end to rest relative to the target and hold the plane. |
| plan | `phases[].tail_actuation` | | false | | Phase → **Tail Actuation** | Push the tail as well as the root. In a phase whose id is `capture`, the tail circles the other way, which wraps the rope. |
| plan | `phases[].tail_spacing_m` | m | None | Greater than 0 and less than `device.tether_length_m` | Phase → **Tail Spacing** | The root-to-tail chord held, the tail behind the root, in a phase that pushes the tail (not in a phase whose id is `capture`, nor with `hold` actuation). |
| plan | `phases[].hold_plane` | | false | | Phase → **Hold Plane** | Pushes the ends toward the circling plane through the target, in proportion to their height, full at `plane_hold_band_m`. |
| plan | `phases[].velocity_scales_with_sphere` | | false | | Phase → **Speed Scales with Sphere** | The speed limit falls in proportion to the sphere's radius from the radius the phase was entered at. No effect without a shrink rate. |
| plan | `phases[].controller.radius_hold_band_m` | m | 2.0 | Greater than 0 | Phase → **Controller** (collapsed) → **Radius Hold Band** | Circle law: the radius error at which the term that holds an end on its sphere reaches full weight. |
| plan | `phases[].controller.radial_rate_band_mps` | m/s | 0.1 | Greater than 0 | Controller → **Radial Rate Band** | Circle law: the outward speed against the sphere's shrink at which the rate term reaches full weight. |
| plan | `phases[].controller.spacing_band_m` | m | 3.0 | Greater than 0 | Controller → **Spacing Band** | The tail's lag law: the chord error at which the tail's tangent weight saturates. |
| plan | `phases[].controller.spacing_rate_band_mps` | m/s | 0.3 | Greater than 0 | Controller → **Spacing Rate Band** | The tail's lag law: the difference in the ends' forward speeds at which the rate term saturates. |
| plan | `phases[].controller.plane_hold_band_m` | m | 1.0 | Greater than 0 | Controller → **Plane Hold Band** | `hold_plane`: the height at which the push toward the plane reaches full weight. |
| plan | `phases[].controller.inner_edge_fraction` | | 0.8 | Greater than 0 and less than 1 | Controller → **Inner Edge Fraction** | The branch law's inner edge as a fraction of the sphere radius; inside it an end is pushed away from the target. |
| plan | `phases[].controller.dead_band_fraction` | | 0.02 | At least 0 and less than 0.5 | Controller → **Dead Band Fraction** | The relative width of the hysteresis band on the branch law's edges and of the velocity kill band (at least two steps' Δv); 0 removes the hysteresis. |
| plan | `phases[].controller.force_ramp_s` | s | 1.0 | Greater than 0 | Controller → **Force Ramp** | Seconds the force bound takes to rise from 0 to `max_force_n` at the capture start and at every phase entry. |
| plan | `phases[].transitions[].to` | | Required | A phase id of the plan | Phase → Transitions → **+ Add Transition** → **To Phase** | The phase to enter. In each step the first transition, in file order, whose conditions all hold fires. |
| plan | `…all_of.distance_m` | m | | `less_than` and/or `greater_than`, each at least 0 | Transition → **+ Condition** → **Distance** | Range between the tether root and the target. |
| plan | `…all_of.relative_speed_mps` | m/s | | As above | **+ Condition** → **Relative Speed** | Magnitude of the root's velocity relative to the target in the target's LVLH frame. |
| plan | `…all_of.time_in_phase_s` | s | | As above | **+ Condition** → **Time in Phase** | Seconds since the current phase was entered. |
| plan | `…all_of.closing_rate_mps` | m/s | | As above | **+ Condition** → **Closing Rate** | The rate at which the root's range to the target shrinks, positive while closing; does not hold while the range is 0. |
| plan | `…all_of.tether_straightness` | | | As above | **+ Condition** → **Tether Straightness** | How far the rope bows from the straight root-to-tail line, as a fraction of the root-to-tail distance (0 = straight); `less_than: 0.15` holds while no body is farther than 15 % of that distance from the line. |

All conditions of a transition must hold (AND). A transition needs at least one condition, and a condition at least one bound; both bounds are strict.

A key left out takes its default, and so does an empty field in the form: the form shows the default as the field's grey hint and does not write the key. Saving a plan keeps every key the form does not show.

## Not yet available

These are planned after v0.4.0 and tracked in the project's [GitHub issues](https://github.com/Conjunction-Crew/ssg-tether-capture/issues):

- **Adding, removing, reordering or renaming phases in the form.** The form edits the phases a plan has; to change the list, edit the plan file (the form keeps what it does not show).
- **Abort triggers.** An `abort` end phase is reached only through an ordinary transition built from the five conditions above; there are no dedicated abort conditions yet.
- **A documented guidance law.** The capture controller is a bounded, ramped heuristic with the per-phase gains above, not a guidance law with published equations.
- **An approach phase based on relative orbital dynamics** (Hill–Clohessy–Wiltshire), for example a computed intercept from a distant start.
