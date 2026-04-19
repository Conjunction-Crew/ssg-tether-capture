// GPU Compute for large debris datasets.
// TODO: SCRUM-63 Clean this up in a general organizational PR.
// (the GPU compute logic is condensed into one file for now during testing / bring up)

use std::borrow::Cow;

use crate::{
    constants::{MAP_LAYER, MAP_UNITS_TO_M, eci_to_orbit_frame},
    resources::space_catalog::SpaceCatalogUiState,
    resources::world_time::WorldTime,
    ui::state::UiScreen,
};
use bevy::{
    camera::visibility::RenderLayers,
    math::DVec3,
    mesh::MeshTag,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{
            AsBindGroup, BindGroup, BindGroupEntries, BindGroupLayoutDescriptor,
            BindGroupLayoutEntries, CachedComputePipelineId, ComputePassDescriptor,
            ComputePipelineDescriptor, PipelineCache, ShaderStages, ShaderType, StorageBuffer,
            UniformBuffer,
            binding_types::{storage_buffer, storage_buffer_read_only, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        storage::{GpuShaderStorageBuffer, ShaderStorageBuffer},
    },
    shader::ShaderRef,
};
use brahe::Epoch;

const ECI_SHADER_ASSET_PATH: &str = "shaders/orbital_eci.wgsl";
const MAP_SHADER_ASSET_PATH: &str = "shaders/map_eci_points.wgsl";
pub const MAP_POINT_SCALE: f32 = 0.2;

#[derive(Clone, Copy, ShaderType, Debug)]
struct MapOrbitPointConfig {
    map_units_to_m: f32,
    point_scale: f32,
    _pad: Vec2,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct MapOrbitPointMaterial {
    #[storage(0, read_only)]
    eci_states: Handle<ShaderStorageBuffer>,
    #[uniform(1)]
    config: MapOrbitPointConfig,
}

impl Material for MapOrbitPointMaterial {
    fn vertex_shader() -> ShaderRef {
        MAP_SHADER_ASSET_PATH.into()
    }

    fn fragment_shader() -> ShaderRef {
        MAP_SHADER_ASSET_PATH.into()
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct GpuComputeLabel;

pub struct GpuComputePlugin;

impl Plugin for GpuComputePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractResourcePlugin::<GpuElements>::default(),
            ExtractResourcePlugin::<GpuComputeUniforms>::default(),
            ExtractResourcePlugin::<GpuEciStateBuffer>::default(),
            MaterialPlugin::<MapOrbitPointMaterial>::default(),
        ))
        .init_resource::<GpuElements>()
        .init_resource::<GpuComputeUniforms>()
        .init_resource::<GpuComputeEpochOrigin>()
        .add_systems(
            PostUpdate,
            (
                capture_gpu_epoch_origin,
                setup_map_orbit_points,
                update_gpu_uniforms,
            )
                .chain()
                .run_if(in_state(UiScreen::Sim)),
        )
        .add_systems(
            PostUpdate,
            sync_map_orbit_point_visibility.run_if(in_state(UiScreen::Sim)),
        )
        .add_systems(OnExit(UiScreen::Sim), reset_gpu_compute_resources);

        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .add_systems(RenderStartup, init_compute_pipeline)
            .add_systems(
                Render,
                prepare_bind_group
                    .in_set(RenderSystems::PrepareBindGroups)
                    .run_if(resource_exists::<GpuElements>)
                    .run_if(resource_exists::<GpuComputeUniforms>)
                    .run_if(resource_exists::<GpuEciStateBuffer>),
            );

        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(GpuComputeLabel, GpuComputeNode::default());
        render_graph.add_node_edge(GpuComputeLabel, bevy::render::graph::CameraDriverLabel);
    }
}

fn update_gpu_uniforms(
    world_time: Res<WorldTime>,
    gpu_elements: Res<GpuElements>,
    epoch_origin: Res<GpuComputeEpochOrigin>,
    mut uniforms: ResMut<GpuComputeUniforms>,
) {
    uniforms.dt_seconds = epoch_origin
        .0
        .map_or(0.0, |origin| (world_time.epoch - origin) as f32);
    uniforms.count = gpu_elements.0.len() as u32;
    uniforms._pad = Vec2::ZERO;
}

