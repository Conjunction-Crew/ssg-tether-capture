use std::borrow::Cow;

use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_graph::{self, RenderLabel},
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor,
            PipelineCache, ShaderStages, ShaderType, StorageBuffer, UniformBuffer,
            binding_types::{storage_buffer, storage_buffer_read_only, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
    },
};
use brahe::{AngleFormat, utils::DOrbitStateProvider};

use crate::{components::orbit::Orbital, resources::world_time::WorldTime, ui::state::UiScreen};

const SHADER_ASSET_PATH: &str = "shaders/orbital_eci.wgsl";

pub struct GpuComputePlugin;

impl Plugin for GpuComputePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractResourcePlugin::<GpuElements>::default(),
            ExtractResourcePlugin::<GpuComputeUniforms>::default(),
        ))
        .add_systems(OnEnter(UiScreen::Sim), init_gpu_compute_resources)
        .add_systems(OnExit(UiScreen::Sim), remove_gpu_compute_resources)
        .add_systems(
            PostUpdate,
            (update_gpu_elements, update_gpu_uniforms)
                .chain()
                .run_if(in_state(UiScreen::Sim)),
        );

        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .add_systems(RenderStartup, init_compute_pipeline)
            .add_systems(
                Render,
                prepare_bind_group
                    .in_set(RenderSystems::PrepareBindGroups)
                    .run_if(resource_exists::<GpuElements>)
                    .run_if(resource_exists::<GpuComputeUniforms>)
                    .run_if(not(resource_exists::<GpuComputeBindGroups>)),
            );
    }
}

fn update_gpu_uniforms(
    time: Res<Time>,
    gpu_elements: Res<GpuElements>,
    mut uniforms: ResMut<GpuComputeUniforms>,
) {
    uniforms.dt_seconds = time.delta_secs();
    uniforms.count = gpu_elements.0.len() as u32;
    uniforms._pad = Vec2::ZERO;
}

fn update_gpu_elements(
    orbitals: Query<&Orbital>,
    world_time: Res<WorldTime>,
    mut gpu_elements: ResMut<GpuElements>,
) {
    let epoch = world_time.epoch;

    gpu_elements.0 = orbitals
        .iter()
        .filter_map(|orbital| {
            let propagator = orbital.propagator.as_ref()?;
            let elements = propagator.state_koe_osc(epoch, AngleFormat::Radians).ok()?;

            if elements[1] >= 1.0 {
                return None;
            }

            Some(GpuOrbitalElements {
                a: elements[0] as f32,
                e: elements[1] as f32,
                i: elements[2] as f32,
                raan: elements[3] as f32,
                argp: elements[4] as f32,
                mean_anomaly: elements[5] as f32,
            })
        })
        .collect();
}

fn init_gpu_compute_resources(mut commands: Commands) {
    commands.insert_resource(GpuElements(Vec::new()));
    commands.insert_resource(GpuComputeUniforms {
        dt_seconds: 0.0,
        count: 0,
        _pad: Vec2::ZERO,
    });
}

fn remove_gpu_compute_resources(mut commands: Commands) {
    commands.remove_resource::<GpuElements>();
    commands.remove_resource::<GpuComputeUniforms>();
}

#[derive(Clone, Copy, ShaderType)]
pub struct GpuOrbitalElements {
    pub a: f32,
    pub e: f32,
    pub i: f32,
    pub raan: f32,
    pub argp: f32,
    pub mean_anomaly: f32,
}

#[derive(Resource, Clone, ExtractResource)]
pub struct GpuElements(pub Vec<GpuOrbitalElements>);

#[derive(Clone, Copy, ShaderType, Default)]
struct GpuEciState {
    position: Vec4,
    velocity: Vec4,
}

#[derive(Resource, Clone, ExtractResource, ShaderType)]
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

fn prepare_bind_group(
    mut commands: Commands,
    pipeline: Res<GpuComputePipeline>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    queue: Res<RenderQueue>,
    gpu_elements: Res<GpuElements>,
    uniforms: Res<GpuComputeUniforms>,
) {
    let count = gpu_elements.0.len();

    let mut elements_buffer = StorageBuffer::from(gpu_elements.0.clone());
    elements_buffer.write_buffer(&render_device, &queue);

    let mut output_buffer = StorageBuffer::from(vec![GpuEciState::default(); count]);
    output_buffer.write_buffer(&render_device, &queue);

    let mut uniform_buffer = UniformBuffer::from(GpuComputeUniforms {
        dt_seconds: uniforms.dt_seconds,
        count: count as u32,
        _pad: Vec2::ZERO,
    });
    uniform_buffer.write_buffer(&render_device, &queue);

    let bind_group = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.bind_group_layout),
        &BindGroupEntries::sequential((&elements_buffer, &output_buffer, &uniform_buffer)),
    );
    commands.insert_resource(GpuComputeBindGroups([bind_group]));
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
    let shader = asset_server.load(SHADER_ASSET_PATH);
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

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct GpuComputeLabel;

#[derive(Default)]
struct GpuComputeNode;

impl render_graph::Node for GpuComputeNode {
    fn run(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), render_graph::NodeRunError> {
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<GpuComputePipeline>();
        let bind_groups = world.resource::<GpuComputeBindGroups>();

        if let Some(update_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.update_pipeline)
        {
            let count = world.resource::<GpuComputeUniforms>().count;
            let workgroups = count.div_ceil(64);

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
