//! The bevy_ui overlays that float in the 3D view (P8 S5, D5): the tether map
//! label, the selected-catalog box and the satellite indicator. Moved here
//! unchanged from the sim screen and `hud.rs`.

use avian3d::prelude::{RigidBodyDisabled, RigidBodyQueryReadOnly};
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;

use crate::render::layers::{MAP_LAYER, SCENE_LAYER, eci_position_to_map};
use crate::ui::catalog_state::SpaceCatalogUiState;
use crate::ui::hud::HudReadouts;
use crate::ui::theme::UiTheme;
use ssg_sim::orbit::cache::OrbitalCache;
use ssg_sim::orbit::catalog::{
    GpuComputeEpochOrigin, GpuElements, SpaceObjectCatalog, propagate_catalog_eci_state,
};
use ssg_sim::physics_bubble::body_eci_state;

#[derive(Component, Debug, Clone, Default)]
pub struct OrbitLabel {
    pub entity: Option<Entity>,
}

#[derive(Component)]
pub struct SelectedCatalogOverlay;

#[derive(Component)]
pub struct SelectedCatalogOverlayLabel;

#[derive(Component)]
pub struct SatelliteIndicatorOverlay {
    pub entity: Option<Entity>,
    pub label: String,
}

#[derive(Component)]
pub struct SatelliteIndicatorOverlayLabel;

const SATELLITE_SCENE_INDICATOR_HALF_EXTENT: f32 = 12.0;
const MAP_OVERLAY_BOX_SIZE_PX: f32 = 30.0;

fn position_overlay_at_viewport_center(node: &mut Node, center: Vec2, size_px: f32) {
    let half_size = size_px * 0.5;

    node.display = Display::Flex;
    node.left = px(center.x - half_size);
    node.top = px(center.y - half_size);
    node.width = px(size_px);
    node.height = px(size_px);
}

