#[cfg(test)]
mod tests {
    use crate::ui::log_ui::CaptureLogUiState;
    use crate::ui::screens::capture_plan::form::generate_filename;
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
