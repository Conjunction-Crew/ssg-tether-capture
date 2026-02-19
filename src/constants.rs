use astrora_core::core::elements::OrbitalElements;

// Camera layers
pub const SCENE_LAYER: usize = 0;
pub const MAP_LAYER: usize = 1;
pub const UI_LAYER: usize = 2;

// Earth constants
pub const EARTH_RADIUS: f32 = 6_360_000.0;
pub const EARTH_ATMOSPHERE_RADIUS: f32 = 6_460_000.0;

// Map constants
pub const MAP_UNITS_TO_M: f32 = 100000.0;

// Tether testing constants
pub const NUM_TETHER_JOINTS: u32 = 10;
pub const DIST_BETWEEN_JOINTS: f32 = 1.1;

// Other constants
pub const ISS_ORBIT: OrbitalElements = OrbitalElements {
    // Semi-major axis (meters)
    a: 6_799_130.0,
    // Eccentricity (dimensionless)
    e: 0.00112,
    // Inclination (radians)
    i: 0.90114,
    // Right ascension of ascending node (radians)
    raan: 3.54993,
    // raan: 6.28,
    // Argument of periapsis (radians)
    argp: 1.51296,
    // True anomaly (radians)
    nu: 4.77190,
};