use bevy::prelude::{Component, Quat, Vec3};

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
	pub position_km: Vec3,
	pub velocity_km_s: Vec3,
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