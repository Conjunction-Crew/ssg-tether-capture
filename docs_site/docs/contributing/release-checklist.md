---
sidebar_position: 7
---

# Release checklist

Walk this list before a release candidate is promoted, on the macOS DMG built from the release branch: a run of the **Packaging Test** workflow (artifact `macos`, file `ssg-tether-capture-test-<run>-macos-arm64.dmg`) or the candidate's release asset (`ssg-tether-capture-v<version>-macos-arm64.dmg`). It takes about an hour, plus the time the capture needs at the warp your Mac reaches.

Record every line as **Match** (you saw what is expected) or **Diff** (say what you saw). A Diff in the app's behaviour blocks the release until it is fixed or accepted; a Diff in a page is fixed in the docs. The [Release Workflow](./release-workflow) page describes the release itself.

## Before you start

- An empty working directory you can delete afterwards, for example `~/ssg-release-check`. Below it is `<wd>`.
- A copy of your app config, `~/Library/Application Support/ssg-tether-capture/config.json`, to restore afterwards.
- A clone of the repository at the release branch, for the command-line lines: `ssg-sim` is not in the installers and is built from source ([Running without a window](../getting-started/headless-cli)).

## 1 · Install and first launch

| # | Action | Expected |
|---|---|---|
| 1.1 | Open the DMG, drag the app to Applications, launch it | macOS may block the unsigned build: **Privacy & Security → Open Anyway** ([Installing on macOS](../getting-started/install-macos)). The window opens on **Select Working Directory**. |
| 1.2 | Read the path field | The last folder you used, or `~/ssg-tether-capture/project1` when there is no config. |
| 1.3 | Type `/System/release-check`, press Enter | You stay on the screen, with the error under the field. |
| 1.4 | Type `<wd>`, press Enter | Home opens. |

## 2 · Home and the plan form

| # | Action | Expected |
|---|---|---|
| 2.1 | Read Home | **Working Directory** with `<wd>` and **Change Directory**; **My Capture Plans** with no cards and **+ New Plan**; **Example Capture Plans** with one card, **example_plan**, without an edit button; no warning banner. |
| 2.2 | **+ New Plan**; leave Plan Name empty; **Save** | Title **New Capture Plan**; sections General, Tether, Capture Sphere and six phases (`rendezvous`, `approach`, `final_approach`, `terminal`, `capture`, `hold`) with the example plan's values. Save shows an error under Plan Name and writes nothing. |
| 2.3 | Click **ft (imperial)**, read Tether Length; click **m (metric)** | 65.6167979 ft, then 20.0 again. |
| 2.4 | Plan Name `Release Check`; **Phase · approach** → Max Force `4`; **Phase · terminal** → **Controller** → Dead Band Fraction `0.03`; **Save** | The form closes and a **Release Check** card appears. `<wd>/release_check.json` has `"schema_version": 1`, `max_force_n` 4.0 in `approach` and `"controller": {"dead_band_fraction": 0.03}` in `terminal`. |
| 2.5 | `shasum <wd>/release_check.json`; **edit** on the card; check both values; **Save** without a change; `shasum` again | Both values are shown; the form closes; the hash is unchanged. |
| 2.6 | **+ New Plan**, Plan Name `Release Check`, **Save** | **Overwrite Existing Plan?** with the file's path; **Cancel** leaves the file as it was. Close the form with **× Cancel**. |

## 3 · A project: Hubble and a chaser 30 m ahead

| # | Action | Expected |
|---|---|---|
| 3.1 | Click **example_plan** | The sim screen in map view. Project Information shows "Sim start epoch"; **Start Simulation** is dimmed; both orbit cards read **Select**; Capture Guidance shows a capture sphere radius of 100.00 m. |
| 3.2 | Target **Select**; search `20580`; click **HST**; **Use Selected** | "Target: Catalog: HST · NORAD 20580 · 1990-037B" and a **Clear** button; the chaser's third row is enabled. |
| 3.3 | Chaser third row: **V-bar**, offset `30`, **Relative to Target** | "Chaser: V-bar +30 m from the target"; **Start Simulation** is enabled. |
| 3.4 | **Start Simulation**; `cat <wd>/project.json` | The Time Warp section appears and the HUD's sim time advances. The file has `"active_plan": "example_plan"`, the target's `gp` record (HST), the chaser's `relative_to_target` with `v_bar` and `30.0`, and `built_with.app_version` = the version under test. |
| 3.5 | `M`; `Tab` twice; `O` twice; `M` | Scene view with the tether and the target; `Tab` moves the camera to the next body; `O` shows and hides the origin axes and the physics-bubble sphere; `M` returns to the map. |