/// Spawns the overlays under the sim screen root, hidden until their systems place them.
pub fn spawn_sim_overlays(
    root: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    theme: &UiTheme,
    readouts: &HudReadouts,
) {
    let tether_root_entity = readouts.reference_entity;
    let capture_target_entity = readouts.target_entity;
    let capture_target_label = readouts.target_label.clone();

    root.spawn((
        OrbitLabel {
            entity: tether_root_entity,
        },
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            border: UiRect::all(px(2.0)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(theme.text_primary),
    ))
    .with_children(|overlay| {
        overlay.spawn((
            Text::new("Tether1"),
            TextFont {
                font: font.clone(),
                font_size: 12.0,
                ..default()
            },
            TextColor(theme.text_primary),
            Node {
                position_type: PositionType::Absolute,
                top: px(-28.0),
                left: px(0.0),
                padding: UiRect::axes(px(8.0), px(4.0)),
                ..default()
            },
            BackgroundColor(theme.panel_background),
        ));
    });

    root.spawn((
        SelectedCatalogOverlay,
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            border: UiRect::all(px(2.0)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(theme.text_accent),
    ))
    .with_children(|overlay| {
        overlay.spawn((
            SelectedCatalogOverlayLabel,
            Text::new(""),
            TextFont {
                font: font.clone(),
                font_size: 12.0,
                ..default()
            },
            TextColor(theme.text_primary),
            Node {
                position_type: PositionType::Absolute,
                top: px(-28.0),
                left: px(0.0),
                padding: UiRect::axes(px(8.0), px(4.0)),
                ..default()
            },
            BackgroundColor(theme.panel_background),
        ));
    });

    root.spawn((
        SatelliteIndicatorOverlay {
            entity: capture_target_entity,
            label: capture_target_label,
        },
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            border: UiRect::all(px(2.0)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(theme.text_primary),
    ))
    .with_children(|overlay| {
        overlay.spawn((
            SatelliteIndicatorOverlayLabel,
            Text::new(""),
            TextFont {
                font: font.clone(),
                font_size: 12.0,
                ..default()
            },
            TextColor(theme.text_primary),
            Node {
                position_type: PositionType::Absolute,
                top: px(-28.0),
                left: px(0.0),
                padding: UiRect::axes(px(8.0), px(4.0)),
                ..default()
            },
            BackgroundColor(theme.panel_background),
        ));
    });
}

pub fn map_orbitals(
    camera: Single<(&Camera, &GlobalTransform, &RenderLayers), With<Camera3d>>,
    mut labels: Query<(&mut Node, &OrbitLabel)>,
    rigidbodies: Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>, Entity)>,
    orbital_cache: Res<OrbitalCache>,
) {
    let (cam, cam_transform, render_layers) = camera.into_inner();

    for (mut node, label) in &mut labels {
        if !render_layers.intersects(&RenderLayers::layer(crate::render::layers::MAP_LAYER)) {
            node.display = Display::None;
            continue;
        }

        node.display = Display::Block;

        let Some(entity) = label.entity else {
            continue;
        };
        let Ok((rb, disabled, entity)) = rigidbodies.get(entity) else {
            continue;
        };

        let Some(world_position) = orbit_label_map_position(
            &orbital_cache,
            entity,
            rb.position.0,
            rb.linear_velocity.0,
            disabled,
        ) else {
            continue;
        };

        if let Ok(viewport_position) = cam.world_to_viewport(cam_transform, world_position) {
            node.top = Val::Px(viewport_position.y);
            node.left = Val::Px(viewport_position.x);
        }
    }
}

/// Where [`map_orbitals`] places a body's label in the map scene: its ECI
/// position ([`body_eci_state`]: a disabled body sits at its cached state, an
/// enabled one at the cached state plus its bubble position, #123). `None` before
/// the body has a cached state.
pub fn orbit_label_map_position(
    orbital_cache: &OrbitalCache,
    entity: Entity,
    position: DVec3,
    linear_velocity: DVec3,
    disabled: bool,
) -> Option<Vec3> {
    let eci = body_eci_state(orbital_cache, entity, position, linear_velocity, disabled)?;
    Some(eci_position_to_map(Vec3::new(
        eci[0] as f32,
        eci[1] as f32,
        eci[2] as f32,
    )))
}

/// The catalog's orbital elements and the clock they are propagated to.
#[derive(SystemParam)]
pub struct CatalogOrbitClock<'w> {
    gpu_elements: Res<'w, GpuElements>,
    epoch_origin: Res<'w, GpuComputeEpochOrigin>,
    world_time: Res<'w, ssg_sim::time::WorldTime>,
}

pub fn update_selected_catalog_overlay(
    camera: Single<(&Camera, &GlobalTransform, &RenderLayers), With<Camera3d>>,
    catalog: Res<SpaceObjectCatalog>,
    catalog_ui: Res<SpaceCatalogUiState>,
    orbit_clock: CatalogOrbitClock,
    overlay: Single<(&mut Node, &Children), With<SelectedCatalogOverlay>>,
    mut label_texts: Query<&mut Text, With<SelectedCatalogOverlayLabel>>,
) {
    let CatalogOrbitClock {
        gpu_elements,
        epoch_origin,
        world_time,
    } = orbit_clock;
    let (camera, camera_transform, render_layers) = camera.into_inner();
    let (mut overlay_node, overlay_children) = overlay.into_inner();

    if !render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        overlay_node.display = Display::None;
        return;
    }

    let Some(selected_index) = catalog_ui.selected_index else {
        overlay_node.display = Display::None;
        return;
    };

    let Some(entry) = catalog.entries.get(selected_index) else {
        overlay_node.display = Display::None;
        return;
    };
    let Some(element) = gpu_elements.0.get(entry.gpu_index) else {
        overlay_node.display = Display::None;
        return;
    };

    let current_epoch_offset_seconds = epoch_origin
        .0
        .map_or(0.0, |origin| (world_time.epoch - origin) as f32);
    let Some((position_eci, _velocity_eci)) =
        propagate_catalog_eci_state(element, current_epoch_offset_seconds)
    else {
        overlay_node.display = Display::None;
        return;
    };

    let map_position = eci_position_to_map(position_eci);
    let Ok(viewport_position) = camera.world_to_viewport(camera_transform, map_position) else {
        overlay_node.display = Display::None;
        return;
    };

    position_overlay_at_viewport_center(
        &mut overlay_node,
        viewport_position,
        MAP_OVERLAY_BOX_SIZE_PX,
    );

    for child in overlay_children.iter() {
        if let Ok(mut text) = label_texts.get_mut(child) {
            text.0 = entry.display_label();
        }
    }
}

