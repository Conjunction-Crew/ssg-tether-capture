use bevy::{math::DVec3, prelude::{Component, Quat, Vec3}};

// Orbital parameters and state for a body approaching another object.
#[derive(Component, Debug, Clone)]
pub struct OrbitalInfo {
	pub object_id: String,
	pub primary_id: String,
	pub tle: Option<TleData>,
	pub orbit: Option<OrbitElements>,
	pub attitude: AttitudeState,
	pub state: OrbitalState,
	pub approach: ApproachMetrics,
}

#[derive(Debug, Clone)]
pub struct TleData {
	pub line1: String,
	pub line2: String,
	pub epoch_utc: Option<String>,
	pub bstar: Option<f32>,
	pub mean_motion: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct OrbitElements {
	pub inclination_deg: f32,
	pub raan_deg: f32,
	pub eccentricity: f32,
	pub arg_perigee_deg: f32,
	pub mean_anomaly_deg: f32,
	pub mean_motion_rev_per_day: f32,
	pub epoch_utc: String,
}

#[derive(Debug, Clone)]
pub struct AttitudeState {
	pub orientation: Quat,
	pub angular_velocity_rad_s: Vec3,
	pub body_axes: BodyAxes,
}

#[derive(Debug, Clone)]
pub struct BodyAxes {
	pub x_body: Vec3,
	pub y_body: Vec3,
	pub z_body: Vec3,
}

#[derive(Debug, Clone)]
pub struct OrbitalState {
	// Earth-centered inertial frame by default.
	pub position_km: DVec3,
	pub velocity_km_s: DVec3,
	pub altitude_km: f32,
}

#[derive(Debug, Clone)]
pub struct ApproachMetrics {
	pub relative_position_km: Vec3,
	pub relative_velocity_km_s: Vec3,
	pub range_km: f32,
	pub closing_speed_km_s: f32,
	pub time_to_closest_approach_s: Option<f32>,
	pub closest_approach_distance_km: Option<f32>,
}

impl Default for AttitudeState {
	fn default() -> Self {
		Self {
			orientation: Quat::IDENTITY,
			angular_velocity_rad_s: Vec3::ZERO,
			body_axes: BodyAxes {
				x_body: Vec3::X,
				y_body: Vec3::Y,
				z_body: Vec3::Z,
			},
		}
	}
}

impl Default for OrbitalState {
	fn default() -> Self {
		Self {
			position_km: DVec3::ZERO,
			velocity_km_s: DVec3::ZERO,
			altitude_km: 0.0,
		}
	}
}

impl Default for ApproachMetrics {
	fn default() -> Self {
		Self {
			relative_position_km: Vec3::ZERO,
			relative_velocity_km_s: Vec3::ZERO,
			range_km: 0.0,
			closing_speed_km_s: 0.0,
			time_to_closest_approach_s: None,
			closest_approach_distance_km: None,
		}
	}
}

impl Default for OrbitalInfo {
	fn default() -> Self {
		Self {
			object_id: String::new(),
			primary_id: String::new(),
			tle: None,
			orbit: None,
			attitude: AttitudeState::default(),
			state: OrbitalState::default(),
			approach: ApproachMetrics::default(),
		}
	}
}
