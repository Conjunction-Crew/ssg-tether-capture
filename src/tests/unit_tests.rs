#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::components::capture_components::{CapturePlan, State};
    use crate::resources::capture_plans::{
        build_capture_component, compile_capture_plan, load_plans_from_dir_with_errors,
        parameter_value, validate_capture_plan, CapturePlanLibrary,
    };
    use crate::ui::screens::capture_plan::{build_capture_plan_json, generate_filename, validate_form};
    use crate::resources::capture_plan_form::{NewCapturePlanForm, TransitionForm, UnitSystem};

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
            id: String::new(),
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
            id: String::new(),
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
            id: String::new(),
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

    #[test]
    fn validate_plan_errors_on_zero_tether_length_in_device() {
        use crate::components::capture_components::CapturePlanDevice;
        let mut plan = minimal_valid_plan();
        plan.device = Some(CapturePlanDevice {
            device_type: "tether".to_string(),
            tether_length: 0.0,
        });
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("tether_length")));
    }

    #[test]
    fn validate_plan_passes_with_valid_device_block() {
        use crate::components::capture_components::CapturePlanDevice;
        let mut plan = minimal_valid_plan();
        plan.device = Some(CapturePlanDevice {
            device_type: "tether".to_string(),
            tether_length: 20.0,
        });
        assert!(validate_capture_plan("test_plan", &plan).is_empty());
    }

    // -------------------------------------------------------------------------
    // compile_capture_plan
    // -------------------------------------------------------------------------

    #[test]
    fn compile_plan_indexes_states_by_id() {
        let plan = CapturePlan {
            name: "Compiled Plan".to_string(),
            id: String::new(),
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
            id: String::new(),
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
            id: String::new(),
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
            id: String::new(),
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
            id: String::new(),
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
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/example_capture_plans");
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
        assert!(errors.iter().any(|e| e.contains("Approach Transition 1") && e.contains("To State")));
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
        assert!(errors.iter().any(|e| e.contains("Approach Transition 1") && e.contains("condition")));
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
        assert!(errors.iter().any(|e| e.contains("Approach Transition 1") && e.contains("number")));
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
        assert_eq!(json["states"][0]["transitions"].as_array().unwrap().len(), 0);
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

    // -------------------------------------------------------------------------
    // plan.id — identity across loader, insert_plan, and serialization
    // -------------------------------------------------------------------------

    #[test]
    fn loader_sets_plan_id_to_file_stem() {
        // All plans loaded from disk must have their id populated with the file
        // stem so that any code using plan.id gets the correct lookup key.
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/example_capture_plans");
        let (plans, _) = load_plans_from_dir_with_errors(&dir);
        for (stem, plan) in &plans {
            assert_eq!(
                &plan.id, stem,
                "plan.id should equal the file stem for '{}'", stem
            );
        }
    }

    #[test]
    fn loader_id_is_independent_of_display_name() {
        // The file stem (id) is independent of the JSON "name" field. A plan
        // keyed under "example_plan" must have id == "example_plan" regardless
        // of what its "name" field contains.
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/example_capture_plans");
        let (plans, _) = load_plans_from_dir_with_errors(&dir);
        if let Some(plan) = plans.get("example_plan") {
            assert_eq!(plan.id, "example_plan");
        }
    }

    #[test]
    fn insert_plan_stamps_id_on_stored_plan() {
        // insert_plan must always set plan.id = plan_id on the stored copy so
        // callers can recover the lookup key from the plan struct itself.
        let mut lib = CapturePlanLibrary::default();
        let plan = minimal_valid_plan(); // plan.id starts as ""
        lib.insert_plan("my_plan".to_string(), plan);
        let stored = lib.plans.get("my_plan").unwrap();
        assert_eq!(stored.id, "my_plan");
    }

    #[test]
    fn insert_plan_id_is_file_stem_not_display_name() {
        // plan.id must equal the HashMap key (file stem), not plan.name.
        let mut lib = CapturePlanLibrary::default();
        let mut plan = minimal_valid_plan();
        plan.name = "My Fancy Plan".to_string(); // display name different from stem
        lib.insert_plan("my_fancy_plan".to_string(), plan);
        let stored = lib.plans.get("my_fancy_plan").unwrap();
        assert_eq!(stored.id, "my_fancy_plan");
        assert_ne!(stored.id, stored.name);
    }

    #[test]
    fn plan_id_field_is_not_serialized_to_json() {
        // plan.id is #[serde(skip)] so it must not appear in the output JSON.
        // If it did, saved plan files would gain an unexpected "id" key.
        let mut plan = minimal_valid_plan();
        plan.id = "test_plan".to_string();
        let json_str = serde_json::to_string(&plan).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(value.get("id").is_none(), "\"id\" must not appear in serialized JSON");
    }

    #[test]
    fn plan_id_is_empty_after_json_deserialization() {
        // When a plan is deserialized from JSON (no "id" key present), plan.id
        // must default to "" — the loader sets it to the file stem afterwards.
        let json = json!({
            "name": "Test Plan",
            "tether": "Tether1",
            "states": [{
                "id": "approach",
                "parameters": { "max_velocity": 1.0, "max_force": 2.0 }
            }]
        });
        let plan: CapturePlan = serde_json::from_value(json).unwrap();
        assert_eq!(plan.id, "", "id should be empty before the loader sets it");
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
        let v = json["states"][0]["parameters"]["max_velocity"].as_f64().unwrap();
        assert!((v - 2.0).abs() < 1e-9);
    }

    #[test]
    fn build_plan_json_imperial_velocity_converts_to_metric() {
        let mut form = valid_form();
        form.unit_system = UnitSystem::Imperial;
        form.approach_max_velocity = "1.0".to_string(); // 1 ft/s
        let json = build_capture_plan_json(&form);
        let v = json["states"][0]["parameters"]["max_velocity"].as_f64().unwrap();
        // 1 ft/s = 0.3048 m/s
        assert!((v - 0.3048).abs() < 1e-9);
    }

    #[test]
    fn build_plan_json_imperial_force_converts_to_metric() {
        let mut form = valid_form();
        form.unit_system = UnitSystem::Imperial;
        form.approach_max_force = "1.0".to_string(); // 1 lbf
        let json = build_capture_plan_json(&form);
        let f = json["states"][0]["parameters"]["max_force"].as_f64().unwrap();
        // 1 lbf = 4.44822 N
        assert!((f - 4.44822).abs() < 1e-4);
    }
}
