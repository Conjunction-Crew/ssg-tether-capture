---
sidebar_position: 5
---

# Capture Log

The Capture Log is a real-time terminal panel anchored to the bottom of the sim screen. It records log messages from the simulation (the capture controller, propagation, the physics bubble, the recorder) and from the app, giving you a timestamped, filterable audit trail of everything that happens during a capture attempt.

## Opening the panel

By default the panel is collapsed to its toolbar. Click the arrow at the left of **Capture Log** to open the log viewport, and again to collapse it. While it is collapsed, the filter buttons, **Clear** and **Save** are shown but disabled, and the panel cannot be resized; open, it can be dragged between a quarter and three quarters of the window's height.

## Log levels

Each entry is tagged with one of four severity levels:

| Label | Colour | Meaning |
|---|---|---|
| `ERR` | Red | A failure that prevented an operation from completing |
| `WARN` | Yellow | An unexpected condition that did not stop execution |
| `INFO` | White | A notable simulation event (state transitions, initialisation) |
| `DBG` | Grey | Verbose diagnostic output (physics bubble changes, force details) |

## Filtering by level

The toolbar has four toggle buttons, one per level. Click a button to hide entries of that level; a hidden level's label is greyed out. Click again to show them.

Filter changes take effect immediately and re-render the visible rows without clearing the underlying buffer.

## Selecting and copying rows

- **Click** a row to select it (highlighted in blue).
- **Shift-click** a second row to extend the selection to a range.
- **Ctrl+A** (or **Cmd+A** on macOS) selects all visible rows.
- **Ctrl+C** (or **Cmd+C** on macOS) copies the selected rows to the system clipboard as plain text, one entry per line.

Each copied line has the format:

```
[HH:MM:SS.mmm] [LEVEL] source: message
```

## Auto-scroll

The viewport keeps the newest entry in view. Scrolling up pauses this so you can read earlier entries; it resumes when a new entry arrives or a filter changes.

## Clearing the log

Click **Clear** to empty the log buffer. This does not affect the simulation — only the displayed log entries are removed.

## Saving the log

Click **Save** to write the buffer to `logs/<plan-id>_<sim-time>.jsonl` in the project folder. `<plan-id>` is the active capture plan's id (the plan file's name without `.json`; `unnamed` when no plan is active), with every character other than letters, digits, `-` and `_` replaced by `_`. `<sim-time>` is the simulation epoch when you clicked, with the same characters replaced by `-`. A line in the log says where the file went.

The file has one JSON object per entry, oldest first:

```json
{"timestamp":"16:03:47.002","epoch_utc":"2026-04-03T16:03:47.002464Z","level":"Info","source":"time","message":"Sim start epoch: …"}
```

| Field | Meaning |
|---|---|
| `timestamp` | Simulation time of day, `HH:MM:SS.mmm` (UTC, truncated to the millisecond), as the panel shows it |
| `epoch_utc` | Full simulation epoch, `YYYY-MM-DDTHH:MM:SS.ffffffZ` (UTC, microseconds), the format of `project.json` |
| `level` | `Error`, `Warn`, `Info` or `Debug` |
| `source` | The emitting subsystem (see below) |
| `message` | The text shown in the panel |

## Exporting data

The **Data Collection** window under the plots has two buttons. Both write into the `exports/` folder of the project (created when needed):

- **Export to CSV** proposes `<plan-id>-<date>_<time>.csv` (you can edit the name), then **Export** writes one row per sample with the columns `seconds_since_start_s` (simulation seconds since the start epoch), `range_m` (distance between the tether root and the target, m) and `rel_speed_mps` (the root's speed relative to the target in the target's LVLH frame, m/s). Before v0.4.0-rc.1 the columns were `seconds_since_start,rel_position,rel_velocity` and the file went to the project folder itself.
- **Export OEM** writes `<plan-id>-<date>_<time>.oem` at once: a CCSDS Orbit Ephemeris Message (OEM 2.0) with the target's and the chaser's (tether root's) states in the GCRF frame, one per simulated second of the running capture. It has the same layout as the `ephemeris.oem` of [`ssg-sim run`](../getting-started/headless-cli#output).

## Log sources

The `source` field identifies which subsystem emitted the entry:

| Source | Description |
|---|---|
| `time` | The sim start epoch |
| `propagation` | SGP4 failures, physics bubble enable/disable, floating-origin resets, the dataset object count, element parse errors |
| `dataset` | Reading the catalog dataset files in `assets/datasets/` |
| `tether` | Spawning the tether (for example, no start state for the chosen spawn orientation) |
| `capture` | The capture controller: phase transitions, the end of a capture, errors |
| `recording` | The recorder: start, stop, warnings, refusals |
| `replay` | Opening a recording for replay |
| `project` | Reading and writing `project.json` |
| `ui` | Actions in the app: capture start, time warp, view and camera-target changes, plan loading and saving |
| `system` | The panel's own **Save** |

## Implementation details

The log is backed by a `CaptureLog` resource — a ring buffer capped at 1,024 entries. When the buffer is full, the oldest DBG entry is evicted first; an ERR, WARN or INFO entry is evicted (oldest first) only when no DBG entry is left, so the phase transitions of a whole capture stay in the log. Simulation systems emit `LogEvent` messages which are collected each frame by the `collect_log_events` system and appended to the buffer with the current simulation timestamp.

Each sim session has its own log: the entries are cleared when you leave the sim screen (back to Home, or through a restart), so the panel and **Save** hold one session. Lines logged on Home after you leave stay and appear at the top of the next session's log. When you come back to the sim screen the panel starts collapsed and its level filters are kept; the scroll position and row selection are reset.
