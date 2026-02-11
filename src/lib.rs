use avian3d::prelude::*;
use bevy::prelude::*;

pub fn run() {
    create_app().add_plugins(DefaultPlugins.build()).run();
}

pub fn create_app() -> App {
    let app = App::new();
    app
}

// Integration Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_setup() {
        let mut app = create_app();
        app.add_plugins(MinimalPlugins.build());
        app.update();
    }
}
