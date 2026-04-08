// Compute shader for parallel compute of orbital elements to eci state.
// Inputs are: 6 orbital elements
// Outputs are: eci state struct
// Uniforms are: current epoch

struct OrbitalElements {
    id: u32,
    a: f32,
    e: f32,
    i: f32,
    raan: f32,
    argp: f32,
    mean_anomaly: f32,
    epoch_offset_seconds: f32,
};

struct EciState {
    id: u32,
    position: vec3<f32>,
    velocity: vec3<f32>,
};

struct ComputeConfig {
    dt_seconds: f32,  // 4 byte
    count: u32,       // 4 byte
    _pad: vec2<f32>,  // 8 byte padding for total of 16 bytes
};

@group(0) @binding(0) var<storage, read> elements: array<OrbitalElements>;
@group(0) @binding(1) var<storage, read_write> eci_state: array<EciState>;
@group(0) @binding(2) var<uniform> config: ComputeConfig;

const MU_EARTH: f32 = 3.9860044e14;
const PI: f32 = 3.141592653589793;
const TAU: f32 = 6.283185307179586;

fn wrap_angle(angle: f32) -> f32 {
    return angle - TAU * floor((angle + PI) / TAU);
}

fn solve_eccentric_anomaly(mean_anomaly: f32, eccentricity: f32) -> f32 {
    var eccentric_anomaly = mean_anomaly;

    if (eccentricity > 0.8) {
        if (mean_anomaly < 0.0) {
            eccentric_anomaly = -PI;
        } else {
            eccentric_anomaly = PI;
        }
    }

    for (var iteration: u32 = 0u; iteration < 6u; iteration = iteration + 1u) {
        let sin_e = sin(eccentric_anomaly);
        let cos_e = cos(eccentric_anomaly);
        let residual = eccentric_anomaly - eccentricity * sin_e - mean_anomaly;
        let derivative = 1.0 - eccentricity * cos_e;
        eccentric_anomaly = eccentric_anomaly - residual / derivative;
    }

    return eccentric_anomaly;
}

fn perifocal_to_eci(value: vec3<f32>, inclination: f32, raan: f32, argp: f32) -> vec3<f32> {
    let cos_raan = cos(raan);
    let sin_raan = sin(raan);
    let cos_argp = cos(argp);
    let sin_argp = sin(argp);
    let cos_i = cos(inclination);
    let sin_i = sin(inclination);

    let r11 = cos_raan * cos_argp - sin_raan * sin_argp * cos_i;
    let r12 = -cos_raan * sin_argp - sin_raan * cos_argp * cos_i;
    let r21 = sin_raan * cos_argp + cos_raan * sin_argp * cos_i;
    let r22 = -sin_raan * sin_argp + cos_raan * cos_argp * cos_i;
    let r31 = sin_argp * sin_i;
    let r32 = cos_argp * sin_i;

    return vec3<f32>(
        r11 * value.x + r12 * value.y,
        r21 * value.x + r22 * value.y,
        r31 * value.x + r32 * value.y,
    );
}

fn elements_to_eci(el: OrbitalElements, dt_seconds: f32) -> EciState {
    if (el.a <= 0.0 || el.e < 0.0 || el.e >= 1.0) {
        return EciState(el.id, vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 0.0));
    }

    let mean_motion = sqrt(MU_EARTH / (el.a * el.a * el.a));
    let mean_anomaly = wrap_angle(el.mean_anomaly + mean_motion * dt_seconds);
    let eccentric_anomaly = solve_eccentric_anomaly(mean_anomaly, el.e);

    let sin_e = sin(eccentric_anomaly);
    let cos_e = cos(eccentric_anomaly);
    let one_minus_e2 = max(0.0, 1.0 - el.e * el.e);
    let sqrt_one_minus_e2 = sqrt(one_minus_e2);
    let radius = el.a * (1.0 - el.e * cos_e);

    if (radius <= 0.0) {
        return EciState(el.id, vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 0.0));
    }

    let position_pf = vec3<f32>(
        el.a * (cos_e - el.e),
        el.a * sqrt_one_minus_e2 * sin_e,
        0.0,
    );

    let velocity_scale = sqrt(MU_EARTH * el.a) / radius;
    let velocity_pf = vec3<f32>(
        -velocity_scale * sin_e,
        velocity_scale * sqrt_one_minus_e2 * cos_e,
        0.0,
    );

    let position_eci = perifocal_to_eci(position_pf, el.i, el.raan, el.argp);
    let velocity_eci = perifocal_to_eci(velocity_pf, el.i, el.raan, el.argp);

    return EciState(
        el.id,
        vec3<f32>(position_eci),
        vec3<f32>(velocity_eci),
    );
}

@compute @workgroup_size(64)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let idx = id.x;
    if (idx >= config.count) {
        return;
    }

    let el = elements[idx];
    let dt_seconds = config.dt_seconds - el.epoch_offset_seconds;
    eci_state[idx] = elements_to_eci(el, dt_seconds);
}
