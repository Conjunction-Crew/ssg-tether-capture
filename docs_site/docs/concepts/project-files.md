---
sidebar_position: 9
---

# Project files

:::note Draft
This page describes the file formats of v0.4.0-rc.2. It is a draft; the full documentation pass comes later.
:::

A project is a folder. Sharing a project means zipping the folder; opening one means pointing the application (or `ssg-sim`) at it. Every file the project holds is plain JSON, text or CSV (except a recording's keyframe table, which is Parquet), and every JSON file has a published [JSON Schema](#json-schemas) and an integer `schema_version`.

## The project folder

The application's working directory is the project folder (you pick it with **Select Working Directory** and can change it from the home screen; `ssg-sim run` and `ssg-sim validate` take it as an argument).

```text
<project>/
  project.json            the manifest (optional; a folder without one still loads)
  <plan-id>.json          capture plans; the file name without .json is the plan id
  datasets/*.json         Space-Track GP datasets the project carries (optional)
  exports/                CSV and OEM files exported from the application
  logs/                   Capture Log files
  runs/<utc-stamp>/       outputs of `ssg-sim run`
  recordings/<name>/      recorded segments (see below)
```

Nothing is moved or rewritten when a project is opened. A folder from an earlier version (plan files only) opens as it is; a plan file is written in the new format only when you save it again.

## The manifest, `project.json`

The manifest records what a run needs besides the plan: the start epoch, the active plan and the two bodies.

```json
{
  "schema_version": 1,
  "name": "Hubble capture study",
  "start_epoch_utc": "2026-04-03T16:03:47.002464Z",
  "active_plan": "example_plan",
  "target": {
    "label": "HST",
    "orbit": { "gp": { "OBJECT_NAME": "HST", "NORAD_CAT_ID": 20580, "...": "the Space-Track record" } },
    "source": { "dataset": "datasets/dataset_2026_04_03.json", "norad_id": 20580 },
    "mass_kg": 2500.0
  },
  "chaser": {
    "label": "P0 HST CHASER",
    "orbit": { "elements": {
      "semi_major_axis_m": 6914000.0, "eccentricity": 0.0002, "inclination_rad": 0.4969,
      "raan_rad": 1.747, "arg_perigee_rad": 1.4, "mean_anomaly_rad": 4.88,
      "epoch_utc": "2026-04-03T16:03:47.002464Z"
    } }
  },
  "built_with": { "app_version": "0.4.0-rc.1", "target": "aarch64-macos", "avian_features": ["simd", "parallel"] }
}
```

| Key | Meaning |
|---|---|
| `start_epoch_utc` | The simulation start, `YYYY-MM-DDTHH:MM:SS.ffffffZ` (UTC, microseconds). The simulation starts from exactly this string; it is also the format of every epoch in the exports. |
| `active_plan` | Id of the capture plan to run. Absent: the project's only plan, else `example_plan`. |
| `target`, `chaser` | A `label`, an `orbit` and optionally `source`, `mass_kg` and `inertia_kg_m2`. |
| `orbit.gp` | The Space-Track GP (OMM JSON) record copied verbatim. The body starts from SGP4, as a catalog pick does. The copy is the source of truth: a different dataset file never changes the run. |
| `orbit.elements` | Osculating Keplerian elements in **metres and radians** at `epoch_utc`. |
| `orbit.relative_to_target` | Chaser only: `preset` (`v_bar` or `r_bar`) and `offset_m`. Not applied yet (see below). |
| `source` | Where the orbit came from (dataset path and NORAD id). Provenance only; a missing dataset file is a warning. |
| `mass_kg` | Mass in kg. The target's is applied; the chaser's is not yet. |
| `built_with` | The build that wrote the file. Only `ssg-sim validate` reads it, to say when the build or platform differs: results are bit-identical only on the same build and platform ([Determinism](./determinism)). |
| `recording` | Optional: the project's recording settings ([below](#the-recording-block)). Absent: recordings start and stop only by hand. |

The chaser's `mass_kg`, both `inertia_kg_m2` and `orbit.relative_to_target` are part of the schema but refused with "applied from v0.4.0-rc.4" until the release that implements them. A file never silently simulates differently from what it says.

The application writes `project.json` when you start a simulation (keeping its `recording` block) and starts from what it wrote; opening the project restores the start epoch and the two bodies. `ssg-sim run <project>` reads the same file, so the same files give the same inputs in both ([Running without a window](../getting-started/headless-cli#running-from-projectjson)).

## Capture plans, schema v1

A capture plan describes the **phases** of a capture (the application used to call them states) and when the controller moves from one to the next. All lengths are metres, speeds m/s, forces N and masses kg: metric is the only stored unit, and the keys carry their unit.

```json
{
  "schema_version": 1,
  "name": "example_plan",
  "tether": "Tether1",
  "device": { "type": "tether", "tether_length_m": 20.0 },
  "phases": [
    { "id": "rendezvous", "max_velocity_mps": 25.0, "max_force_n": 25.0,
      "transitions": [ { "to": "approach", "all_of": { "distance_m": { "less_than": 2000.0 } } } ] },
    { "id": "capture", "max_velocity_mps": 0.5, "max_force_n": 2.0, "shrink_rate_mps": 0.005, "end": "success" }
  ]
}
```

| Key | Required | Default | Rule |
|---|---|---|---|
| `schema_version` | yes | none | `1`. A file without it is a v0 plan (see below). |
| `name` | yes | none | Display name; not blank. |
| `tether` | yes | none | Id the tether is registered under; not blank. |
| `device.type` | no | `"tether"` | Free text. |
| `device.tether_length_m` | no | 20.0 | Greater than 0. |
| `device.segment_spacing_m` | no | 0.1 | Greater than 0. The tether's body count (interior nodes plus root and tail) may not exceed 1,000; above 200 it is a warning, because recordings become large. |
| `device.segment_mass_kg` | no | 0.1 | Greater than 0. |
| `device.end_mass_kg` | no | 2.0 | Mass of the root and of the tail; greater than 0. |
| `device.joint_damping_linear`, `device.joint_damping_angular` | no | 1.0, 1.0 | At least 0. |
| `capture_sphere.initial_radius_m` | no | 25.0 | Greater than 0. |
| `phases` | yes | none | At least one; the capture starts in the first. Ids are not blank and unique. |
| `phases[].max_velocity_mps`, `phases[].max_force_n` | yes | none | At least 0. |
| `phases[].shrink_rate_mps` | no | none | At least 0. |
| `phases[].transitions[]` | no | none | `to` names a phase; `all_of` holds at least one condition. |
| `…all_of.distance_m`, `…all_of.relative_speed_mps` | no | none | `{ "less_than": …, "greater_than": … }` with at least one bound, each at least 0. `relative_speed_mps` is the magnitude of the relative velocity in the target's LVLH frame. |
| `phases[].end` | no | none | `"success"` or `"abort"`. An end phase has no transitions. |

The defaults are the values the simulation used before they became configurable, so a plan that sets none of them runs exactly as before.

**All conditions of a transition must hold** (logical AND). In each step at most one transition fires: the first one, in file order, whose conditions hold.

The `end` key marks a phase as the conclusion of the capture. When the capture enters an end phase, the simulation writes the log line `Capture ended: success (phase <id>)` (or `abort`) and an end event that a running recording uses to stop after its delay ([Recordings](./recordings#starting-and-stopping)). Nothing else changes: the controller keeps running in the end phase.

## Opening a v0 plan

A plan file without `schema_version` is a v0 plan. It loads exactly as it did, because it is converted to v1 in memory when read; the file itself is not touched until you save the plan, which writes v1.

| v0 | v1 |
|---|---|
| `states` | `phases` (same order) |
| `states[].parameters.max_velocity`, `max_force`, `shrink_rate` | `max_velocity_mps`, `max_force_n`, `shrink_rate_mps` |
| `device.tether_length` | `device.tether_length_m` |
| a transition's `distance` and `relative_velocity` | `all_of.distance_m` and `all_of.relative_speed_mps` |

The conversion loses nothing. A value the old format accepted but never used (`units`, `next`, `next_conditions`, `parameters.kp`, `device.segments`, a `chaser` or `target` block, …) is kept under `migrated_from_v0.unmapped` as the value together with its JSON Pointer in the old document, and a warning names it. A value that v0 ignored stays ignored: `"units": "ft"` keeps the number as metres, and the warning says that v1 values are always metric. Where a v1 field exists for it, the warning names the field to set instead (`device.segment_mass` → `device.segment_mass_kg`). A transition without a numeric bound could never fire in v0; it is removed from `transitions` and kept under `unmapped`.

A v0 file that was refused before is refused now, with a message that names the key and where it is. One rule is stricter: negative limits are no longer accepted (a negative force reversed the push).

## Validation

Every rule above is checked when a plan or manifest is read, and each problem is reported in field order as

```text
[<plan-id>] /phases/2/max_force_n: must not be negative
[project.json] /target/mass_kg: must be greater than 0
```

(file or plan id, JSON Pointer, message). A file with a problem does not load; the other files of the project are not affected. A wrong type is reported where it is, an unknown key with its name. `ssg-sim validate` runs the checks over a whole folder ([below](#checking-a-project-from-the-command-line)).

## Versions and compatibility

- Every file type carries an integer `schema_version`: plans, `project.json`, the recording manifest and `config.json` (a config without it is read as before).
- A reader **refuses** a `schema_version` it does not know, and refuses **unknown keys**, naming the key. Refusing is deliberate: a file that is read with a part ignored could simulate something other than what it says.
- Inside 0.4.x a schema only grows by **optional** keys whose absence reproduces the earlier behaviour. New keys can still appear while v0.4.0 is in its release-candidate line, so a file written by a later candidate can be refused by an earlier one, with the key named. From v0.4.0 on, patch releases add no keys, so a file written by any 0.4.x opens in every 0.4.y.
- A change that breaks this needs `schema_version` 2 and a conversion from 1.

## Exports

| File | Written by | Contents |
|---|---|---|
| `runs/<stamp>/trajectory.csv` | `ssg-sim run` | `step,epoch_utc,phase,range_m,rel_speed_mps` |
| `runs/<stamp>/summary.json` | `ssg-sim run` | A report of the run (`final.phase`, the transitions, `state_hash`); it belongs to the command line and has no schema. |
| `runs/<stamp>/ephemeris.oem` | `ssg-sim run` | CCSDS OEM 2.0: target and chaser, GCRF, UTC |
| `exports/<plan-id>-<stamp>.csv` | **Export to CSV** | `seconds_since_start_s,range_m,rel_speed_mps` |
| `exports/<plan-id>-<stamp>.oem` | **Export OEM** | The same layout as `ephemeris.oem`, one state per simulated second |
| `logs/<plan-id>_<stamp>.jsonl` | **Save** in the Capture Log | One JSON object per entry, with `timestamp` and `epoch_utc` |

Units: `range_m` is the distance between the tether root and the target in metres, `rel_speed_mps` the root's speed relative to the target in the target's LVLH frame in m/s, `seconds_since_start_s` simulation seconds since the start epoch. Every `epoch_utc` is `YYYY-MM-DDTHH:MM:SS.ffffffZ`, the format of `project.json`. File names are sanitised: every character other than letters, digits, `-` and `_` becomes `_`.

An OEM file has `CCSDS_OEM_VERS = 2.0`, one segment for the target and one for the chaser (the tether root), `CENTER_NAME = EARTH`, `REF_FRAME = GCRF`, `TIME_SYSTEM = UTC`, positions in km with six decimals (1 mm) and velocities in km/s with nine decimals. `CREATION_DATE` is the start epoch of the run, so the same run writes the same bytes. See [Running without a window](../getting-started/headless-cli#output) and the [Capture Log](./capture-log).

## Recordings

A recording stores a segment of a running simulation; what it holds, how it starts and stops, and how it is played and checked is described in [Recordings](./recordings). A recording folder is written as `recordings/<name>.partial/` and renamed when it is complete:

```text
recordings/<name>/
  manifest.json      the recording manifest (schema v1)
  inputs.jsonl       the input log from step 0, one JSON object per line: {"step": <u64>, "command": …}
  keyframes.parquet  one row per body per frame
  frames.jsonl       one line per frame: step, epoch, phase, sphere radius, frame origin, origin shift
  transitions.jsonl  one line per capture transition in the segment
  log.jsonl          the segment's log events
```

**`inputs.jsonl`** holds every input that affects the simulation, from step 0 of the session (also before the recording started), ordered by step. The first line is the number of fixed steps the spawn update ran, `{"step": k, "command": {"session_start": {"steps_before_propagators": k}}}`; each capture start follows as `{"step": <the step it was applied after>, "command": {"start_capture": {"target": "target", "plan_id": "<id>"}}}`.

**`keyframes.parquet`** has one row per body per frame, with the columns `step` (INT64), `body` (INT32, an `index` of the manifest's body table) and, as DOUBLE, `position_m_x`, `position_m_y`, `position_m_z`, `rotation_x`, `rotation_y`, `rotation_z`, `rotation_w`, `velocity_mps_x`, `velocity_mps_y`, `velocity_mps_z`, `angular_velocity_radps_x`, `angular_velocity_radps_y`, `angular_velocity_radps_z`. The values are in the simulation's local frame and are stored exactly (plain encoding, no compression), 64 frames per row group with a shorter last one. A frame is taken at `start_step`, every `keyframe_interval_steps` after it, and at `end_step`. Later releases may add columns; the columns above keep their meaning.

**`frames.jsonl`** has one line per frame, in the order of the table:

```json
{"step":162547,"epoch_utc":"2026-04-03T16:46:06.799339Z","phase":"capture","capture_sphere_radius_m":4.99960937501819,"frame_origin_eci":[5688404.548055969,3714844.426850873,1004419.4278612267,-3269.920460524945,5952.755114869788,-3442.3836914584326],"origin_shift_m":[0.0,0.0,0.0],"origin_shift_mps":[0.0,0.0,0.0]}
```

`frame_origin_eci` is the ECI position (m) and velocity (m/s) of the local frame's origin. `origin_shift_m` and `origin_shift_mps` are the sum of the floating-origin resets since the previous frame (zero in the first frame and in most others): every body's local state moved by minus that shift. Numbers are written so that they read back exactly.

**`transitions.jsonl`** has one line per capture transition inside the segment: `{"step", "epoch_utc", "from", "to", "reason"}`. **`log.jsonl`** has the segment's log events in the format of a saved [Capture Log](./capture-log).

The manifest says what was recorded and how to read the rest:

| Key | Meaning |
|---|---|
| `schema_version` | `1` |
| `built_with` | The build that recorded (`app_version`, `target`, `avian_features`). A recording re-simulates bit for bit only on the machine that recorded it ([Recordings](./recordings#what-re-simulation-promises)). |
| `plan_id`, `plan` | The id of the capture plan and a copy of it (schema v1) as it was when the recording started. |
| `start_step`, `start_epoch_utc` | The simulation step and epoch of the first frame. |
| `keyframe_interval_steps` | Steps between two frames (K); at least 1. |
| `bodies` | The recorded bodies: `index` (the position in the list, also the `body` column), `role` (`target`, `tether_root`, `tether_node` or `tether_tail`) and `name`. |
| `limits` | `max_duration_s` (the longest segment in simulation seconds), `max_tether_bodies` (every body except the target) and, optionally, `max_size_mb` (greater than 0). |
| `project` | Optional: the scenario at the start (`project.json` as the run used it). Absent: the recording plays but cannot be re-simulated. |
| `source` | Optional: the program that wrote it, `app` or `cli`. A recording whose input log lacks its first line can be re-simulated only when `source` is `cli`. |
| `end_step` | Optional: the step of the last frame; at least `start_step`. |
| `end_reason` | Optional: `manual`, `end_success`, `end_abort`, `max_duration`, `max_size` or `session_end`. |
| `start_trigger`, `start_phase` | Optional: `manual` or `phase`; `start_phase` (the phase that started it) is present exactly when `start_trigger` is `phase`. |

The recording budget is sized for tethers of up to 200 bodies; the default 20 m tether has 192.

### The `recording` block

`project.json` may carry the project's recording settings. Every key is optional and falls back to its default; a project without the block records only by hand.

```json
"recording": {
  "auto_start_phase": "capture",
  "auto_stop_on_end": true,
  "stop_after_end_s": 60,
  "keyframe_interval_steps": 32,
  "max_duration_s": 1800,
  "max_size_mb": 100,
  "max_tether_bodies": 200
}
```

| Key | Default | Rule |
|---|---|---|
| `auto_start_phase` | none (no automatic start) | Start a recording when the capture enters this phase. A name that is not a phase of the active plan is a warning. |
| `auto_stop_on_end` | `true` | Stop `stop_after_end_s` after the capture enters an end phase. |
| `stop_after_end_s` | 60 | Simulated seconds recorded after the end event; greater than 0. |
| `keyframe_interval_steps` | 32 | Steps between two frames (K); at least 1. |
| `max_duration_s` | 1800 | The longest recording in simulated seconds; greater than 0. |
| `max_size_mb` | 100 | The largest keyframe table in MB; greater than 0. |
| `max_tether_bodies` | 200 | A tether with more bodies is not recorded; at least 1. |

The application's Recording window edits these settings and **Save** writes them into `project.json`. `ssg-sim run --record` reads them too ([Running without a window](../getting-started/headless-cli#recording-a-run)).

## Checking a project from the command line

```bash
ssg-sim validate my-project
```

reads `project.json`, every plan file and every recording under `recordings/` and prints one `error:`, `warning:` or `info:` line per finding, then the totals; the exit code is 0 without errors and 1 with errors. See [Running without a window](../getting-started/headless-cli#checking-a-project).

## JSON Schemas

The schemas are generated from the types the application reads, so they cannot drift from the code (a test compares them with the committed files). Editors that understand JSON Schema can use them for completion and checking:

| File | Schema |
|---|---|
| Capture plan | [`capture-plan.v1.schema.json`](https://conjunction-crew.github.io/ssg-tether-capture/schemas/capture-plan.v1.schema.json) |
| `project.json` | [`project.v1.schema.json`](https://conjunction-crew.github.io/ssg-tether-capture/schemas/project.v1.schema.json) |
| Recording manifest | [`recording-manifest.v1.schema.json`](https://conjunction-crew.github.io/ssg-tether-capture/schemas/recording-manifest.v1.schema.json) |

Readers refuse unknown keys, so a file must not carry a `$schema` key of its own; attach the schema in your editor instead (in Visual Studio Code, with the `json.schemas` setting).
