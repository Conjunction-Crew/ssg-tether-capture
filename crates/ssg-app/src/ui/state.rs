use bevy::prelude::*;

#[derive(States, Debug, Clone, Eq, PartialEq, Hash, Default)]
pub enum UiScreen {
    #[default]
    WorkingDirectorySetup,
    Home,
    Sim,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct SelectedProject {
    pub project_id: Option<String>,
}
