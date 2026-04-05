// Compute shader for parallel compute of orbital elements to eci state.
// Inputs are: 6 orbital elements
// Outputs are: eci state struct
// Uniforms are: current epoch

struct OrbitalElements {
    a: f32,
    e: f32,
    i: f32,
    raan: f32,
    argp: f32,
    mean_anomaly: f32,
};

struct EciState {
    position: vec4<f32>,
    velocity: vec4<f32>,
};

struct ComputeConfig {
    dt_seconds: f32,
    count: u32,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<storage, read> elements: array<OrbitalElements>;
@group(0) @binding(1) var<storage, read_write> eci_state: array<EciState>;
@group(0) @binding(2) var<uniform> config: ComputeConfig;

@compute @workgroup_size(64)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let idx = id.x;
    if (idx >= config.count) {
        return;
    }

    let el = elements[idx];
    // propagate el -> eci_state[idx]
}