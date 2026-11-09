---
sidebar_position: 6
---

# The Bevy fork

The app builds against a small fork of Bevy 0.18, not the crates.io release. This page summarises why and how to change it. The authoritative record is [`FORK.md`](https://github.com/Conjunction-Crew/ssg-tether-capture/blob/main/FORK.md) in the repository root.

## What is pinned

The root `Cargo.toml` points 26 Bevy crates (`bevy` and its sub-crates) at the fork with a `[patch.crates-io]` table:

| | |
|---|---|
| Repository | [`Conjunction-Crew/ssg-bevy`](https://github.com/Conjunction-Crew/ssg-bevy) |
| Branch | `ssg-bevy-0.18.0` |
| Pinned commit (`rev`) | `f93dee9e299d46e2af62a8d753fa62227ee3c802` |
| Upstream base | tag `v0.18.0` |

The fork is `v0.18.0` plus three commits.

## The three commits and why they are needed

| Commit | What it does | Why |
|---|---|---|
| `3d2782f` | Adds `Atmosphere.world_position`, so the atmosphere's centre can be placed. | The app keeps the camera near the world origin and moves everything else ([Floating Origin](../concepts/floating-origin)), so the Earth moves in render space. The app writes the atmosphere centre in `floating_origin_update_visuals` and when the map view is toggled. Stock Bevy 0.18 always centres the atmosphere on the planet's origin, and the sky would be wrong away from it. |
| `67aaf7d` | Attenuates a directional light only when its ray really crosses the atmosphere. | Without it the sun is dimmed as if every ray crossed the atmosphere, including for spacecraft, tethers and debris outside it, which then look unlit. |
| `f93dee9` | Fixes how the auto-exposure compensation-curve asset is used. | The scene camera uses auto exposure with a compensation curve, which the unfixed asset handling breaks. |

Upstream: the pull request the first commit came from was closed unmerged; Bevy 0.19 and later place the atmosphere from the entity's `GlobalTransform`, which makes `Atmosphere.world_position` unnecessary. The directional-light fix has no upstream equivalent and must be re-checked on any upgrade.

## Policy

- The engine stays on Bevy 0.18 until the upgrade below.
- The fork is edited only when a change in scope needs it: commit on the `ssg-bevy-0.18.0` branch, bump the `rev` in the root `Cargo.toml`, and add a row to the commits table in `FORK.md`.
- Dependabot ignores the `bevy*` crates (`.github/dependabot.yml`), so the pin never moves on its own.

## Updating the pin

1. Replace the old SHA with the new one in all 26 `rev = "…"` entries of the root `Cargo.toml`.
2. Build once without `--locked`, so `Cargo.lock` picks up the new commit, and review the lock-file diff: only the `ssg-bevy` source strings should change.
3. Check that the new SHA appears 26 times (`grep -c 'rev = "<new-sha>"' Cargo.toml`).

`FORK.md` has the exact commands.

## The upgrade path

Moving to Bevy 0.20 and dropping the fork is tracked in [#85, "Migrate to Bevy 0.20 and drop the fork"](https://github.com/Conjunction-Crew/ssg-tether-capture/issues/85). The atmosphere commit becomes unnecessary there; the directional-light fix must be re-checked against the new release.
