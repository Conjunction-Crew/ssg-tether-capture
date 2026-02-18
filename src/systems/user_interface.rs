use crate::components::{orbit::Orbital, user_interface::TrackObject};

use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

pub fn track_objects(
    bodies: Query<(&Transform, &LinearVelocity, &Orbital)>,
    mut trackers: Query<(&mut Text, &TrackObject)>,
) {
    for (mut text, tracker) in &mut trackers {
        if let Some(entity) = &tracker.entity {
            if let Ok((transform, velocity, orbital)) = bodies.get(*entity) {
                if let Some(elements) = orbital.elements {
                    text.0 = format!(
                        "Position: ({:.2}, {:.2}, {:.2})\nVelocity: ({:.2}, {:.2}, {:.2})\nSemi-major axis (m): {:.2}\nEccentricity: {:.2}\nInclination (radians): {:.2}\nRAAN (radians): {:.2}\nArgument of periapsis (radians): {:.2}\nTrue anomaly (radians): {:.2}",
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
                        elements.nu
                    );
                }
            }
        }
    }
}
