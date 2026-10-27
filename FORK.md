# Bevy fork

This project builds against a small fork of Bevy 0.18. This file says what the
fork contains, why, and how to change it.

## What

The root `Cargo.toml` has a `[patch.crates-io]` table that redirects 26 Bevy
crates to the fork:

| | |
|---|---|
| Repository | `Conjunction-Crew/ssg-bevy` |
| Branch | `ssg-bevy-0.18.0` |
| Pinned commit (`rev`) | `f93dee9e299d46e2af62a8d753fa62227ee3c802` |
| Upstream base | tag `v0.18.0` = `5f8270f` |

The fork is `v0.18.0` plus three commits (10 files, +86/−45).

## Commits

| Commit | Subject | What it does |
|---|---|---|
| `3d2782f` | pull from #21834 | Adds `Atmosphere.world_position`, so the atmosphere centre can follow the floating origin (`crates/bevy_pbr/src/atmosphere/*`). |
| `67aaf7d` | directional lights only affected when rays genuinely pass through atmosphere | Applies atmospheric attenuation to a directional light only when the ray really crosses the atmosphere (`pbr_lighting.wgsl`). No upstream equivalent. |
| `f93dee9` | auto exposure bugfix for compensation curve | Fixes how the auto-exposure compensation-curve asset is used (`bevy_post_process/.../compensation_curve.rs`). |

## Why we need them

- **`Atmosphere.world_position`.** The simulation keeps the camera near the
  world origin and moves everything else (the floating origin). The Earth and
  its atmosphere therefore move in render space. The app writes the atmosphere
  centre in `floating_origin_update_visuals` and when the map view is toggled.
  Stock Bevy 0.18 always centres the atmosphere on the planet's origin, so the
  sky would be wrong away from it.
- **Directional-light attenuation.** Without the fix, the sun is dimmed as if
  every ray crossed the atmosphere, including for objects (satellites, tethers,
  debris) that sit outside it. That makes spacecraft look unlit.
- **Auto-exposure fix.** The scene camera uses auto exposure with a
  compensation curve. The unfixed asset handling breaks that curve.

## Upstream status

- Upstream PR #21834 (the source of `3d2782f`) was closed unmerged in
  December 2025. Bevy 0.19 and later place the atmosphere from the entity's
  `GlobalTransform`, which removes the need for `Atmosphere.world_position`.
- The directional-light fix has no upstream equivalent. Re-check it when
  migrating.
- Bevy is at 0.20.0; this project stays on 0.18.

## Policy

- The engine is frozen at 0.18 (high-level plan rule 9): no engine upgrades
  in the current reorganization.
- Edit the fork only when an in-scope item needs it. Commit on the
  `ssg-bevy-0.18.0` branch, bump the `rev` in the root `Cargo.toml`, and add a
  row to the Commits table above.
- Dependabot ignores `bevy`.

## How to update the pin

1. Replace the old SHA with the new one in all 26 `rev = "…"` entries:

   ```bash
   sed -i.bak 's/f93dee9e299d46e2af62a8d753fa62227ee3c802/<new-sha>/g' Cargo.toml && rm Cargo.toml.bak
   ```

2. Build once without `--locked` so `Cargo.lock` picks up the new commit, then
   review the diff (only `ssg-bevy` source strings should change):

   ```bash
   cargo build --workspace && git diff Cargo.lock
   ```

   Check that `grep -c 'rev = "<new-sha>"' Cargo.toml` prints 26.

## Migration

The plan to drop the fork is tracked in the follow-up issue "Migrate to Bevy
0.20 and drop the fork".
