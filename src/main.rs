use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.build())
        .run();
}

#[cfg(test)]
mod tests {
    #[test]
    fn test1() {
        assert!(true);
    }
}