fn capture_gpu_epoch_origin(
    world_time: Res<WorldTime>,
    gpu_elements: Res<GpuElements>,
    mut epoch_origin: ResMut<GpuComputeEpochOrigin>,
) {
    if epoch_origin.0.is_none() && !gpu_elements.0.is_empty() {
        epoch_origin.0 = Some(world_time.epoch);
    }
}

fn reset_gpu_compute_resources(
    mut commands: Commands,
    mut gpu_elements: ResMut<GpuElements>,
    mut uniforms: ResMut<GpuComputeUniforms>,
    mut epoch_origin: ResMut<GpuComputeEpochOrigin>,
) {
    gpu_elements.0.clear();
    *uniforms = GpuComputeUniforms::default();
    epoch_origin.0 = None;
    commands.remove_resource::<GpuEciStateBuffer>();
}

fn sync_map_orbit_point_visibility(
    catalog_ui: Res<SpaceCatalogUiState>,
    mut points: Query<&mut Visibility, With<MapOrbitPointMarker>>,
) {
    let visibility = if catalog_ui.show_points {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for mut point_visibility in &mut points {
        *point_visibility = visibility;
    }
}

fn setup_map_orbit_points(
    mut commands: Commands,
    gpu_elements: Res<GpuElements>,
    catalog_ui: Res<SpaceCatalogUiState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MapOrbitPointMaterial>>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    output_buffer: Option<Res<GpuEciStateBuffer>>,
) {
    if output_buffer.is_some() || gpu_elements.0.is_empty() {
        return;
    }

    let count = gpu_elements.0.len();
    let marker_mesh = meshes.add(Cuboid::from_size(Vec3::splat(1.0)));
    let eci_states = buffers.add(ShaderStorageBuffer::from(vec![
        GpuEciState::default();
        count
    ]));
    let marker_material = materials.add(MapOrbitPointMaterial {
        eci_states: eci_states.clone(),
        config: MapOrbitPointConfig {
            map_units_to_m: MAP_UNITS_TO_M as f32,
            point_scale: MAP_POINT_SCALE,
            _pad: Vec2::ZERO,
        },
    });

    commands.insert_resource(GpuEciStateBuffer(eci_states));

    for index in 0..count {
        commands.spawn((
            DespawnOnExit(UiScreen::Sim),
            MapOrbitPointMarker,
            if catalog_ui.show_points {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
            RenderLayers::layer(MAP_LAYER),
            Mesh3d(marker_mesh.clone()),
            MeshMaterial3d(marker_material.clone()),
            MeshTag(index as u32),
            Transform::IDENTITY,
        ));
    }
}

#[derive(Component)]
pub struct MapOrbitPointMarker;

#[derive(Clone, Copy, ShaderType)]
pub struct GpuOrbitalElements {
    pub id: u32,
    pub a: f32,
    pub e: f32,
    pub i: f32,
    pub raan: f32,
    pub argp: f32,
    pub mean_anomaly: f32,
    pub epoch_offset_seconds: f32,
}

pub fn propagate_catalog_eci_state(
    element: &GpuOrbitalElements,
    current_epoch_offset_seconds: f32,
) -> Option<(Vec3, Vec3)> {
    let semi_major_axis = element.a as f64;
    let eccentricity = element.e as f64;

    if semi_major_axis <= 0.0 || !(0.0..1.0).contains(&eccentricity) {
        return None;
    }

    let dt_seconds = current_epoch_offset_seconds as f64 - element.epoch_offset_seconds as f64;
    let mean_motion =
        (3.986_004_4e14_f64 / (semi_major_axis * semi_major_axis * semi_major_axis)).sqrt();
    let mean_anomaly = wrap_angle(element.mean_anomaly as f64 + mean_motion * dt_seconds);
    let eccentric_anomaly = solve_eccentric_anomaly(mean_anomaly, eccentricity);

    let sin_e = eccentric_anomaly.sin();
    let cos_e = eccentric_anomaly.cos();
    let one_minus_e2 = (1.0 - eccentricity * eccentricity).max(0.0);
    let sqrt_one_minus_e2 = one_minus_e2.sqrt();
    let radius = semi_major_axis * (1.0 - eccentricity * cos_e);

    if radius <= 0.0 {
        return None;
    }

    let position_pf = DVec3::new(
        semi_major_axis * (cos_e - eccentricity),
        semi_major_axis * sqrt_one_minus_e2 * sin_e,
        0.0,
    );

    let velocity_scale = (3.986_004_4e14_f64 * semi_major_axis).sqrt() / radius;
    let velocity_pf = DVec3::new(
        -velocity_scale * sin_e,
        velocity_scale * sqrt_one_minus_e2 * cos_e,
        0.0,
    );

    let position_eci = perifocal_to_eci(
        position_pf,
        element.i as f64,
        element.raan as f64,
        element.argp as f64,
    );
    let velocity_eci = perifocal_to_eci(
        velocity_pf,
        element.i as f64,
        element.raan as f64,
        element.argp as f64,
    );

    Some((position_eci.as_vec3(), velocity_eci.as_vec3()))
}

pub fn eci_position_to_map(position_eci: Vec3) -> Vec3 {
    let scale = MAP_UNITS_TO_M as f32;
    eci_to_orbit_frame(position_eci) / scale
}

fn wrap_angle(angle: f64) -> f64 {
    angle - std::f64::consts::TAU * ((angle + std::f64::consts::PI) / std::f64::consts::TAU).floor()
}

fn solve_eccentric_anomaly(mean_anomaly: f64, eccentricity: f64) -> f64 {
    let mut eccentric_anomaly = if eccentricity > 0.8 {
        if mean_anomaly < 0.0 {
            -std::f64::consts::PI
        } else {
            std::f64::consts::PI
        }
    } else {
        mean_anomaly
    };

    for _ in 0..6 {
        let sin_e = eccentric_anomaly.sin();
        let cos_e = eccentric_anomaly.cos();
        let residual = eccentric_anomaly - eccentricity * sin_e - mean_anomaly;
        let derivative = 1.0 - eccentricity * cos_e;
        eccentric_anomaly -= residual / derivative;
    }

    eccentric_anomaly
}

fn perifocal_to_eci(value: DVec3, inclination: f64, raan: f64, argp: f64) -> DVec3 {
    let cos_raan = raan.cos();
    let sin_raan = raan.sin();
    let cos_argp = argp.cos();
    let sin_argp = argp.sin();
    let cos_i = inclination.cos();
    let sin_i = inclination.sin();

    let r11 = cos_raan * cos_argp - sin_raan * sin_argp * cos_i;
    let r12 = -cos_raan * sin_argp - sin_raan * cos_argp * cos_i;
    let r21 = sin_raan * cos_argp + cos_raan * sin_argp * cos_i;
    let r22 = -sin_raan * sin_argp + cos_raan * cos_argp * cos_i;
    let r31 = sin_argp * sin_i;
    let r32 = cos_argp * sin_i;

    DVec3::new(
        r11 * value.x + r12 * value.y,
        r21 * value.x + r22 * value.y,
        r31 * value.x + r32 * value.y,
    )
}

#[derive(Resource, Clone, Default, ExtractResource)]
pub struct GpuElements(pub Vec<GpuOrbitalElements>);

#[derive(Resource, Clone, ExtractResource)]
struct GpuEciStateBuffer(Handle<ShaderStorageBuffer>);

#[derive(Resource, Default)]
pub struct GpuComputeEpochOrigin(pub Option<Epoch>);

#[derive(Clone, Copy, ShaderType, Default)]
struct GpuEciState {
    id: u32,
    position: Vec3,
    velocity: Vec3,
}

#[derive(Resource, Clone, Copy, Default, ExtractResource, ShaderType)]
struct GpuComputeUniforms {
    dt_seconds: f32,
    count: u32,
    _pad: Vec2,
}

#[derive(Resource)]
struct GpuComputePipeline {
    bind_group_layout: BindGroupLayoutDescriptor,
    update_pipeline: CachedComputePipelineId,
}

#[derive(Resource)]
struct GpuComputeBindGroups([BindGroup; 1]);

#[derive(Resource)]
struct GpuComputeBuffers {
    count: usize,
    output_handle: Handle<ShaderStorageBuffer>,
    elements: StorageBuffer<Vec<GpuOrbitalElements>>,
    uniforms: UniformBuffer<GpuComputeUniforms>,
}

fn prepare_bind_group(
    mut commands: Commands,
    pipeline: Res<GpuComputePipeline>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    queue: Res<RenderQueue>,
    gpu_elements: Res<GpuElements>,
    uniforms: Res<GpuComputeUniforms>,
    output_handle: Res<GpuEciStateBuffer>,
    gpu_output_buffers: Res<RenderAssets<GpuShaderStorageBuffer>>,
    buffers: Option<ResMut<GpuComputeBuffers>>,
) {
    let count = gpu_elements.0.len();

    if count == 0 {
        return;
    }

    let Some(gpu_output_buffer) = gpu_output_buffers.get(&output_handle.0) else {
        return;
    };

    let rebuild_buffers = buffers
        .as_ref()
        .is_none_or(|buffers| buffers.count != count || buffers.output_handle != output_handle.0);

    if rebuild_buffers {
        let mut new_buffers = GpuComputeBuffers {
            count,
            output_handle: output_handle.0.clone(),
            elements: StorageBuffer::from(gpu_elements.0.clone()),
            uniforms: UniformBuffer::from(*uniforms),
        };
        new_buffers.elements.write_buffer(&render_device, &queue);
        new_buffers.uniforms.write_buffer(&render_device, &queue);

        let bind_group = render_device.create_bind_group(
            None,
            &pipeline_cache.get_bind_group_layout(&pipeline.bind_group_layout),
            &BindGroupEntries::sequential((
                &new_buffers.elements,
                gpu_output_buffer.buffer.as_entire_buffer_binding(),
                &new_buffers.uniforms,
            )),
        );

        commands.insert_resource(new_buffers);
        commands.insert_resource(GpuComputeBindGroups([bind_group]));
        return;
    }

    let mut buffers = buffers.expect("buffers checked above");

    if gpu_elements.is_changed() {
        buffers.elements.set(gpu_elements.0.clone());
        buffers.elements.write_buffer(&render_device, &queue);
    }

    if uniforms.is_changed() {
        buffers.uniforms.set(*uniforms);
        buffers.uniforms.write_buffer(&render_device, &queue);
    }
}

fn init_compute_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    pipeline_cache: Res<PipelineCache>,
) {
    let bind_group_layout = BindGroupLayoutDescriptor::new(
        "GpuComputeLayout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                storage_buffer_read_only::<Vec<GpuOrbitalElements>>(false),
                storage_buffer::<Vec<GpuEciState>>(false),
                uniform_buffer::<GpuComputeUniforms>(false),
            ),
        ),
    );
    let shader = asset_server.load(ECI_SHADER_ASSET_PATH);
    let update_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        layout: vec![bind_group_layout.clone()],
        shader,
        entry_point: Some(Cow::from("update")),
        ..default()
    });

    commands.insert_resource(GpuComputePipeline {
        bind_group_layout,
        update_pipeline,
    });
}

#[derive(Default)]
struct GpuComputeNode;

impl render_graph::Node for GpuComputeNode {
    fn run(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), render_graph::NodeRunError> {
        let Some(pipeline_cache) = world.get_resource::<PipelineCache>() else {
            return Ok(());
        };
        let Some(pipeline) = world.get_resource::<GpuComputePipeline>() else {
            return Ok(());
        };
        let Some(bind_groups) = world.get_resource::<GpuComputeBindGroups>() else {
            return Ok(());
        };
        let Some(uniforms) = world.get_resource::<GpuComputeUniforms>() else {
            return Ok(());
        };

        if uniforms.count == 0 {
            return Ok(());
        }

        if let Some(update_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.update_pipeline)
        {
            let workgroups = uniforms.count.div_ceil(64);

            let mut pass = render_context
                .command_encoder()
                .begin_compute_pass(&ComputePassDescriptor::default());

            pass.set_pipeline(update_pipeline);
            pass.set_bind_group(0, &bind_groups.0[0], &[]);
            pass.dispatch_workgroups(workgroups, 1, 1);
        }

        Ok(())
    }
}
