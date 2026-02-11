use avian3d::prelude::*;
use bevy::prelude::*;

pub fn run() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.build().disable::<TransformPlugin>())
    .add_plugins(PhysicsPlugins::default())
    .run();
}

// Integration Tests
#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            TransformPlugin,
            ColliderHierarchyPlugin,
            AssetPlugin::default(),
        ));
        app.init_asset::<Mesh>();

        app
    }

    #[test]
    fn minimal_rigidbody_setup() {
        let mut app = test_app();

        let test_sphere_mesh = Mesh::from(Sphere::new(1.0));

        app.world_mut().spawn((
            RigidBody::Dynamic,
            Collider::convex_hull_from_mesh(&test_sphere_mesh).unwrap(),
            Transform::from_xyz(0.0, 4.0, 0.0),
        ));

        app.update();
    }
}
