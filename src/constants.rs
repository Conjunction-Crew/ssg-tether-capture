use nalgebra::Vector6;

// Camera layers
pub const SCENE_LAYER: usize = 0;
pub const MAP_LAYER: usize = 1;
pub const UI_LAYER: usize = 2;

// Earth constants
pub const EARTH_RADIUS: f32 = 6_360_000.0;
pub const EARTH_ATMOSPHERE_RADIUS: f32 = 6_460_000.0;

// Map constants
pub const MAP_UNITS_TO_M: f64 = 100000.0;

// Floating origin constants
pub const PHYSICS_ENABLE_RADIUS: f64 = 100.0;
pub const PHYSICS_DISABLE_RADIUS: f64 = PHYSICS_ENABLE_RADIUS * 1.1;
pub const MAX_ORIGIN_OFFSET: f64 = PHYSICS_ENABLE_RADIUS / 2.0;

// Orbit controls widget — hold-to-accelerate
/// Maximum pitch (radians) for the orbit camera — full ±180° so the user
/// can rotate all the way around vertically.
pub const ORBIT_MAX_PITCH_RAD: f32 = std::f32::consts::PI;
/// Base orbit speed (rad/s) while a direction button is pressed.
/// One quarter of the old fixed speed, giving fine-grained control on short taps.
pub const ORBIT_WIDGET_BASE_ORBIT_SPEED: f32 = 0.2;
/// Base zoom speed (units/s) while a zoom button is pressed.
pub const ORBIT_WIDGET_BASE_ZOOM_SPEED: f32 = 5.0;
/// Seconds of continuous hold before acceleration kicks in.
pub const ORBIT_WIDGET_HOLD_THRESHOLD_SECS: f32 = 2.0;
/// Multiplier applied after the hold threshold (restores the original pre-constant speed).
pub const ORBIT_WIDGET_ACCEL_MULTIPLIER: f32 = 4.0;

// Other constants
pub const ISS_ORBIT: Vector6<f64> = Vector6::new(
    // Semi-major axis (meters)
    6_799_130.0,
    // Eccentricity (dimensionless)
    0.00112,
    // Inclination (radians)
    0.90114,
    // Right ascension of ascending node (radians)
    3.54993,
    // Argument of periapsis (radians)
    1.51296,
    // Mean anomaly (radians)
    4.77190,
);
