#[cfg(test)]
mod tests {
    use crate::ui::form::{NewCapturePlanForm, TransitionForm, UnitSystem};
    use crate::ui::log_ui::CaptureLogUiState;
    use crate::ui::screens::capture_plan::{
        build_capture_plan_json, generate_filename, validate_form,
    };
    use ssg_sim::telemetry::LogLevel;

    // -------------------------------------------------------------------------
    // generate_filename
    // -------------------------------------------------------------------------

    #[test]
    fn generate_filename_replaces_spaces_with_underscores() {
        assert_eq!(generate_filename("My Test Plan"), "my_test_plan.json");
    }

    #[test]
    fn generate_filename_replaces_illegal_chars_with_underscores() {
        // Illegal chars (<>:"/\|?*) are replaced with underscores, not stripped
        let result = generate_filename("Plan<>:");
        assert!(result.starts_with("plan___"));
        assert!(result.ends_with(".json"));
    }

    #[test]
    fn generate_filename_appends_json_extension() {
        let name = "approach_v2";
        assert!(generate_filename(name).ends_with(".json"));
    }

    // -------------------------------------------------------------------------
    // validate_form
    // -------------------------------------------------------------------------

    fn valid_form() -> NewCapturePlanForm {
        NewCapturePlanForm {
            open: true,
            plan_name: "My Plan".to_string(),
            tether_name: "Tether1".to_string(),
            tether_type: "tether".to_string(),
            tether_length: "20.0".to_string(),
            approach_max_velocity: "1.0".to_string(),
            approach_max_force: "2.0".to_string(),
            terminal_max_velocity: "0.2".to_string(),
            terminal_max_force: "2.0".to_string(),
            terminal_shrink_rate: "0.125".to_string(),
            capture_max_velocity: "0.1".to_string(),
            capture_max_force: "2.0".to_string(),
            capture_shrink_rate: "0.025".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn validate_form_passes_on_valid_form() {
        assert!(validate_form(&valid_form()).is_empty());
    }

    #[test]
    fn validate_form_errors_on_empty_plan_name() {
        let mut form = valid_form();
        form.plan_name = "  ".to_string();
        let errors = validate_form(&form);
        assert!(errors.iter().any(|e| e.contains("Plan Name")));
    }

    #[test]
    fn validate_form_errors_on_empty_tether_name() {
        let mut form = valid_form();
        form.tether_name = String::new();
        let errors = validate_form(&form);
        assert!(errors.iter().any(|e| e.contains("Tether Name")));
    }

    #[test]
    fn validate_form_errors_on_zero_tether_length() {
        let mut form = valid_form();
        form.tether_length = "0.0".to_string();
        let errors = validate_form(&form);
        assert!(errors.iter().any(|e| e.contains("Tether Length")));
    }

    #[test]
    fn validate_form_errors_on_negative_tether_length() {
        let mut form = valid_form();
        form.tether_length = "-5.0".to_string();
        let errors = validate_form(&form);
        assert!(errors.iter().any(|e| e.contains("Tether Length")));
    }

    #[test]
    fn validate_form_errors_on_non_numeric_tether_length() {
        let mut form = valid_form();
        form.tether_length = "abc".to_string();
        let errors = validate_form(&form);
        assert!(errors.iter().any(|e| e.contains("Tether Length")));
    }

    #[test]
    fn validate_form_errors_on_non_numeric_velocity() {
        let mut form = valid_form();
        form.approach_max_velocity = "fast".to_string();
        let errors = validate_form(&form);
        assert!(errors.iter().any(|e| e.contains("Approach Max Velocity")));
    }

    #[test]
    fn validate_form_errors_on_empty_approach_transition_to() {
        let mut form = valid_form();
        form.approach_transitions.push(TransitionForm {
            to: String::new(),
            distance_kind: "less_than".to_string(),
            distance_value: "50.0".to_string(),
        });
        let errors = validate_form(&form);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Approach Transition 1") && e.contains("To State"))
        );
    }

    #[test]
    fn validate_form_errors_on_empty_approach_transition_condition() {
        let mut form = valid_form();
        form.approach_transitions.push(TransitionForm {
            to: "terminal".to_string(),
            distance_kind: String::new(),
            distance_value: "50.0".to_string(),
        });
        let errors = validate_form(&form);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Approach Transition 1") && e.contains("condition"))
        );
    }

