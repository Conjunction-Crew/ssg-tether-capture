#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::components::capture_components::{CapturePlan, State};
    use crate::resources::capture_plans::{
        build_capture_component, compile_capture_plan, load_plans_from_dir_with_errors,
        parameter_value, validate_capture_plan,
    };
    use crate::ui::screens::capture_plan::generate_filename;

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
    // validate_capture_plan
    // -------------------------------------------------------------------------

    fn minimal_valid_plan() -> CapturePlan {
        CapturePlan {
            name: "Test Plan".to_string(),
            tether: "Tether1".to_string(),
            states: vec![State {
                id: "approach".to_string(),
                next: None,
                parameters: Some(json!({ "max_velocity": 1.0, "max_force": 2.0 })),
                transitions: None,
                next_conditions: None,
            }],
            device: None,
        }
    }

    #[test]
    fn validate_plan_passes_for_valid_plan() {
        let plan = minimal_valid_plan();
        assert!(validate_capture_plan("test_plan", &plan).is_empty());
    }

    #[test]
    fn validate_plan_errors_on_empty_name() {
        let mut plan = minimal_valid_plan();
        plan.name = "  ".to_string();
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("'name'")));
    }

    #[test]
    fn validate_plan_errors_on_empty_tether() {
        let mut plan = minimal_valid_plan();
        plan.tether = String::new();
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("'tether'")));
    }

    #[test]
    fn validate_plan_errors_on_empty_states() {
        let mut plan = minimal_valid_plan();
        plan.states.clear();
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("'states'")));
    }

    #[test]
    fn validate_plan_errors_on_missing_max_velocity() {
        let mut plan = minimal_valid_plan();
        plan.states[0].parameters = Some(json!({ "max_force": 2.0 }));
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("max_velocity")));
    }

    #[test]
    fn validate_plan_errors_on_missing_max_force() {
        let mut plan = minimal_valid_plan();
        plan.states[0].parameters = Some(json!({ "max_velocity": 1.0 }));
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("max_force")));
    }

    #[test]
    fn validate_plan_errors_on_transition_to_unknown_state() {
        let mut plan = minimal_valid_plan();
        plan.states[0].transitions = Some(vec![json!({ "to": "nonexistent" })]);
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("nonexistent")));
    }

    #[test]
    fn validate_plan_errors_on_transition_missing_to_field() {
        let mut plan = minimal_valid_plan();
        plan.states[0].transitions = Some(vec![json!({ "distance": { "less_than": 10.0 } })]);
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("'to' field")));
    }

    #[test]
    fn validate_plan_accepts_valid_internal_transition() {
        let plan = CapturePlan {
            name: "Two State Plan".to_string(),
            tether: "Tether1".to_string(),
            states: vec![
                State {
                    id: "approach".to_string(),
                    next: None,
                    parameters: Some(json!({ "max_velocity": 1.0, "max_force": 2.0 })),
                    transitions: Some(vec![json!({ "to": "capture" })]),
                    next_conditions: None,
                },
                State {
                    id: "capture".to_string(),
                    next: None,
                    parameters: Some(json!({ "max_velocity": 0.5, "max_force": 2.0 })),
                    transitions: None,
                    next_conditions: None,
                },
            ],
            device: None,
        };
        assert!(validate_capture_plan("two_state_plan", &plan).is_empty());
    }

    #[test]
    fn validate_plan_errors_on_duplicate_state_ids() {
        let plan = CapturePlan {
            name: "Bad Plan".to_string(),
            tether: "Tether1".to_string(),
            states: vec![
                State {
                    id: "approach".to_string(),
                    next: None,
                    parameters: Some(json!({ "max_velocity": 1.0, "max_force": 2.0 })),
                    transitions: None,
                    next_conditions: None,
                },
                State {
                    id: "approach".to_string(), // duplicate
                    next: None,
                    parameters: Some(json!({ "max_velocity": 0.5, "max_force": 2.0 })),
                    transitions: None,
                    next_conditions: None,
                },
            ],
            device: None,
        };
        let errors = validate_capture_plan("bad_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("Duplicate")));
    }

    // -------------------------------------------------------------------------
    // compile_capture_plan
    // -------------------------------------------------------------------------

    #[test]
    fn compile_plan_indexes_states_by_id() {
        let plan = CapturePlan {
            name: "Compiled Plan".to_string(),
            tether: "Tether1".to_string(),
            states: vec![
                State {
                    id: "approach".to_string(),
                    next: None,
                    parameters: Some(json!({ "max_velocity": 1.0, "max_force": 2.0 })),
                    transitions: None,
                    next_conditions: None,
                },
                State {
                    id: "capture".to_string(),
                    next: None,
                    parameters: Some(
                        json!({ "max_velocity": 0.5, "max_force": 2.0, "shrink_rate": 0.01 }),
                    ),
                    transitions: None,
                    next_conditions: None,
                },
            ],
            device: None,
        };
        let compiled = compile_capture_plan(&plan);
        assert!(compiled.state("approach").is_some());
        assert!(compiled.state("capture").is_some());
        assert!(compiled.state("nonexistent").is_none());
    }

    #[test]
    fn compile_plan_parses_parameters_correctly() {
        let plan = CapturePlan {
            name: "Param Plan".to_string(),
            tether: "Tether1".to_string(),
            states: vec![State {
                id: "terminal".to_string(),
                next: None,
                parameters: Some(
                    json!({ "max_velocity": 0.3, "max_force": 1.5, "shrink_rate": 0.005 }),
                ),
                transitions: None,
                next_conditions: None,
            }],
            device: None,
        };
        let compiled = compile_capture_plan(&plan);
        let state = compiled.state("terminal").unwrap();
        assert!((state.parameters.max_velocity - 0.3).abs() < f64::EPSILON);
        assert!((state.parameters.max_force - 1.5).abs() < f64::EPSILON);
        assert!((state.parameters.shrink_rate.unwrap() - 0.005).abs() < f64::EPSILON);
    }

    #[test]
    fn compile_plan_parses_distance_transition_conditions() {
        let plan = CapturePlan {
            name: "Transition Plan".to_string(),
            tether: "Tether1".to_string(),
            states: vec![State {
                id: "approach".to_string(),
                next: None,
                parameters: Some(json!({ "max_velocity": 1.0, "max_force": 2.0 })),
                transitions: Some(vec![
                    json!({ "to": "approach", "distance": { "less_than": 50.0 } }),
                ]),
                next_conditions: None,
            }],
            device: None,
        };
        let compiled = compile_capture_plan(&plan);
        let transition = &compiled.state("approach").unwrap().transitions[0];
        assert_eq!(transition.to, "approach");
        assert_eq!(transition.distance_less_than, Some(50.0));
        assert_eq!(transition.distance_greater_than, None);
    }

    // -------------------------------------------------------------------------
    // build_capture_component
    // -------------------------------------------------------------------------

    #[test]
    fn build_capture_component_stores_plan_id_not_display_name() {
        let plan = minimal_valid_plan(); // plan.name == "Test Plan"
        let component = build_capture_component("test_plan", &plan, 0.0).unwrap();
        // Must store the HashMap key (file stem), NOT plan.name
        assert_eq!(component.plan_id, "test_plan");
        assert_ne!(component.plan_id, plan.name);
    }

    #[test]
    fn build_capture_component_sets_first_state() {
        let plan = CapturePlan {
            name: "Plan".to_string(),
            tether: "Tether1".to_string(),
            states: vec![
                State {
                    id: "approach".to_string(),
                    next: None,
                    parameters: None,
                    transitions: None,
                    next_conditions: None,
                },
                State {
                    id: "capture".to_string(),
                    next: None,
                    parameters: None,
                    transitions: None,
                    next_conditions: None,
                },
            ],
            device: None,
        };
        let component = build_capture_component("plan", &plan, 123.0).unwrap();
        assert_eq!(component.current_state, "approach");
        assert!((component.state_enter_time_s - 123.0).abs() < f64::EPSILON);
    }

    #[test]
    fn build_capture_component_returns_none_for_empty_states() {
        let plan = CapturePlan {
            name: "Empty".to_string(),
            tether: "Tether1".to_string(),
            states: vec![],
            device: None,
        };
        assert!(build_capture_component("empty", &plan, 0.0).is_none());
    }

    // -------------------------------------------------------------------------
    // load_plans_from_dir_with_errors
    // -------------------------------------------------------------------------

    #[test]
    fn load_plans_loads_example_plan_successfully() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/capture_plans");
        let (plans, errors) = load_plans_from_dir_with_errors(&dir);
        assert!(
            !plans.is_empty(),
            "Expected at least one example plan to load"
        );
        // The example plan should not produce errors
        assert!(
            errors.get("example_plan").is_none(),
            "example_plan should not have load errors"
        );
    }

    #[test]
    fn load_plans_from_nonexistent_dir_returns_empty() {
        let dir = std::path::PathBuf::from("/nonexistent/path/that/does/not/exist");
        let (plans, errors) = load_plans_from_dir_with_errors(&dir);
        assert!(plans.is_empty());
        assert!(errors.is_empty());
    }

    // -------------------------------------------------------------------------
    // parameter_value helper
    // -------------------------------------------------------------------------

    #[test]
    fn parameter_value_extracts_f64() {
        let params = Some(json!({ "max_velocity": 2.5 }));
        assert_eq!(parameter_value(&params, "max_velocity"), Some(2.5));
    }

    #[test]
    fn parameter_value_returns_none_for_missing_key() {
        let params = Some(json!({ "max_force": 1.0 }));
        assert_eq!(parameter_value(&params, "max_velocity"), None);
    }

    #[test]
    fn parameter_value_returns_none_for_none_params() {
        assert_eq!(parameter_value(&None, "max_velocity"), None);
    }
}
