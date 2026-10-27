use super::*;

// ── Sync form fields from InputField components back into the resource ────

pub fn sync_form_fields(
    field_query: Query<(&InputField, &FormFieldId), Changed<InputField>>,
    mut form: ResMut<NewCapturePlanForm>,
) {
    for (field, id) in &field_query {
        match id {
            FormFieldId::PlanName => form.plan_name = field.value.clone(),
            FormFieldId::TetherName => form.tether_name = field.value.clone(),
            FormFieldId::TetherLength => form.tether_length = field.value.clone(),
            FormFieldId::ApproachMaxVelocity => form.approach_max_velocity = field.value.clone(),
            FormFieldId::ApproachMaxForce => form.approach_max_force = field.value.clone(),
            FormFieldId::TerminalMaxVelocity => form.terminal_max_velocity = field.value.clone(),
            FormFieldId::TerminalMaxForce => form.terminal_max_force = field.value.clone(),
            FormFieldId::TerminalShrinkRate => form.terminal_shrink_rate = field.value.clone(),
            FormFieldId::CaptureMaxVelocity => form.capture_max_velocity = field.value.clone(),
            FormFieldId::CaptureMaxForce => form.capture_max_force = field.value.clone(),
            FormFieldId::CaptureShrinkRate => form.capture_shrink_rate = field.value.clone(),
            FormFieldId::ApproachTransitionDistanceValue(i) => {
                if let Some(t) = form.approach_transitions.get_mut(*i) {
                    t.distance_value = field.value.clone();
                }
            }
            FormFieldId::TerminalTransitionDistanceValue(i) => {
                if let Some(t) = form.terminal_transitions.get_mut(*i) {
                    t.distance_value = field.value.clone();
                }
            }
        }
    }
}

// ── Button interaction system ─────────────────────────────────────────────

pub fn capture_plan_interactions(
    mut buttons: Query<
        (
            &Interaction,
            &mut BackgroundColor,
            Option<&NewPlanCancelButton>,
            Option<&NewPlanSaveButton>,
            Option<&AddApproachTransitionButton>,
            Option<&AddTerminalTransitionButton>,
            Option<&RemoveApproachTransitionButton>,
            Option<&RemoveTerminalTransitionButton>,
            Option<&ConfirmOverwriteButton>,
            Option<&CancelOverwriteButton>,
            Option<&UnitMetricButton>,
            Option<&UnitImperialButton>,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut events: MessageWriter<UiEvent>,
    screen: Res<State<UiScreen>>,
    form: Res<NewCapturePlanForm>,
    theme: Res<UiTheme>,
) {
    if *screen.get() != UiScreen::Home && *screen.get() != UiScreen::Sim {
        return;
    }
    if !form.open {
        return;
    }

    for (
        interaction,
        mut bg,
        cancel,
        save,
        add_approach,
        add_terminal,
        remove_approach,
        remove_terminal,
        confirm_overwrite,
        cancel_overwrite,
        unit_metric,
        unit_imperial,
    ) in &mut buttons
    {
        if cancel.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::CloseNewCapturePlanForm);
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.panel_background_soft),
                Interaction::None => *bg = BackgroundColor(theme.panel_background),
            }
        } else if save.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::SaveCapturePlan);
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
                Interaction::None => *bg = BackgroundColor(theme.button_background),
            }
        } else if add_approach.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::AddApproachTransition);
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.panel_background),
                Interaction::None => *bg = BackgroundColor(theme.panel_background_soft),
            }
        } else if add_terminal.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::AddTerminalTransition);
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.panel_background),
                Interaction::None => *bg = BackgroundColor(theme.panel_background_soft),
            }
        } else if let Some(btn) = remove_approach {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::RemoveApproachTransition(btn.0));
                }
                Interaction::Hovered => *bg = BackgroundColor(Color::srgba(0.75, 0.2, 0.2, 0.65)),
                Interaction::None => *bg = BackgroundColor(Color::srgba(0.6, 0.15, 0.15, 0.5)),
            }
        } else if let Some(btn) = remove_terminal {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::RemoveTerminalTransition(btn.0));
                }
                Interaction::Hovered => *bg = BackgroundColor(Color::srgba(0.75, 0.2, 0.2, 0.65)),
                Interaction::None => *bg = BackgroundColor(Color::srgba(0.6, 0.15, 0.15, 0.5)),
            }
        } else if confirm_overwrite.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::ConfirmOverwriteCapturePlan);
                }
                Interaction::Hovered => *bg = BackgroundColor(Color::srgb(0.8, 0.25, 0.25)),
                Interaction::None => *bg = BackgroundColor(Color::srgb(0.7, 0.2, 0.2)),
            }
        } else if cancel_overwrite.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::CancelOverwriteCapturePlan);
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.panel_background),
                Interaction::None => *bg = BackgroundColor(theme.panel_background_soft),
            }
        } else if unit_metric.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::SetUnitSystem(UnitSystem::Metric));
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
                Interaction::None => {}
            }
        } else if unit_imperial.is_some() {
            match *interaction {
                Interaction::Pressed => {
                    events.write(UiEvent::SetUnitSystem(UnitSystem::Imperial));
                }
                Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
                Interaction::None => {}
            }
        }
    }
}