pub fn update_satellite_indicator_overlay(
    camera: Single<(&Camera, &GlobalTransform, &RenderLayers), With<Camera3d>>,
    catalog_ui: Res<SpaceCatalogUiState>,
    orbital_cache: Res<OrbitalCache>,
    overlay: Single<(&SatelliteIndicatorOverlay, &mut Node, &Children)>,
    transforms: Query<&GlobalTransform>,
    rigidbodies: Query<(RigidBodyQueryReadOnly, Has<RigidBodyDisabled>)>,
    mut label_texts: Query<&mut Text, With<SatelliteIndicatorOverlayLabel>>,
) {
    let (camera, camera_transform, render_layers) = camera.into_inner();
    let (overlay_state, mut overlay_node, overlay_children) = overlay.into_inner();

    if !catalog_ui.show_satellite_indicator {
        overlay_node.display = Display::None;
        return;
    }

    let Some(entity) = overlay_state.entity else {
        overlay_node.display = Display::None;
        return;
    };

    if render_layers.intersects(&RenderLayers::layer(MAP_LAYER)) {
        let Ok((rb, disabled)) = rigidbodies.get(entity) else {
            overlay_node.display = Display::None;
            return;
        };
        let Some(params) = orbital_cache.eci_states.get(&entity) else {
            overlay_node.display = Display::None;
            return;
        };

        let position_eci = if disabled {
            Vec3::new(params[0] as f32, params[1] as f32, params[2] as f32)
        } else {
            Vec3::new(
                (params[0] + rb.position.0.x) as f32,
                (params[1] + rb.position.0.y) as f32,
                (params[2] + rb.position.0.z) as f32,
            )
        };
        let world_position = eci_position_to_map(position_eci);

        let Ok(viewport_position) = camera.world_to_viewport(camera_transform, world_position)
        else {
            overlay_node.display = Display::None;
            return;
        };

        position_overlay_at_viewport_center(
            &mut overlay_node,
            viewport_position,
            MAP_OVERLAY_BOX_SIZE_PX,
        );
    } else if render_layers.intersects(&RenderLayers::layer(SCENE_LAYER)) {
        let Ok(transform) = transforms.get(entity) else {
            overlay_node.display = Display::None;
            return;
        };

        let center = transform.translation();
        let half_extent = Vec3::splat(SATELLITE_SCENE_INDICATOR_HALF_EXTENT);
        let corners = [
            center + Vec3::new(-half_extent.x, -half_extent.y, -half_extent.z),
            center + Vec3::new(-half_extent.x, -half_extent.y, half_extent.z),
            center + Vec3::new(-half_extent.x, half_extent.y, -half_extent.z),
            center + Vec3::new(-half_extent.x, half_extent.y, half_extent.z),
            center + Vec3::new(half_extent.x, -half_extent.y, -half_extent.z),
            center + Vec3::new(half_extent.x, -half_extent.y, half_extent.z),
            center + Vec3::new(half_extent.x, half_extent.y, -half_extent.z),
            center + Vec3::new(half_extent.x, half_extent.y, half_extent.z),
        ];

        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        let mut any_projected = false;

        for corner in corners {
            if let Ok(viewport_position) = camera.world_to_viewport(camera_transform, corner) {
                min = min.min(viewport_position);
                max = max.max(viewport_position);
                any_projected = true;
            }
        }

        if !any_projected {
            overlay_node.display = Display::None;
            return;
        }

        let padding = 6.0;
        let size = (max - min).max(Vec2::splat(18.0)) + Vec2::splat(padding * 2.0);

        overlay_node.display = Display::Flex;
        overlay_node.left = px(min.x - padding);
        overlay_node.top = px(min.y - padding);
        overlay_node.width = px(size.x);
        overlay_node.height = px(size.y);
    } else {
        overlay_node.display = Display::None;
        return;
    }

    for child in overlay_children.iter() {
        if let Ok(mut text) = label_texts.get_mut(child) {
            text.0 = overlay_state.label.clone();
        }
    }
}
