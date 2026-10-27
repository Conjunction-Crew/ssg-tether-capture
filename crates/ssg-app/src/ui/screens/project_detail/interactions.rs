use super::*;

pub fn project_detail_interactions(
    mut interactions: Query<
        (
            &Interaction,
            Option<&StartSimButton>,
            Option<&BackButton>,
            Option<&CaptureButton>,
            Option<&MapViewButton>,
            Option<&TimeWarpDecreaseButton>,
            Option<&TimeWarpIncreaseButton>,
            Option<&CycleCameraButton>,
            Option<&ExitSimCancelButton>,
            Option<&ExitSimConfirmButton>,
            Option<&ToggleCaptureGizmosButton>,
            &mut BackgroundColor,
        ),
        (
            Changed<Interaction>,
            With<Button>,
            Without<CollapsibleToggle>,
        ),
    >,
    mut commands: Commands,
    exit_modal_query: Query<Entity, With<ExitSimConfirmModal>>,
    capture_entities: Query<Entity, With<CaptureComponent>>,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<ssg_sim::session::UiScreen>>,
    theme: Res<UiTheme>,
    form: Res<NewCapturePlanForm>,
    restart_modal_query: Query<Entity, With<RestartPromptModal>>,
    selection: Res<OrbitalSelectionState>,
) {
    if *screen.get() != ssg_sim::session::UiScreen::Sim {
        return;
    }

    let any_modal_open =
        form.open || !exit_modal_query.is_empty() || !restart_modal_query.is_empty();
    let capture_active = !capture_entities.is_empty();
    let can_start_sim = selection.target.is_some() && selection.chaser.is_some();

    for (
        interaction,
        start_sim,
        back_button,
        capture_button,
        map_view_button,
        time_warp_decrease,
        time_warp_increase,
        cycle_camera_button,
        exit_cancel_button,
        exit_confirm_button,
        toggle_gizmos_button,
        mut background_color,
    ) in &mut interactions
    {
        let is_capture = capture_button.is_some();
        let is_exit_confirm = exit_confirm_button.is_some();
        match *interaction {
            Interaction::Pressed => {
                // Capture button is disabled once capture is active
                if is_capture && capture_active {
                    continue;
                } else if start_sim.is_some() && !can_start_sim {
                    *background_color = BackgroundColor(theme.panel_background);
                    continue;
                }
                *background_color = BackgroundColor(theme.button_background_hover);
                // Exit modal buttons are always allowed
                if start_sim.is_some() {
                    events.write(UiEvent::StartSim);
                } else if exit_cancel_button.is_some() {
                    for entity in &exit_modal_query {
                        commands.entity(entity).despawn();
                    }
                    events.write(UiEvent::CancelExitConfirm);
                } else if exit_confirm_button.is_some() {
                    events.write(UiEvent::BackToHome);
                } else if any_modal_open {
                    // Block all other buttons when a modal is open
                } else if back_button.is_some() {
                    events.write(UiEvent::ShowExitConfirm);
                } else if let Some(capture_entity) = capture_button {
                    events.write(UiEvent::CaptureDebris {
                        entity: capture_entity.entity,
                        plan_id: capture_entity.plan_id.clone(),
                    });
                } else if map_view_button.is_some() {
                    events.write(UiEvent::ToggleMapView);
                } else if time_warp_decrease.is_some() {
                    events.write(UiEvent::ChangeTimeWarp { increase: false });
                } else if time_warp_increase.is_some() {
                    events.write(UiEvent::ChangeTimeWarp { increase: true });
                } else if cycle_camera_button.is_some() {
                    events.write(UiEvent::CycleCameraTarget);
                } else if toggle_gizmos_button.is_some() {
                    events.write(UiEvent::ToggleCaptureGizmos);
                }
            }
            Interaction::Hovered => {
                if is_capture && capture_active {
                    // Keep dimmed appearance — don't show hover highlight
                } else if start_sim.is_some() && !can_start_sim {
                    // Keep dimmed appearance until both target and chaser orbits are selected
                } else if any_modal_open
                    && exit_cancel_button.is_none()
                    && exit_confirm_button.is_none()
                {
                    // Block hover highlight for non-exit-modal buttons when any modal is open
                } else {
                    *background_color = BackgroundColor(theme.button_background);
                }
            }
            Interaction::None => {
                if (is_capture && capture_active) || (start_sim.is_some() && !can_start_sim) {
                    *background_color = BackgroundColor(theme.panel_background);
                } else if is_exit_confirm {
                    // Preserve blue spawn color; avoid blink to soft on first frame
                    *background_color = BackgroundColor(theme.button_background);
                } else {
                    *background_color = BackgroundColor(theme.panel_background_soft);
                }
            }
        }
    }
}

