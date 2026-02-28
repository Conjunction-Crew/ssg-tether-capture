use bevy::prelude::*;

use crate::{
    components::capture_components::CaptureComponent, resources::capture_plans::CapturePlanLibrary,
};

pub fn capture_state_machine_update(
    capture_entities: Query<(Entity, &mut CaptureComponent)>,
    capture_plan_lib: ResMut<CapturePlanLibrary>,
    time: Res<Time>,
) {
    for (entity, mut capture_component) in capture_entities {
        capture_component.state_elapsed_time_s = time.elapsed_secs_f64();

        // Execute plan state machine
        if let Some(plan) = capture_plan_lib.plans.get(&capture_component.plan_id) {
        }
    }
}
