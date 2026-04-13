use bevy::{ecs::entity::Entity, prelude::Component};
use brahe::KeplerianPropagator;
use nalgebra::Vector6;
use serde::Deserialize;
use serde_json::Value;

// Component to query Earth.
#[derive(Component)]
pub struct Earth;

// A root tether node
#[derive(Component, Debug, Clone)]
pub struct TetherRoot;

// Component for identifying a root node of a tether system.
#[derive(Component, Debug, Clone)]
pub struct TetherNode {
    pub root: Entity,
}

// Orbital parameters and state for a body approaching another object.
#[derive(Component, Debug, Clone)]
pub struct Orbital {
    pub object_id: String,
    pub parent_entity: Option<Entity>,
    pub propagator: Option<KeplerianPropagator>,
}

// Init methods for orbital objects
#[derive(Component, Debug, Clone)]
#[require(Orbital)]
pub enum Orbit {
    FromTle(String),
    FromElements(Vector6<f64>),
}

impl Default for Orbital {
    fn default() -> Self {
        Self {
            object_id: String::new(),
            parent_entity: None,
            propagator: None,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct JsonOrbital {
    #[serde(default)]
    #[serde(rename = "OBJECT_NAME")]
    pub object_name: Option<String>,
    #[serde(default)]
    #[serde(rename = "OBJECT_ID")]
    pub object_id: Option<String>,
    #[serde(default)]
    #[serde(rename = "EPOCH")]
    pub epoch: Option<String>,
    #[serde(default)]
    #[serde(rename = "MEAN_MOTION")]
    pub mean_motion: Option<Value>,
    #[serde(default)]
    #[serde(rename = "ECCENTRICITY")]
    pub eccentricity: Option<Value>,
    #[serde(default)]
    #[serde(rename = "INCLINATION")]
    pub inclination: Option<Value>,
    #[serde(default)]
    #[serde(rename = "RA_OF_ASC_NODE")]
    pub ra_of_asc_node: Option<Value>,
    #[serde(default)]
    #[serde(rename = "ARG_OF_PERICENTER")]
    pub arg_of_pericenter: Option<Value>,
    #[serde(default)]
    #[serde(rename = "MEAN_ANOMALY")]
    pub mean_anomaly: Option<Value>,
    #[serde(default)]
    #[serde(rename = "EPHEMERIS_TYPE")]
    pub ephemeris_type: Option<Value>,
    #[serde(default)]
    #[serde(rename = "CLASSIFICATION_TYPE")]
    pub classification_type: Option<String>,
    #[serde(default)]
    #[serde(rename = "NORAD_CAT_ID")]
    pub norad_cat_id: Option<Value>,
    #[serde(default)]
    #[serde(rename = "ELEMENT_SET_NO")]
    pub element_set_no: Option<Value>,
    #[serde(default)]
    #[serde(rename = "REV_AT_EPOCH")]
    pub rev_at_epoch: Option<Value>,
    #[serde(default)]
    #[serde(rename = "BSTAR")]
    pub bstar: Option<Value>,
    #[serde(default)]
    #[serde(rename = "MEAN_MOTION_DOT")]
    pub mean_motion_dot: Option<Value>,
    #[serde(default)]
    #[serde(rename = "MEAN_MOTION_DDOT")]
    pub mean_motion_ddot: Option<Value>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "JsonOrbitalDataSource")]
pub struct JsonOrbitalData {
    pub name: String,
    pub data: Vec<JsonOrbital>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum JsonOrbitalDataSource {
    Wrapped {
        name: String,
        data: Vec<JsonOrbital>,
    },
    Bare(Vec<JsonOrbital>),
}

impl From<JsonOrbitalDataSource> for JsonOrbitalData {
    fn from(source: JsonOrbitalDataSource) -> Self {
        match source {
            JsonOrbitalDataSource::Wrapped { name, data } => Self { name, data },
            JsonOrbitalDataSource::Bare(data) => Self {
                name: String::new(),
                data,
            },
        }
    }
}
