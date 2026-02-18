use bevy::{ecs::entity::Entity, math::DVec3, prelude::{Component, Quat, Vec3}};
use astrora_core::core::elements::OrbitalElements;

#[derive(Debug, Clone)]
pub enum PhysicsState {
	INACTIVE,
	ACTIVE
}

// Orbital parameters and state for a body approaching another object.
#[derive(Component, Debug, Clone)]
pub struct Orbital {
	pub object_id: String,
	pub parent_entity: Option<Entity>,
	pub primary_id: String,
	pub tle: Option<TleData>,
	pub elements: Option<OrbitalElements>,
	pub attitude: AttitudeState,
	pub state: PhysicsState,
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
pub struct ApproachMetrics {
	pub relative_position_km: DVec3,
	pub relative_velocity_km_s: DVec3,
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

impl Default for ApproachMetrics {
	fn default() -> Self {
		Self {
			relative_position_km: DVec3::ZERO,
			relative_velocity_km_s: DVec3::ZERO,
			range_km: 0.0,
			closing_speed_km_s: 0.0,
			time_to_closest_approach_s: None,
			closest_approach_distance_km: None,
		}
	}
}

impl Default for Orbital {
	fn default() -> Self {
		Self {
			object_id: String::new(),
			parent_entity: None,
			primary_id: String::new(),
			tle: None,
			elements: None,
			attitude: AttitudeState::default(),
			state: PhysicsState::INACTIVE,
			approach: ApproachMetrics::default(),
		}
	}
}
