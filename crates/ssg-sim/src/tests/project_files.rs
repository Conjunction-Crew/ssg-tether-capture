//! Tests for the plan file I/O and the config-file helpers in `project.rs` (P5 S6).
//!
//! Every test works in a [`TempDir`] under `std::env::temp_dir()`; none reads or
//! writes the user's config, home or working directory (P5 rule 19). The functions
//! that do (`load_from_config`, `save_to_config`, `set_working_directory`) are
//! inventory exceptions (Q46); their file logic is tested through the path-taking
//! helpers they call.

use std::path::Path;

use crate::capture::plan::CapturePlanLibrary;
use crate::project::{
    load_plans_from_dir, load_plans_from_dir_with_errors, read_config_file, save_plan_file,
    write_config_file,
};
use crate::test_support::TempDir;

fn plan_json(name: &str) -> String {
    format!(
        r#"{{"name": "{name}", "tether": "T", "states": [
  {{"id": "first", "parameters": {{"max_velocity": 1.0, "max_force": 1.0}}}}
]}}"#
    )
}

fn sorted_keys<V>(map: &bevy::platform::collections::HashMap<String, V>) -> Vec<String> {
    let mut keys: Vec<String> = map.keys().cloned().collect();
    keys.sort();
    keys
}

// ---------------------------------------------------------------------------
// load_plans_from_dir / load_plans_from_dir_with_errors
// ---------------------------------------------------------------------------

/// One directory holding every kind of file the loader has to tell apart.
fn mixed_plan_dir(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    dir.write("good.json", &plan_json("Good plan"));
    dir.write("broken.json", "{ not json");
    dir.write(
        "empty_name.json",
        r#"{"name": "", "tether": "T", "states": [
  {"id": "first", "parameters": {"max_velocity": 1.0, "max_force": 1.0}}
]}"#,
    );
    dir.write("notes.txt", "not a plan");
    // Bytes that are not UTF-8: `read_to_string` fails, the file is reported.
    std::fs::write(dir.path().join("latin1.json"), [0x7b, 0xff, 0xfe, 0x7d])
        .expect("could not write the non-UTF-8 file");
    dir
}

#[test]
fn load_plans_from_dir_with_errors_separates_plans_from_errors() {
    let dir = mixed_plan_dir("plans-mixed");

    let (plans, errors) = load_plans_from_dir_with_errors(dir.path());

    assert_eq!(sorted_keys(&plans), ["good"], "only the valid file loads");
    assert_eq!(plans["good"].name, "Good plan");
    assert_eq!(plans["good"].id, "good", "the id is the file stem");
    assert_eq!(
        sorted_keys(&errors),
        ["broken", "empty_name", "latin1"],
        "every rejected .json file is reported, the .txt file is ignored"
    );
    assert!(
        errors["broken"].len() == 1 && errors["broken"][0].starts_with("Invalid JSON: "),
        "a parse error says 'Invalid JSON: ...': {:?}",
        errors["broken"]
    );
    // F-32 (P6): validation messages are `[<plan-id>] <json-pointer>: <message>`.
    assert_eq!(
        errors["empty_name"],
        ["[empty_name] /name: must not be blank"],
        "a validation error is the validator's message"
    );
    assert!(
        errors["latin1"].len() == 1 && errors["latin1"][0].starts_with("Could not read file: "),
        "an unreadable file says 'Could not read file: ...': {:?}",
        errors["latin1"]
    );
}

#[test]
fn load_plans_from_dir_returns_only_the_valid_plans() {
    let dir = mixed_plan_dir("plans-valid-only");

    let plans = load_plans_from_dir(dir.path());

    assert_eq!(sorted_keys(&plans), ["good"]);
    assert_eq!(plans["good"].tether, "T");
    assert_eq!(plans["good"].phases.len(), 1);
}

#[test]
fn load_plans_from_dir_with_errors_on_a_missing_directory_is_empty() {
    let dir = TempDir::new("plans-missing");

    let (plans, errors) = load_plans_from_dir_with_errors(&dir.path().join("does_not_exist"));

    assert!(plans.is_empty());
    assert!(errors.is_empty());
}

