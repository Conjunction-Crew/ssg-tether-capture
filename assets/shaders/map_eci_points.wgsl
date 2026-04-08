#import bevy_pbr::{
    mesh_functions,
    view_transformations::position_world_to_clip
}

struct EciState {
    position: vec4<f32>,
    velocity: vec4<f32>,
};

struct MapOrbitPointConfig {
    map_units_to_m: f32,
    point_scale: f32,
    _pad: vec2<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read> eci_states: array<EciState>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> config: MapOrbitPointConfig;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;

    let tag = mesh_functions::get_tag(vertex.instance_index);
    let raw_map_pos = eci_states[tag].position.xyz / config.map_units_to_m;
    let map_pos = vec3<f32>(raw_map_pos.x, raw_map_pos.z, -raw_map_pos.y);

    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let local_pos = vec4(vertex.position * config.point_scale, 1.0);
    let base_world_pos = mesh_functions::mesh_position_local_to_world(world_from_local, local_pos);
    let world_pos = vec4<f32>(base_world_pos.xyz + map_pos, base_world_pos.w);

    out.clip_position = position_world_to_clip(world_pos.xyz);
    out.color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