    #[test]
    fn validate_form_errors_on_non_numeric_approach_transition_distance() {
        let mut form = valid_form();
        form.approach_transitions.push(TransitionForm {
            to: "terminal".to_string(),
            distance_kind: "less_than".to_string(),
            distance_value: "far".to_string(),
        });
        let errors = validate_form(&form);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Approach Transition 1") && e.contains("number"))
        );
    }

    #[test]
    fn validate_form_errors_on_terminal_transition_fields() {
        let mut form = valid_form();
        form.terminal_transitions.push(TransitionForm {
            to: String::new(),
            distance_kind: String::new(),
            distance_value: String::new(),
        });
        let errors = validate_form(&form);
        assert!(errors.iter().any(|e| e.contains("Terminal Transition 1")));
    }

    // -------------------------------------------------------------------------
    // build_capture_plan_json
    // -------------------------------------------------------------------------

    #[test]
    fn build_plan_json_has_correct_top_level_keys() {
        let form = valid_form();
        let json = build_capture_plan_json(&form);
        assert_eq!(json["name"], "My Plan");
        assert_eq!(json["tether"], "Tether1");
        assert!(json["states"].is_array());
        assert_eq!(json["states"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn build_plan_json_state_ids_are_ordered() {
        let form = valid_form();
        let json = build_capture_plan_json(&form);
        let states = json["states"].as_array().unwrap();
        assert_eq!(states[0]["id"], "approach");
        assert_eq!(states[1]["id"], "terminal");
        assert_eq!(states[2]["id"], "capture");
    }

    #[test]
    fn build_plan_json_empty_transitions_produce_empty_array() {
        let form = valid_form(); // no transitions by default
        let json = build_capture_plan_json(&form);
        assert_eq!(
            json["states"][0]["transitions"].as_array().unwrap().len(),
            0
        );
    }

    #[test]
    fn build_plan_json_transition_uses_distance_kind_as_key() {
        let mut form = valid_form();
        form.approach_transitions.push(TransitionForm {
            to: "terminal".to_string(),
            distance_kind: "less_than".to_string(),
            distance_value: "50.0".to_string(),
        });
        let json = build_capture_plan_json(&form);
        let transition = &json["states"][0]["transitions"][0];
        assert_eq!(transition["to"], "terminal");
        // The condition key should be "less_than", not hardcoded
        assert!(!transition["distance"]["less_than"].is_null());
    }

    #[test]
    fn build_plan_json_greater_than_condition_uses_correct_key() {
        let mut form = valid_form();
        form.approach_transitions.push(TransitionForm {
            to: "capture".to_string(),
            distance_kind: "greater_than".to_string(),
            distance_value: "100.0".to_string(),
        });
        let json = build_capture_plan_json(&form);
        let dist = &json["states"][0]["transitions"][0]["distance"];
        assert!(!dist["greater_than"].is_null());
        assert!(dist["less_than"].is_null());
    }

    #[test]
    fn build_plan_json_metric_values_are_not_converted() {
        let mut form = valid_form();
        form.unit_system = UnitSystem::Metric;
        form.approach_max_velocity = "2.0".to_string();
        let json = build_capture_plan_json(&form);
        let v = json["states"][0]["parameters"]["max_velocity"]
            .as_f64()
            .unwrap();
        assert!((v - 2.0).abs() < 1e-9);
    }

    #[test]
    fn build_plan_json_imperial_velocity_converts_to_metric() {
        let mut form = valid_form();
        form.unit_system = UnitSystem::Imperial;
        form.approach_max_velocity = "1.0".to_string(); // 1 ft/s
        let json = build_capture_plan_json(&form);
        let v = json["states"][0]["parameters"]["max_velocity"]
            .as_f64()
            .unwrap();
        // 1 ft/s = 0.3048 m/s
        assert!((v - 0.3048).abs() < 1e-9);
    }

    #[test]
    fn build_plan_json_imperial_force_converts_to_metric() {
        let mut form = valid_form();
        form.unit_system = UnitSystem::Imperial;
        form.approach_max_force = "1.0".to_string(); // 1 lbf
        let json = build_capture_plan_json(&form);
        let f = json["states"][0]["parameters"]["max_force"]
            .as_f64()
            .unwrap();
        // 1 lbf = 4.44822 N
        assert!((f - 4.44822).abs() < 1e-4);
    }

    // The form still builds v0-shaped JSON; saving migrates it to a v1 plan
    // (F-15, P6): no warning, nothing unmapped, the same limits and transitions.
    #[test]
    fn form_json_migrates_to_a_v1_plan_without_warnings() {
        use ssg_sim::project::plan_files::{plan_to_json, read_plan_value};
        let mut form = valid_form();
        form.terminal_transitions.push(TransitionForm {
            to: "capture".to_string(),
            distance_kind: "less_than".to_string(),
            distance_value: "5.0".to_string(),
        });
        let report = read_plan_value("my_plan", &build_capture_plan_json(&form));
        assert_eq!(report.errors, Vec::<String>::new());
        assert_eq!(report.warnings, Vec::<String>::new());
        let plan = report.plan.unwrap();
        assert_eq!(plan.migrated_from_v0, None);
        let ids: Vec<&str> = plan.phases.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["approach", "terminal", "capture"]);
        assert_eq!(plan.phases[1].shrink_rate_mps, Some(0.125));
        let transition = &plan.phases[1].transitions.as_ref().unwrap()[0];
        assert_eq!(transition.to, "capture");
        assert_eq!(transition.all_of.distance_m.unwrap().less_than, Some(5.0));
        assert_eq!(plan.device.as_ref().unwrap().tether_length_m, Some(20.0));
        assert!(plan_to_json(&plan).starts_with("{\n  \"schema_version\": 1,"));
    }

    // -------------------------------------------------------------------------
    // CaptureLogUiState defaults (A3)
    // -------------------------------------------------------------------------

    #[test]
    fn capture_log_ui_state_default_has_all_filters_active() {
        let state = CaptureLogUiState::default();
        assert!(state.active_filters.contains(&LogLevel::Error));
        assert!(state.active_filters.contains(&LogLevel::Warn));
        assert!(state.active_filters.contains(&LogLevel::Info));
        assert!(state.active_filters.contains(&LogLevel::Debug));
        assert!(!state.is_open);
        assert!(state.selected_rows.is_none());
        assert!(state.selection_anchor.is_none());
    }

    #[test]
    fn capture_log_ui_state_filter_toggle_removes_level() {
        let mut state = CaptureLogUiState::default();
        state.active_filters.remove(&LogLevel::Debug);
        assert_eq!(state.active_filters.len(), 3);
        assert!(!state.active_filters.contains(&LogLevel::Debug));
        assert!(state.active_filters.contains(&LogLevel::Info));
    }
}