pub fn dropdown_interactions(
    mut buttons: Query<
        (
            &Interaction,
            Option<&DropdownTrigger>,
            Option<&DropdownOption>,
            &mut BackgroundColor,
        ),
        (
            Changed<Interaction>,
            With<Button>,
            Or<(With<DropdownTrigger>, With<DropdownOption>)>,
        ),
    >,
    mut options_containers: Query<(&DropdownOptions, &mut Node)>,
    mut form: ResMut<NewCapturePlanForm>,
    theme: Res<UiTheme>,
) {
    if !form.open {
        return;
    }
    for (interaction, trigger, option, mut bg) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                if let Some(trig) = trigger {
                    *bg = BackgroundColor(theme.button_background);
                    // Toggle this dropdown; close all others
                    for (opts, mut node) in &mut options_containers {
                        if opts.is_approach == trig.is_approach
                            && opts.index == trig.index
                            && opts.field == trig.field
                        {
                            node.display = if node.display == Display::None {
                                Display::Flex
                            } else {
                                Display::None
                            };
                        } else {
                            node.display = Display::None;
                        }
                    }
                } else if let Some(opt) = option {
                    *bg = BackgroundColor(theme.button_background);
                    // Write form value directly
                    let transitions = if opt.is_approach {
                        &mut form.approach_transitions
                    } else {
                        &mut form.terminal_transitions
                    };
                    if let Some(t) = transitions.get_mut(opt.index) {
                        match opt.field {
                            DropdownField::To => t.to = opt.value.clone(),
                            DropdownField::Condition => t.distance_kind = opt.value.clone(),
                        }
                    }
                    // Close the dropdown
                    for (opts, mut node) in &mut options_containers {
                        if opts.is_approach == opt.is_approach
                            && opts.index == opt.index
                            && opts.field == opt.field
                        {
                            node.display = Display::None;
                        }
                    }
                }
            }
            Interaction::Hovered => {
                *bg = BackgroundColor(theme.button_background);
            }
            Interaction::None => {
                if trigger.is_some() {
                    *bg = BackgroundColor(theme.panel_background_soft);
                } else if let Some(opt) = option {
                    let transitions = if opt.is_approach {
                        &form.approach_transitions
                    } else {
                        &form.terminal_transitions
                    };
                    let selected = transitions
                        .get(opt.index)
                        .map(|t| match opt.field {
                            DropdownField::To => t.to == opt.value,
                            DropdownField::Condition => t.distance_kind == opt.value,
                        })
                        .unwrap_or(false);
                    *bg = if selected {
                        BackgroundColor(theme.button_background)
                    } else {
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0))
                    };
                }
            }
        }
    }
}

pub fn sync_dropdown_labels(
    form: Res<NewCapturePlanForm>,
    triggers: Query<(&DropdownTrigger, &Children)>,
    mut labels: Query<&mut Text, With<DropdownTriggerLabel>>,
) {
    if !form.is_changed() {
        return;
    }
    for (trigger, children) in &triggers {
        let transitions = if trigger.is_approach {
            &form.approach_transitions
        } else {
            &form.terminal_transitions
        };
        let current = transitions
            .get(trigger.index)
            .map(|t| match trigger.field {
                DropdownField::To => t.to.as_str(),
                DropdownField::Condition => t.distance_kind.as_str(),
            })
            .unwrap_or("");
        for child in children.iter() {
            if let Ok(mut text) = labels.get_mut(child) {
                text.0 = if current.is_empty() {
                    "—".to_string()
                } else {
                    current.to_string()
                };
            }
        }
    }
}

pub fn tether_type_radio_interactions(
    mut buttons: Query<
        (&Interaction, &TetherTypeRadioButton, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
    mut form: ResMut<NewCapturePlanForm>,
    theme: Res<UiTheme>,
) {
    if !form.open {
        return;
    }
    for (interaction, radio, mut bg) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                form.tether_type = radio.0.clone();
            }
            Interaction::Hovered => *bg = BackgroundColor(theme.button_background_hover),
            Interaction::None => {
                if form.tether_type == radio.0 {
                    *bg = BackgroundColor(theme.button_background);
                } else {
                    *bg = BackgroundColor(theme.panel_background_soft);
                }
            }
        }
    }
}
