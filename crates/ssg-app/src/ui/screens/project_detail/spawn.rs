use super::*;
use crate::ui::hud::HudReadouts;
use crate::ui::overlays::spawn_sim_overlays;

/// The bevy_ui part of the sim screen: the in-scene overlays only (D5).
pub fn spawn_project_detail_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    theme: Res<UiTheme>,
    readouts: Res<HudReadouts>,
) {
    let font = asset_server.load("fonts/FiraMono-Medium.ttf");

    commands
        .spawn((
            SimScreen,
            ScreenRoot,
            RenderLayers::layer(UI_LAYER),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        ))
        .with_children(|root| spawn_sim_overlays(root, &font, &theme, &readouts));
}

pub fn cleanup_project_detail_screen(
    mut commands: Commands,
    roots: Query<Entity, With<SimScreen>>,
) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}
