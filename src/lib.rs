mod components;
mod systems;

use avian3d::prelude::*;
use bevy::prelude::*;
use systems::orbit_camera::*;
use systems::setup::setup;

// Main entrypoint to run the desktop application
pub fn run() {
    let mut app = create_app();
    app.add_plugins(DefaultPlugins.build().disable::<TransformPlugin>())
        .run();
}

fn create_app() -> App {
    let mut app = App::new();
    app.add_plugins(PhysicsPlugins::default())
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                orbit_camera_input,
                orbit_camera_track,
                orbit_camera_switch_target,
                orbit_camera_control_target,
            ),
        )
        .insert_resource(ClearColor(Color::srgb(0.0, 0.0, 0.0)))
        .insert_resource(Gravity(Vec3::ZERO));

    app
}

// Minimal test app harness for unit testing
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{input::InputPlugin, scene::ScenePlugin};

    fn test_app() -> App {
        let mut app = create_app();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            InputPlugin,
            ScenePlugin
        ))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>();
        app
    }

    #[test]
    fn minimal_rigidbody_setup() {
        let mut app = test_app();

        let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

        app.world_mut().spawn((
            RigidBody::Dynamic,
            Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
            Transform::from_xyz(40.0, 40.0, 40.0),
        ));

        app.update();
    }
}
