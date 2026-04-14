---
sidebar_position: 5
---

# Floating Origin

Orbital distances are enormous — low Earth orbit is roughly 400 km altitude, geostationary orbit is ~36,000 km. Bevy (and most game engines) use 32-bit floats for world-space transforms, which have insufficient precision at these scales.

## How it works

The **floating origin** pattern keeps the Bevy world origin near the camera target at all times. Each frame:

1. The `floating_origin` system (in `PostUpdate`) measures the offset between the current world origin and the active `CameraTarget`'s position.
2. If the offset exceeds `MAX_ORIGIN_OFFSET`, all entity `Transform` positions are shifted — the world is "recentred" around the target.
3. Because the offset is subtracted from every entity uniformly, relative positions are preserved.

This means that even though real distances are millions of metres, no entity's Bevy `Transform` ever drifts far from the origin, keeping floating-point precision acceptable.

## Relevant constants

| Constant | Description |
|---|---|
| `PHYSICS_ENABLE_RADIUS` | Entities within this distance (double precision, metres) have Avian3D physics enabled |
| `PHYSICS_DISABLE_RADIUS` | Entities beyond this distance have physics suspended (`RigidBodyDisabled`) |
| `MAX_ORIGIN_OFFSET` | Maximum allowable offset before a floating origin correction is applied |
| `MAX_LINVEL` | Linear velocity cap applied to physics bodies to prevent instability during origin shifts |

## Interaction with physics

Avian3D physics bodies store positions internally in double precision. When the floating origin shifts, the `floating_origin` system also updates Avian3D's `Position` component (not just the Bevy `Transform`) to keep physics and rendering consistent.

## Interaction with the camera

`orbit_camera_track` is ordered **after** `floating_origin` (`.after(floating_origin)`) so that the camera transform is computed using the post-shift world positions, preventing a one-frame lag artefact.
