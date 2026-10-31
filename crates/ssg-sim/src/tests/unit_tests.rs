#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::capture::plan::{
        CapturePlan, CapturePlanDevice, CapturePlanLibrary, ConditionBounds, Phase, Transition,
        TransitionConditions, build_capture_component, compile_capture_plan, validate_capture_plan,
    };
    use crate::project::load_plans_from_dir_with_errors;
    use crate::project::plan_files::read_plan_value;
    use crate::test_support::repo_root;

    // -------------------------------------------------------------------------
    // validate_capture_plan
    // -------------------------------------------------------------------------

    // F-15 (P6): the plans below are the typed v1 model (phases, unit-suffixed
    // fields) instead of v0 `State`s with `Option<Value>` parameters.
    fn phase(id: &str, max_velocity_mps: f64, max_force_n: f64) -> Phase {
        Phase {
            id: id.to_string(),
            max_velocity_mps,
            max_force_n,
            shrink_rate_mps: None,
            transitions: None,
            end: None,
            tail_actuation: None,
        }
    }

    fn plan_with_phases(name: &str, phases: Vec<Phase>) -> CapturePlan {
        CapturePlan {
            schema_version: 1,
            name: name.to_string(),
            id: String::new(),
            tether: "Tether1".to_string(),
            device: None,
            capture_sphere: None,
            phases,
            migrated_from_v0: None,
        }
    }

    fn distance_below(to: &str, less_than: f64) -> Transition {
        Transition {
            to: to.to_string(),
            all_of: TransitionConditions {
                distance_m: Some(ConditionBounds {
                    less_than: Some(less_than),
                    greater_than: None,
                }),
                relative_speed_mps: None,
            },
        }
    }

    fn minimal_valid_plan() -> CapturePlan {
        plan_with_phases("Test Plan", vec![phase("approach", 1.0, 2.0)])
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
        assert!(errors.iter().any(|e| e.contains("/name")));
    }

    #[test]
    fn validate_plan_errors_on_empty_tether() {
        let mut plan = minimal_valid_plan();
        plan.tether = String::new();
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("/tether")));
    }

    #[test]
    fn validate_plan_errors_on_empty_states() {
        let mut plan = minimal_valid_plan();
        plan.phases.clear();
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("/phases")));
    }

    // F-15 (P6): a v1 phase cannot lack its limits, so a missing one is refused when
    // the file is read (the serde message names the field).
    #[test]
    fn validate_plan_errors_on_missing_max_velocity() {
        let doc = json!({"schema_version": 1, "name": "p", "tether": "T",
            "phases": [{"id": "approach", "max_force_n": 2.0}]});
        let errors = read_plan_value("test_plan", &doc).errors;
        assert!(errors.iter().any(|e| e.contains("max_velocity_mps")));
    }

    #[test]
    fn validate_plan_errors_on_missing_max_force() {
        let doc = json!({"schema_version": 1, "name": "p", "tether": "T",
            "phases": [{"id": "approach", "max_velocity_mps": 1.0}]});
        let errors = read_plan_value("test_plan", &doc).errors;
        assert!(errors.iter().any(|e| e.contains("max_force_n")));
    }

    #[test]
    fn validate_plan_errors_on_transition_to_unknown_state() {
        let mut plan = minimal_valid_plan();
        plan.phases[0].transitions = Some(vec![distance_below("nonexistent", 10.0)]);
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("nonexistent")));
    }

    // F-15 (P6): `to` is required by the v1 type; a v0 file without it is refused by
    // the migration.
    #[test]
    fn validate_plan_errors_on_transition_missing_to_field() {
        let doc = json!({"name": "p", "tether": "T", "states": [{"id": "approach",
            "parameters": {"max_velocity": 1.0, "max_force": 2.0},
            "transitions": [{"distance": {"less_than": 10.0}}]}]});
        let errors = read_plan_value("test_plan", &doc).errors;
        assert!(errors.iter().any(|e| e.contains("/transitions/0/to")));
    }

    #[test]
    fn validate_plan_accepts_valid_internal_transition() {
        let mut approach = phase("approach", 1.0, 2.0);
        approach.transitions = Some(vec![distance_below("capture", 5.0)]);
        let plan = plan_with_phases("Two State Plan", vec![approach, phase("capture", 0.5, 2.0)]);
        assert!(validate_capture_plan("two_state_plan", &plan).is_empty());
    }

    #[test]
    fn validate_plan_errors_on_duplicate_state_ids() {
        let plan = plan_with_phases(
            "Bad Plan",
            vec![phase("approach", 1.0, 2.0), phase("approach", 0.5, 2.0)],
        );
        let errors = validate_capture_plan("bad_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("duplicate")));
    }

    #[test]
    fn validate_plan_errors_on_zero_tether_length_in_device() {
        let mut plan = minimal_valid_plan();
        plan.device = Some(CapturePlanDevice {
            device_type: Some("tether".to_string()),
            tether_length_m: Some(0.0),
            ..CapturePlanDevice::default()
        });
        let errors = validate_capture_plan("test_plan", &plan);
        assert!(errors.iter().any(|e| e.contains("tether_length")));
    }

    #[test]
    fn validate_plan_passes_with_valid_device_block() {
        let mut plan = minimal_valid_plan();
        plan.device = Some(CapturePlanDevice {
            device_type: Some("tether".to_string()),
            tether_length_m: Some(20.0),
            ..CapturePlanDevice::default()
        });
        assert!(validate_capture_plan("test_plan", &plan).is_empty());
    }

    // -------------------------------------------------------------------------
    // compile_capture_plan
    // -------------------------------------------------------------------------

    #[test]
    fn compile_plan_indexes_states_by_id() {
        let mut capture = phase("capture", 0.5, 2.0);
        capture.shrink_rate_mps = Some(0.01);
        let plan = plan_with_phases("Compiled Plan", vec![phase("approach", 1.0, 2.0), capture]);
        let compiled = compile_capture_plan(&plan);
        assert!(compiled.state("approach").is_some());
        assert!(compiled.state("capture").is_some());
        assert!(compiled.state("nonexistent").is_none());
    }

    #[test]
    fn compile_plan_parses_parameters_correctly() {
        let mut terminal = phase("terminal", 0.3, 1.5);
        terminal.shrink_rate_mps = Some(0.005);
        let plan = plan_with_phases("Param Plan", vec![terminal]);
        let compiled = compile_capture_plan(&plan);
        let state = compiled.state("terminal").unwrap();
        assert!((state.parameters.max_velocity - 0.3).abs() < f64::EPSILON);
        assert!((state.parameters.max_force - 1.5).abs() < f64::EPSILON);
        assert!((state.parameters.shrink_rate.unwrap() - 0.005).abs() < f64::EPSILON);
    }

    #[test]
    fn compile_plan_parses_distance_transition_conditions() {
        let mut approach = phase("approach", 1.0, 2.0);
        approach.transitions = Some(vec![distance_below("approach", 50.0)]);
        let plan = plan_with_phases("Transition Plan", vec![approach]);
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
        let plan = plan_with_phases(
            "Plan",
            vec![phase("approach", 0.0, 0.0), phase("capture", 0.0, 0.0)],
        );
        let component = build_capture_component("plan", &plan, 123.0).unwrap();
        assert_eq!(component.current_state, "approach");
        assert!((component.state_enter_time_s - 123.0).abs() < f64::EPSILON);
    }

    #[test]
    fn build_capture_component_returns_none_for_empty_states() {
        let plan = plan_with_phases("Empty", vec![]);
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
            "schema_version": 1,
            "name": "Test Plan",
            "tether": "Tether1",
            "phases": [{
                "id": "approach",
                "max_velocity_mps": 1.0,
                "max_force_n": 2.0
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
            epoch_utc: "2026-04-03T00:00:00.000000Z".to_string(),
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