pub fn view_edit_plan_interactions(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>, With<ViewEditPlanButton>),
    >,
    mut events: MessageWriter<UiEvent>,
    selected_project: Res<SelectedProject>,
    theme: Res<UiTheme>,
    form: Res<NewCapturePlanForm>,
    exit_modal: Query<Entity, With<ExitSimConfirmModal>>,
    restart_modal: Query<Entity, With<RestartPromptModal>>,
) {
    let any_modal_open = form.open || !exit_modal.is_empty() || !restart_modal.is_empty();

    for (interaction, mut bg) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                if !any_modal_open && let Some(plan_id) = &selected_project.project_id {
                    events.write(UiEvent::EditCapturePlan(plan_id.clone()));
                }
            }
            Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
            Interaction::None => *bg = BackgroundColor(theme.button_background),
        }
    }
}

pub fn restart_prompt_interactions(
    mut buttons: Query<
        (
            &Interaction,
            Option<&RestartSimButton>,
            Option<&DismissRestartButton>,
            Option<&RecompileLiveButton>,
            &mut BackgroundColor,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut commands: Commands,
    modal_query: Query<Entity, With<RestartPromptModal>>,
    mut sync_state: ResMut<SimPlanSyncState>,
    mut next_screen: ResMut<NextState<ssg_sim::session::UiScreen>>,
    mut next_sim_state: ResMut<NextState<SimState>>,
    mut capture_plan_lib: ResMut<CapturePlanLibrary>,
    theme: Res<UiTheme>,
    mut log: MessageWriter<LogEvent>,
) {
    for (interaction, restart_btn, dismiss_btn, recompile_btn, mut bg) in &mut buttons {
        if restart_btn.is_none() && dismiss_btn.is_none() && recompile_btn.is_none() {
            continue;
        }
        match *interaction {
            Interaction::Pressed => {
                // Despawn modal
                for entity in &modal_query {
                    commands.entity(entity).despawn();
                }
                if restart_btn.is_some() {
                    sync_state.in_sync = true;
                    sync_state.restart_requested = true;
                    log.write(LogEvent {
                        level: LogLevel::Info,
                        source: "ui",
                        message: "Simulation restart requested".to_string(),
                    });
                    // Transition Home → Sim to trigger full cleanup and re-setup
                    next_sim_state.set(SimState::Setup);
                    next_screen.set(ssg_sim::session::UiScreen::Home);
                } else if recompile_btn.is_some() {
                    // Apply changes live without restarting the sim.
                    let refreshed: Vec<_> = capture_plan_lib
                        .plans
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    for (id, plan) in refreshed {
                        capture_plan_lib.insert_plan(id, plan);
                    }
                    sync_state.in_sync = true;
                    log.write(LogEvent {
                        level: LogLevel::Info,
                        source: "ui",
                        message: "Capture plan updated in situ — no restart required".to_string(),
                    });
                } else if dismiss_btn.is_some() {
                    sync_state.in_sync = false;
                }
            }
            Interaction::Hovered => *bg = BackgroundColor(theme.button_background),
            Interaction::None => {
                if restart_btn.is_some() {
                    *bg = BackgroundColor(theme.button_background);
                } else {
                    *bg = BackgroundColor(theme.panel_background_soft);
                }
            }
        }
    }
}

pub fn collapsible_toggle_interaction(
    mut toggles: Query<
        (
            Entity,
            &Interaction,
            &CollapsibleToggle,
            &mut BackgroundColor,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut contents: Query<(&mut Node, &CollapsibleContent)>,
    children_query: Query<&Children>,
    mut texts: Query<&mut Text>,
    form: Res<NewCapturePlanForm>,
    exit_modal: Query<Entity, With<ExitSimConfirmModal>>,
    restart_modal: Query<Entity, With<RestartPromptModal>>,
    theme: Res<UiTheme>,
) {
    let any_modal_open = form.open || !exit_modal.is_empty() || !restart_modal.is_empty();
    for (entity, interaction, toggle, mut bg) in &mut toggles {
        match *interaction {
            Interaction::Pressed => {
                if any_modal_open {
                    continue;
                }
                *bg = BackgroundColor(theme.button_background_hover);
                let mut collapsed = false;
                for (mut node, content) in &mut contents {
                    if content.section == toggle.section {
                        if node.display == Display::None {
                            node.display = Display::Flex;
                        } else {
                            node.display = Display::None;
                            collapsed = true;
                        }
                    }
                }
                if let Ok(children) = children_query.get(entity) {
                    for child in children {
                        if let Ok(mut text) = texts.get_mut(*child) {
                            text.0 = if collapsed {
                                "<".to_string()
                            } else {
                                "v".to_string()
                            };
                        }
                    }
                }
            }
            Interaction::Hovered => {
                if !any_modal_open {
                    *bg = BackgroundColor(theme.button_background);
                }
            }
            Interaction::None => {
                *bg = BackgroundColor(theme.panel_background_soft);
            }
        }
    }
}

pub fn update_sync_indicator(
    mut indicators: Query<&mut Node, With<SyncIndicator>>,
    sync_state: Res<SimPlanSyncState>,
) {
    for mut node in &mut indicators {
        node.display = if sync_state.in_sync {
            Display::None
        } else {
            Display::Flex
        };
    }
}