#[test]
fn load_plans_from_dir_with_errors_on_a_file_instead_of_a_directory_is_empty() {
    let dir = TempDir::new("plans-not-a-dir");
    let file = dir.write("a_file.json", &plan_json("Not a directory"));

    // The path exists but `read_dir` fails on it: nothing is loaded, nothing is
    // reported.
    let (plans, errors) = load_plans_from_dir_with_errors(&file);

    assert!(plans.is_empty());
    assert!(errors.is_empty());
}

// ---------------------------------------------------------------------------
// save_plan_file
// ---------------------------------------------------------------------------

#[test]
fn save_plan_file_writes_pretty_json_that_loads_back() {
    let dir = TempDir::new("save-plan");
    let json: serde_json::Value = serde_json::from_str(&plan_json("Saved plan")).unwrap();

    save_plan_file(dir.path(), "saved.json", &json).expect("save failed");

    let text = std::fs::read_to_string(dir.path().join("saved.json")).unwrap();
    assert_eq!(
        text,
        serde_json::to_string_pretty(&json).unwrap(),
        "the file holds exactly the pretty-printed value"
    );
    assert!(
        text.contains("\n  \"name\": \"Saved plan\""),
        "indented:\n{text}"
    );
    let plans = load_plans_from_dir(dir.path());
    assert_eq!(plans["saved"].name, "Saved plan");
}

#[test]
fn save_plan_file_replaces_an_existing_file() {
    let dir = TempDir::new("save-plan-replace");
    save_plan_file(
        dir.path(),
        "p.json",
        &serde_json::from_str(&plan_json("First")).unwrap(),
    )
    .unwrap();

    save_plan_file(
        dir.path(),
        "p.json",
        &serde_json::from_str(&plan_json("Second")).unwrap(),
    )
    .unwrap();

    assert_eq!(load_plans_from_dir(dir.path())["p"].name, "Second");
}

#[test]
fn save_plan_file_into_a_missing_directory_is_a_not_found_error() {
    let dir = TempDir::new("save-plan-missing");
    let missing = dir.path().join("no_such_dir");

    let result = save_plan_file(&missing, "p.json", &serde_json::json!({"name": "x"}));

    let error = result.expect_err("saving into a missing directory must fail");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(!missing.exists(), "the directory is not created");
}

// ---------------------------------------------------------------------------
// CapturePlanLibrary::load_all / reload_user_plans
// ---------------------------------------------------------------------------

#[test]
fn load_all_unions_example_and_user_plans_and_compiles_them() {
    let examples = TempDir::new("load-all-examples");
    let users = TempDir::new("load-all-users");
    examples.write("example_a.json", &plan_json("Example A"));
    examples.write("example_bad.json", "[");
    users.write("user_a.json", &plan_json("User A"));
    users.write(
        "user_empty.json",
        r#"{"name": "x", "tether": "", "states": []}"#,
    );
    let mut library = CapturePlanLibrary::default();

    let errors = library.load_all(examples.path(), users.path());

    assert_eq!(sorted_keys(&library.example_plans), ["example_a"]);
    assert_eq!(sorted_keys(&library.user_plans), ["user_a"]);
    assert_eq!(sorted_keys(&library.plans), ["example_a", "user_a"]);
    assert_eq!(
        sorted_keys(&library.compiled_plans),
        ["example_a", "user_a"],
        "every plan of the union is compiled"
    );
    assert_eq!(
        sorted_keys(&errors),
        ["example_bad", "user_empty"],
        "the load errors of both directories are merged"
    );
    assert!(errors["example_bad"][0].starts_with("Invalid JSON: "));
    // F-32 (P6): typed `[<plan-id>] <json-pointer>: <message>` messages; a v0
    // file's `states` are checked as the migrated plan's `phases`.
    assert_eq!(
        errors["user_empty"],
        [
            "[user_empty] /tether: must not be blank",
            "[user_empty] /phases: at least one phase is required"
        ]
    );
}

