use bevy::ecs::system::SystemParam;

use super::orbital_selection::{
    catalog_pick, clear_orbit, custom_orbit, set_orbit, set_orbital_element,
};
use super::*;
use ssg_sim::time::SimConfig;

#[derive(SystemParam)]
pub struct SimScreenEventParams<'w> {
    form: ResMut<'w, NewCapturePlanForm>,
    sync_state: ResMut<'w, SimPlanSyncState>,
    next_screen: ResMut<'w, NextState<UiScreen>>,
    next_sim_state: ResMut<'w, NextState<SimState>>,
    capture_plan_lib: ResMut<'w, CapturePlanLibrary>,
    catalog_ui: ResMut<'w, SpaceCatalogUiState>,
    log: MessageWriter<'w, LogEvent>,
    selection: ResMut<'w, OrbitalSelectionState>,
    catalog: Option<Res<'w, SpaceObjectCatalog>>,
    sim_config: Res<'w, SimConfig>,
}

/// The sidebar's and the restart prompt's events that `handle_ui_events` does
/// not own: the rc.3 button behaviour, moved here unchanged.
pub fn sim_screen_events(mut events: MessageReader<UiEvent>, mut p: SimScreenEventParams) {
    for event in events.read() {
        match event {
            UiEvent::RestartSim => {
                p.form.show_restart_prompt = false;
                p.sync_state.in_sync = true;
                p.sync_state.restart_requested = true;
                p.log.write(LogEvent {
                    level: LogLevel::Info,
                    source: "ui",
                    message: "Simulation restart requested".to_string(),
                });
                // Transition Home → Sim to trigger full cleanup and re-setup
                p.next_sim_state.set(SimState::Setup);
                p.next_screen.set(UiScreen::Home);
            }
            UiEvent::ApplyPlanLive => {
                p.form.show_restart_prompt = false;
                // Apply changes live without restarting the sim.
                let refreshed: Vec<_> = p
                    .capture_plan_lib
                    .plans
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (id, plan) in refreshed {
                    p.capture_plan_lib.insert_plan(id, plan);
                }
                p.sync_state.in_sync = true;
                p.log.write(LogEvent {
                    level: LogLevel::Info,
                    source: "ui",
                    message: "Capture plan updated in situ — no restart required".to_string(),
                });
            }
            UiEvent::DismissRestartPrompt => {
                p.form.show_restart_prompt = false;
                p.sync_state.in_sync = false;
            }
            UiEvent::ToggleSatelliteIndicator => {
                p.catalog_ui.show_satellite_indicator = !p.catalog_ui.show_satellite_indicator;
            }
            UiEvent::UseCatalogSelection(role) => {
                let entry = p.catalog_ui.selected_index.and_then(|index| {
                    let entry = p.catalog.as_ref()?.entries.get(index)?;
                    Some(catalog_pick(index, entry))
                });
                if let Some(selected) = entry {
                    set_orbit(&mut p.selection, *role, selected);
                    // F-45: the row is used; the catalog stays open for the other role.
                    p.catalog_ui.selected_index = None;
                }
            }
            UiEvent::SelectFromCatalog(_) => {
                // F-45: `handle_ui_events` switches the camera to the map view.
                p.catalog_ui.show_catalog = true;
            }
            UiEvent::ClearOrbitalSelection(role) => {
                clear_orbit(&mut p.selection, *role);
            }
            UiEvent::CreateCustomOrbit(role) => {
                let selected = custom_orbit(*role, p.sim_config.start_epoch);
                set_orbit(&mut p.selection, *role, selected);
            }
            UiEvent::SetOrbitalElement { role, field, value } => {
                set_orbital_element(&mut p.selection, *role, *field, *value);
            }
            _ => {}
        }
    }
}
