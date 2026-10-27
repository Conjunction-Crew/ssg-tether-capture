use super::*;

pub fn update_selected_catalog_overlay(
    camera: Single<(&Camera, &GlobalTransform, &RenderLayers), With<Camera3d>>,
    catalog: Res<SpaceObjectCatalog>,
    catalog_ui: Res<SpaceCatalogUiState>,
    gpu_elements: Res<GpuElements>,
    epoch_origin: Res<GpuComputeEpochOrigin>,
    world_time: Res<ssg_sim::time::WorldTime>,
    overlay: Single<(&mut Node, &Children), With<SelectedCatalogOverlay>>,
    mut label_texts: Query<&mut Text, With<SelectedCatalogOverlayLabel>>,
) {
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