#[test]
fn load_all_lets_a_user_plan_replace_an_example_of_the_same_name() {
    let examples = TempDir::new("load-all-shadow-ex");
    let users = TempDir::new("load-all-shadow-us");
    examples.write("shared.json", &plan_json("From the examples"));
    users.write("shared.json", &plan_json("From the user"));
    let mut library = CapturePlanLibrary::default();

    let errors = library.load_all(examples.path(), users.path());

    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(library.example_plans["shared"].name, "From the examples");
    assert_eq!(library.user_plans["shared"].name, "From the user");
    assert_eq!(
        library.plans["shared"].name, "From the user",
        "the user's plan wins in the union"
    );
    assert_eq!(library.plans["shared"].id, "shared");
}

#[test]
fn load_all_replaces_what_the_library_held_before() {
    let examples = TempDir::new("load-all-replace-ex");
    let users = TempDir::new("load-all-replace-us");
    examples.write("only.json", &plan_json("Only"));
    let mut library = CapturePlanLibrary::default();
    library.load_all(examples.path(), users.path());
    std::fs::remove_file(examples.path().join("only.json")).unwrap();
    examples.write("other.json", &plan_json("Other"));

    library.load_all(examples.path(), users.path());

    assert_eq!(sorted_keys(&library.example_plans), ["other"]);
    assert_eq!(sorted_keys(&library.plans), ["other"]);
}

#[test]
fn reload_user_plans_picks_up_a_saved_plan_and_keeps_the_examples() {
    let examples = TempDir::new("reload-examples");
    let users = TempDir::new("reload-users");
    examples.write("example_a.json", &plan_json("Example A"));
    let mut library = CapturePlanLibrary::default();
    library.load_all(examples.path(), users.path());
    assert!(library.user_plans.is_empty());
    save_plan_file(
        users.path(),
        "mine.json",
        &serde_json::from_str(&plan_json("Mine")).unwrap(),
    )
    .unwrap();

    library.reload_user_plans(users.path());

    assert_eq!(sorted_keys(&library.user_plans), ["mine"]);
    assert_eq!(sorted_keys(&library.example_plans), ["example_a"]);
    assert_eq!(sorted_keys(&library.plans), ["example_a", "mine"]);
    assert_eq!(
        sorted_keys(&library.compiled_plans),
        ["example_a", "mine"],
        "the new plan is compiled"
    );
    assert_eq!(library.plans["mine"].id, "mine");
}

#[test]
fn reload_user_plans_recompiles_an_edited_plan() {
    let examples = TempDir::new("reload-edit-ex");
    let users = TempDir::new("reload-edit-us");
    users.write("mine.json", &plan_json("Before"));
    let mut library = CapturePlanLibrary::default();
    library.load_all(examples.path(), users.path());
    assert_eq!(library.plans["mine"].name, "Before");
    users.write(
        "mine.json",
        r#"{"name": "After", "tether": "T", "states": [
  {"id": "a", "parameters": {"max_velocity": 1.0, "max_force": 1.0}},
  {"id": "b", "parameters": {"max_velocity": 1.0, "max_force": 1.0}}
]}"#,
    );

    library.reload_user_plans(users.path());

    assert_eq!(library.plans["mine"].name, "After");
    assert!(
        library.compiled_plans["mine"].state("b").is_some(),
        "the compiled plan has the state added by the edit"
    );
}

#[test]
fn reload_user_plans_drops_a_deleted_plan_but_keeps_its_compiled_entry() {
    let examples = TempDir::new("reload-delete-ex");
    let users = TempDir::new("reload-delete-us");
    users.write("gone.json", &plan_json("Gone"));
    let mut library = CapturePlanLibrary::default();
    library.load_all(examples.path(), users.path());
    std::fs::remove_file(users.path().join("gone.json")).unwrap();

    library.reload_user_plans(users.path());

    assert!(library.user_plans.is_empty());
    assert!(library.plans.is_empty(), "the union no longer has the plan");
    // Proposed finding (P5), low: `reload_user_plans` rebuilds `plans` but never
    // removes entries from `compiled_plans`, so a deleted (or no longer valid) user
    // plan stays compiled until the next `load_all` replaces the library's maps.
    // `compiled_plans` is not cleared by `load_all` either. Pinned as it is today.
    assert_eq!(sorted_keys(&library.compiled_plans), ["gone"]);
}

