//! App-side integration tests (P3 S2): they need the camera plugin.

use crate::render::camera::{OrbitCamera, OrbitCameraParams};
use crate::render::celestial::Earth;
use crate::render::layers::SCENE_LAYER;
use crate::tests::app_support::{enter_sim, test_app, tick};
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use ssg_sim::anchor::CameraTarget;

#[test]
fn orbit_camera() {
    let mut app = test_app();
    enter_sim(&mut app);

    // `orbit_camera_track` needs the Earth, a target and a camera on the scene layer.
    // Earth is directly "below" the target (along -Z), so the scene camera's up vector
    // is +Z.
    app.world_mut()
        .spawn((Earth, Transform::from_xyz(10.0, 20.0, -1000.0)));
    app.world_mut()
        .spawn((CameraTarget, Transform::from_xyz(10.0, 20.0, 30.0)));
    let orbit_camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            RenderLayers::layer(SCENE_LAYER),
            Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            OrbitCamera {
                map_params: OrbitCameraParams::default(),
                scene_params: OrbitCameraParams::default(),
            },
        ))
        .id();

    tick(&mut app);

    // The camera tracks the target: focus = target translation, up = away from Earth.
    let camera = app.world().get::<OrbitCamera>(orbit_camera).unwrap();
    assert_eq!(camera.scene_params.focus, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!(camera.scene_params.up, Vec3::Z);

    // With up = +Z, an unrotated Earth (north axis +Z) and zero yaw/pitch,
    // `orbit_camera_track` gives forward = +X, so the camera sits `distance` (30)
    // behind the focus: focus - X * 30.
    let translation = app
        .world()
        .get::<Transform>(orbit_camera)
        .unwrap()
        .translation;
    assert!(
        translation.abs_diff_eq(Vec3::new(-20.0, 20.0, 30.0), 1e-4),
        "camera translation is {translation:?}"
    );
}
