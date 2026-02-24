use bevy::prelude::*;

#[derive(States, Debug, Clone, Eq, PartialEq, Hash, Default)]
pub enum UiScreen {
	#[default]
	Home,
	ProjectDetail,
}

#[derive(Debug, Clone)]
pub struct MockProject {
	pub id: String,
	pub title: String,
	pub description: String,
	pub tether_id: String,
}

#[derive(Resource, Debug, Clone)]
pub struct ProjectCatalog {
	pub projects: Vec<MockProject>,
}

impl Default for ProjectCatalog {
	fn default() -> Self {
		Self {
			projects: vec![MockProject {
				id: "tether-1".to_string(),
				title: "SSG Tether 1".to_string(),
				description: "Baseline tether scenario initialized from ISS orbital elements."
					.to_string(),
				tether_id: "Tether1".to_string(),
			}],
		}
	}
}

#[derive(Resource, Debug, Clone, Default)]
pub struct SelectedProject {
	pub project_id: Option<String>,
}