// ---------------------------------------------------------------------------
// read_config_file / write_config_file (the seam under load_from_config and
// save_to_config, OQ3 a)
// ---------------------------------------------------------------------------

#[test]
fn read_config_file_returns_the_working_directory_entry() {
    let dir = TempDir::new("config-read");
    let file = dir.write("config.json", r#"{"working_directory": "/some/where"}"#);

    assert_eq!(read_config_file(&file).as_deref(), Some("/some/where"));
}

#[test]
fn read_config_file_ignores_other_entries() {
    let dir = TempDir::new("config-read-extra");
    let file = dir.write(
        "config.json",
        r#"{"theme": "dark", "working_directory": "/a", "zoom": 2}"#,
    );

    assert_eq!(read_config_file(&file).as_deref(), Some("/a"));
}

#[test]
fn read_config_file_is_none_for_a_missing_file() {
    let dir = TempDir::new("config-read-missing");

    assert_eq!(read_config_file(&dir.path().join("config.json")), None);
}

#[test]
fn read_config_file_is_none_for_text_that_is_not_json() {
    let dir = TempDir::new("config-read-garbage");
    let file = dir.write("config.json", "working_directory = /a");

    assert_eq!(read_config_file(&file), None);
}

#[test]
fn read_config_file_is_none_without_a_string_entry() {
    let dir = TempDir::new("config-read-entry");
    for (name, contents) in [
        ("no_entry.json", r#"{"theme": "dark"}"#),
        ("number.json", r#"{"working_directory": 5}"#),
        ("null.json", r#"{"working_directory": null}"#),
        ("array.json", r#"["working_directory"]"#),
    ] {
        let file = dir.write(name, contents);
        assert_eq!(read_config_file(&file), None, "{name}: {contents}");
    }
}

#[test]
fn write_config_file_creates_the_directory_and_reads_back() {
    let dir = TempDir::new("config-write");
    let file = dir.path().join("nested").join("deeper").join("config.json");

    write_config_file(&file, "/projects/capture");

    assert!(file.is_file(), "the file and its parents are created");
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(
        value,
        serde_json::json!({ "working_directory": "/projects/capture" })
    );
    assert_eq!(
        read_config_file(&file).as_deref(),
        Some("/projects/capture")
    );
}

#[test]
fn write_config_file_replaces_the_previous_content() {
    let dir = TempDir::new("config-write-replace");
    let file = dir.write(
        "config.json",
        r#"{"working_directory": "/old", "theme": "dark"}"#,
    );

    write_config_file(&file, "/new");

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(
        value,
        serde_json::json!({ "working_directory": "/new" }),
        "the whole file is rewritten, other entries are not kept"
    );
}

#[test]
fn write_config_file_round_trips_awkward_paths() {
    let dir = TempDir::new("config-write-awkward");
    let file = dir.path().join("config.json");
    for path in [
        r"C:\Users\me\ssg tether capture",
        r#"/with "quotes" and \ backslash"#,
        "/ünïcödé/目录",
        "",
    ] {
        write_config_file(&file, path);
        assert_eq!(read_config_file(&file).as_deref(), Some(path), "{path:?}");
    }
}

#[test]
fn write_config_file_under_a_regular_file_writes_nothing() {
    let dir = TempDir::new("config-write-blocked");
    let blocker = dir.write("blocker", "I am a file, not a directory");
    let file = blocker.join("config.json");

    // The parent cannot be created: the failure is logged (a warning), not
    // returned, and nothing appears.
    write_config_file(&file, "/never");

    assert!(!Path::new(&file).exists());
    assert_eq!(
        std::fs::read_to_string(&blocker).unwrap(),
        "I am a file, not a directory",
        "the blocking file is untouched"
    );
}
