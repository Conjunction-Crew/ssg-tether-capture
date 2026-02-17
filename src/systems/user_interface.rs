use crate::components::user_interface::TrackObject;

use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

pub fn track_objects(
    bodies: Query<(&Transform, &LinearVelocity)>,
    mut trackers: Query<(&mut Text, &TrackObject)>,
) {
    for (mut text, tracker) in &mut trackers {
        if let Some(entity) = &tracker.entity {
            if let Ok((transform, velocity)) = bodies.get(*entity) {
                text.0 = format!(
                    "Position: ({:.2}, {:.2}, {:.2}), Velocity: ({:.2}, {:.2}, {:.2})",
                    transform.translation.x,
                    transform.translation.y,
                    transform.translation.z,
                    velocity.x,
                    velocity.y,
                    velocity.z
                );
            }
        }
    }
}
