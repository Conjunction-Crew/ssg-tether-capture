---
sidebar_position: 5
---

# Capture Log

The Capture Log is a real-time terminal panel anchored to the bottom of the sim screen. It records structured log messages from the capture state machine, propagation system, and simulation setup, giving you a timestamped, filterable audit trail of everything that happens during a capture attempt.

## Opening the panel

By default the panel is collapsed. Click the **^** button in the header to expand the log viewport. The button label changes to **v** while the panel is open. Click again to collapse it.

## Log levels

Each entry is tagged with one of four severity levels:

| Label | Colour | Meaning |
|---|---|---|
| `ERR` | Red | A failure that prevented an operation from completing |
| `WARN` | Yellow | An unexpected condition that did not stop execution |
| `INFO` | White | A notable simulation event (state transitions, initialisation) |
| `DBG` | Grey | Verbose diagnostic output (physics bubble changes, force details) |

## Filtering by level

The header row contains four toggle buttons — one for each level. Click a button to hide entries of that level. An inactive (hidden) level is shown with a dimmed background. Click again to re-enable it.

Filter changes take effect immediately and re-render the visible rows without clearing the underlying buffer.

## Selecting and copying rows

- **Click** a row to select it (highlighted in blue).
- **Shift-click** a second row to extend the selection to a range.
- **Ctrl+A** (or **Cmd+A** on macOS) selects all visible rows.
- **Ctrl+C** (or **Cmd+C** on macOS) copies the selected rows to the system clipboard as plain text, one entry per line.

Each copied line has the format:

```
[HH:MM:SS] [LEVEL] source: message
```

## Auto-scroll

The viewport automatically scrolls to the newest entry when new messages arrive. If you manually scroll up, auto-scroll pauses so you can read earlier entries. It resumes when you scroll back to the bottom.

## Clearing the log

Click the **Clear** button in the header to empty the log buffer. This does not affect the simulation — only the displayed log entries are removed.

## Log sources

The `source` field identifies which subsystem emitted the entry:

| Source | Description |
|---|---|
| `sim` | Simulation setup and initialisation events (`setup_entities`, `setup_tether`) |
| `capture` | Capture state machine transitions and errors (`capture_state_machine_update`) |
| `propagation` | Orbital propagation events, physics bubble add/remove, dataset parse errors |
| `ui` | UI-driven events (capture plan loading, screen navigation) |

## Implementation details

The log is backed by a `CaptureLog` resource — a ring buffer capped at 256 entries. Older entries are evicted when the buffer is full. Simulation systems emit `LogEvent` messages which are collected each frame by the `collect_log_events` system and appended to the buffer with the current simulation timestamp.

Log entries persist across sim screen re-entries. Only the scroll position and row selection are reset when you navigate back to the sim screen.
