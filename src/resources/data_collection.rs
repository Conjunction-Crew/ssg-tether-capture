use bevy::prelude::*;
use bevy_egui::egui::ahash::{HashMap, HashMapExt};

#[derive(Debug)]
pub struct DataCollectionSettings {
    pub selecting_csv_dir: bool,
    pub csv_export_filename: String,
    pub num_exports_completed: u32,
}

#[derive(Resource, Debug)]
pub struct DataCollection {
    pub velocity: HashMap<Entity, Vec<(f64, f64)>>,
    pub position: HashMap<Entity, Vec<(f64, f64)>>,
    pub forces: HashMap<Entity, Vec<(f64, f64)>>,
    pub settings: DataCollectionSettings,
}

impl Default for DataCollection {
    fn default() -> Self {
        Self {
            velocity: HashMap::new(),
            position: HashMap::new(),
            forces: HashMap::new(),
            settings: DataCollectionSettings {
                selecting_csv_dir: false,
                csv_export_filename: String::from("capture0.csv"),
                num_exports_completed: 0,
            },
        }
    }
}
