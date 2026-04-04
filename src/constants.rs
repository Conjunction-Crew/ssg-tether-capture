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

// Tether testing constants
pub const TETHER_LENGTH: f64 = 20.0;
pub const DIST_BETWEEN_JOINTS: f64 = 0.1;

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
    // True anomaly (radians)
    4.77190,
);
