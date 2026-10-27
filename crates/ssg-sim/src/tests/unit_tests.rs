#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::capture::plan::{
        CapturePlan, CapturePlanLibrary, State, build_capture_component, compile_capture_plan,
        parameter_value, validate_capture_plan,
    };
    use crate::project::load_plans_from_dir_with_errors;
    use crate::test_support::repo_root;

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
        use crate::capture::plan::CapturePlanDevice;
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
        use crate::capture::plan::CapturePlanDevice;
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
        let dir = repo_root().join("assets/example_capture_plans");
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
    // plan.id — identity across loader, insert_plan, and serialization
    // -------------------------------------------------------------------------

    #[test]
    fn loader_sets_plan_id_to_file_stem() {
        // All plans loaded from disk must have their id populated with the file
        // stem so that any code using plan.id gets the correct lookup key.
        let dir = repo_root().join("assets/example_capture_plans");
        let (plans, _) = load_plans_from_dir_with_errors(&dir);
        for (stem, plan) in &plans {
            assert_eq!(
                &plan.id, stem,
                "plan.id should equal the file stem for '{}'",
                stem
            );
        }
    }

    #[test]
    fn loader_id_is_independent_of_display_name() {
        // The file stem (id) is independent of the JSON "name" field. A plan
        // keyed under "example_plan" must have id == "example_plan" regardless
        // of what its "name" field contains.
        let dir = repo_root().join("assets/example_capture_plans");
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
        assert!(
            value.get("id").is_none(),
            "\"id\" must not appear in serialized JSON"
        );
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

    // -------------------------------------------------------------------------
    // CaptureLog ring-buffer (A1)
    // -------------------------------------------------------------------------

    use crate::telemetry::{CaptureLog, LogEntry, LogLevel};

    fn make_entry(msg: &str) -> LogEntry {
        LogEntry {
            timestamp: "00:00:00".to_string(),
            level: LogLevel::Info,
            source: "test",
            message: msg.to_string(),
        }
    }

    #[test]
    fn capture_log_push_within_capacity() {
        let mut log = CaptureLog::default();
        for i in 0..10 {
            log.push(make_entry(&format!("entry {i}")));
        }
        assert_eq!(log.entries.len(), 10);
        assert_eq!(log.entries[0].message, "entry 0");
        assert_eq!(log.entries[9].message, "entry 9");
    }

    #[test]
    fn capture_log_push_evicts_oldest_at_capacity() {
        let mut log = CaptureLog {
            entries: std::collections::VecDeque::new(),
            max_entries: 3,
        };
        for i in 0..4 {
            log.push(make_entry(&format!("entry {i}")));
        }
        assert_eq!(log.entries.len(), 3);
        // "entry 0" must have been evicted
        assert_eq!(log.entries[0].message, "entry 1");
        assert_eq!(log.entries[2].message, "entry 3");
    }

    #[test]
    fn capture_log_push_single_slot_always_keeps_newest() {
        let mut log = CaptureLog {
            entries: std::collections::VecDeque::new(),
            max_entries: 1,
        };
        for i in 0..5 {
            log.push(make_entry(&format!("entry {i}")));
        }
        assert_eq!(log.entries.len(), 1);
        assert_eq!(log.entries[0].message, "entry 4");
    }

    #[test]
    fn capture_log_clear_empties_entries() {
        let mut log = CaptureLog::default();
        for i in 0..5 {
            log.push(make_entry(&format!("entry {i}")));
        }
        log.clear();
        assert!(log.entries.is_empty());
    }

    // -------------------------------------------------------------------------
    // LogLevel helpers (A2)
    // -------------------------------------------------------------------------

    #[test]
    fn log_level_labels() {
        assert_eq!(LogLevel::Error.label(), "ERR");
        assert_eq!(LogLevel::Warn.label(), "WARN");
        assert_eq!(LogLevel::Info.label(), "INFO");
        assert_eq!(LogLevel::Debug.label(), "DBG");
    }

    #[test]
    fn log_level_hash_eq_all_four_variants_are_distinct() {
        let mut set = std::collections::HashSet::new();
        for _ in 0..3 {
            set.insert(LogLevel::Error);
            set.insert(LogLevel::Warn);
            set.insert(LogLevel::Info);
            set.insert(LogLevel::Debug);
        }
        assert_eq!(set.len(), 4);
    }
}
