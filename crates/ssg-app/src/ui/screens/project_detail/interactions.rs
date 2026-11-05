use bevy::ecs::system::SystemParam;

use super::*;

#[derive(SystemParam)]
pub struct SimScreenEventParams<'w> {
    form: ResMut<'w, NewCapturePlanForm>,
    sync_state: ResMut<'w, SimPlanSyncState>,
    next_screen: ResMut<'w, NextState<UiScreen>>,
    next_sim_state: ResMut<'w, NextState<SimState>>,
    capture_plan_lib: ResMut<'w, CapturePlanLibrary>,
    catalog_ui: ResMut<'w, SpaceCatalogUiState>,
    log: MessageWriter<'w, LogEvent>,
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
            _ => {}
        }
    }
}
