use crate::{
    components::{
        orbit::Orbital,
        user_interface::{OrbitLabel, TrackObject},
    },
    constants::MAP_UNITS_TO_M,
    resources::time_warp::TimeWarp,
};

use astrora_core::core::{constants::GM_EARTH, elements::coe_to_rv};
use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

pub fn track_objects(
    bodies: Query<(&Transform, &LinearVelocity, &Orbital)>,
    mut trackers: Query<(&mut Text, &TrackObject)>,
    time_warp: Res<TimeWarp>,
) {
    for (mut text, tracker) in &mut trackers {
        if let Some(entity) = &tracker.entity {
            if let Ok((transform, velocity, orbital)) = bodies.get(*entity) {
                if let Some(elements) = orbital.elements {
                    text.0 = format!(
                        "Position: ({:.2}, {:.2}, {:.2})\nVelocity: ({:.2}, {:.2}, {:.2})\nSemi-major axis (m): {:.2}\nEccentricity: {:.2}\nInclination (radians): {:.2}\nRAAN (radians): {:.2}\nArgument of periapsis (radians): {:.2}\nTrue anomaly (radians): {:.2}\nTime warp: {}x",
                        transform.translation.x,
                        transform.translation.y,
                        transform.translation.z,
                        velocity.x,
                        velocity.y,
                        velocity.z,
                        elements.a,
                        elements.e,
                        elements.i,
                        elements.raan,
                        elements.argp,
                        elements.nu,
                        time_warp.multiplier
                    );
                }
            }
        }
    }
}

pub fn map_orbitals(
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut labels: Query<(&mut Node, &OrbitLabel)>,
    orbitals: Query<&Orbital>,
) {
    let (cam, cam_transform) = camera.into_inner();

    for (mut node, label) in &mut labels {
        if let Some(entity) = label.entity {
            if let Ok(orbital) = orbitals.get(entity) {
                if let Some(elements) = orbital.elements {
                    let (r, _v) = coe_to_rv(&elements, GM_EARTH);
                    let world_position = Vec3::new(
                        r.x as f32 / MAP_UNITS_TO_M,
                        r.y as f32 / MAP_UNITS_TO_M,
                        r.z as f32 / MAP_UNITS_TO_M,
                    ) + Vec3::new(0.0, 1.0, 0.0);

                    if let Ok(viewport_position) =
                        cam.world_to_viewport(cam_transform, world_position)
                    {
                        node.top = Val::Px(viewport_position.y);
                        node.left = Val::Px(viewport_position.x);
                    }
                }
            }
        }
    }
}