## 4 · Run to capture

| # | Action | Expected |
|---|---|---|
| 4.1 | **Capture** | Capture status "Engaged (…)", and the log line `Capture initiated …`. The **Data Collection** window opens with two plots. From 30 m the capture is in `terminal` within a few steps. |
| 4.2 | Press `.` until the readout stops rising | The readout doubles (2x, 4x, …) and shows the warp actually running; at your Mac's limit the log says `Time warp → Nx (machine limit)`. |
| 4.3 | Let it run until Capture Guidance shows `hold` | The log has `State: terminal → capture (…)`, then `State: capture → hold (…)` and `Capture ended: success (phase hold)`. No NaN anywhere in the HUD. |
| 4.4 | Press `,` until **Paused**, then `.` once | Sim time stops while paused; one `.` resumes at 1x. |

## 5 · Log and exports

| # | Action | Expected |
|---|---|---|
| 5.1 | Open the **Capture Log** (the arrow); turn **DBG** off and on; **Save** | The filter hides and shows the debug lines; the log says `Log saved to logs/example_plan_<stamp>.jsonl`, and the file has one JSON object per line. |
| 5.2 | Data Collection → **Export to CSV** → **Export** | `<wd>/exports/example_plan-<stamp>.csv` with the header `seconds_since_start_s,range_m,rel_speed_mps`. |
| 5.3 | **Export OEM** | `<wd>/exports/example_plan-<stamp>.oem`, starting `CCSDS_OEM_VERS = 2.0`, with a segment for HST and one for the chaser. |

## 6 · Record and replay

| # | Action | Expected |
|---|---|---|
| 6.1 | **Reset Sim**; in the **Recording** window **⏺ Record**; **Capture**; warp to 8x; after about a simulated minute **⏹ Stop** | The sim restarts at 1x with the same selection. The log says `Recording started at step …` and `Recording stopped at step …`; `<wd>/recordings/example_plan-<UTC time>/` holds `manifest.json`, `keyframes.parquet`, `frames.jsonl`, `inputs.jsonl`, `transitions.jsonl` and `log.jsonl`. |
| 6.2 | **Back → Exit Sim**; click **example_plan** again | The chaser card reads "Chaser: V-bar +30 m from the target" again (restored from `project.json`); the **Recordings** window lists the recording with its duration, bodies and size. |
| 6.3 | **Open**; in **Transport** play, pause, step a frame, jump to the end, change the speed, scrub; **Exit replay** | The replay follows the tether root; steps land on frames; the step, epoch, phase and range update; the capture button and the warp keys do nothing. **Exit replay** leaves the replay. |

## 7 · Command line

From the repository root, at the release branch:

| # | Action | Expected |
|---|---|---|
| 7.1 | `cargo run --release --locked -p ssg-sim-cli -- validate <wd>` | A totals line with `0 errors`; exit code 0. |
| 7.2 | `cargo run --release --locked -p ssg-sim-cli -- run <wd> --steps 6400` | `<wd>/runs/<stamp>/` with `summary.json`, `trajectory.csv` and `ephemeris.oem`; exit code 0. |
| 7.3 | `cargo run --release --locked -p ssg-sim-cli -- replay <wd>/recordings/<name>` | `built with: this build` and every frame identical; exit code 0. |

## 8 · After the release

| # | Action | Expected |
|---|---|---|
| 8.1 | Open the [releases page](https://github.com/Conjunction-Crew/ssg-tether-capture/releases) | The new release is **Latest**, with a `.dmg`, an `.msi` and a `.tar.gz`. |
| 8.2 | Open the docs site's `/docs/` | This documentation's home page; the version menu's default is the new version (for an `X.Y.0` release). |
| 8.3 | Open `/docs/plan-reference/capture-plan-schema/` and `/schemas/capture-plan.v1.schema.json` | The schema page, and the JSON file. |
| 8.4 | On a phone in dark mode, open the docs and tap the menu | The sidebar opens with the docs, the versions and GitHub. |

Afterwards, restore your config and delete `<wd>`.
