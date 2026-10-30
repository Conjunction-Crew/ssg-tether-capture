//! `project.json` (design D2, D6, D10; P6 S3): the reader and validator with one
//! test per rule, the compatibility rule, the conversion to and from sim inputs, the
//! write-and-read round trips on generated manifests (17-digit elements bit for
//! bit, GP records) and the published JSON Schema file.

use serde_json::{Value, json};

use crate::orbit::catalog::{JsonOrbital, gp_record_to_elements};
use crate::orbit::selection::{
    EditableOrbitalElements, OrbitalSelectionSource, OrbitalSelectionState, SelectedOrbitalObject,
};
use crate::project::epoch::{format_utc, parse_utc};
use crate::project::manifest::{
    BuiltWith, MANIFEST_FILE, ManifestBody, ManifestElements, ManifestOrbit, ManifestSource,
    ProjectManifest, RelativePreset, RelativeToTarget, manifest_to_json, read_manifest_file,
    read_manifest_value, read_sim_inputs, save_and_reload, save_manifest,
};
use crate::test_support::{Lcg, TempDir, fixed_epoch, q13};
use crate::tests::plan_schema_v1::check_schema;

const START: &str = "2026-04-03T16:03:47.002464Z";

/// A valid manifest document (the design's example, trimmed) to break.
fn doc() -> Value {
    json!({
        "schema_version": 1,
        "name": "Hubble capture study",
        "start_epoch_utc": START,
        "active_plan": "example_plan",
        "target": {
            "label": "HST",
            "orbit": { "gp": serde_json::to_value(q13::fixture_record(q13::HST_NORAD_ID)).unwrap() },
            "source": { "dataset": "datasets/dataset_2026_04_03.json", "norad_id": 20580 },
            "mass_kg": 2500.0
        },
        "chaser": {
            "label": "P0 HST CHASER",
            "orbit": { "elements": {
                "semi_major_axis_m": 6914000.0, "eccentricity": 0.0002, "inclination_rad": 0.4969,
                "raan_rad": 1.747, "arg_perigee_rad": 1.4, "mean_anomaly_rad": 4.88,
                "epoch_utc": START
            } }
        },
        "built_with": { "app_version": "0.4.0-rc.1", "target": "aarch64-macos", "avian_features": ["simd", "parallel"] }
    })
}

fn read(value: &Value) -> crate::project::manifest::ManifestReport {
    read_manifest_value(value)
}

#[track_caller]
fn assert_error(value: &Value, expected: &[&str]) {
    let report = read(value);
    assert_eq!(report.errors, expected, "errors for {value}");
    assert!(
        report.manifest.is_none(),
        "a manifest with errors does not load"
    );
}

// ---------------------------------------------------------------------------
// the reader
// ---------------------------------------------------------------------------

#[test]
fn the_design_example_loads_and_turns_into_sim_inputs() {
    let report = read(&doc());
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let manifest = report.manifest.expect("the example is valid");

    let inputs = manifest.sim_inputs().expect("valid manifests give inputs");

    assert_eq!(inputs.start_epoch, fixed_epoch());
    assert_eq!(inputs.active_plan.as_deref(), Some("example_plan"));
    let target_gp = inputs
        .target
        .gp
        .as_ref()
        .expect("a GP body starts from SGP4");
    assert_eq!(target_gp.norad_id, 20580);
    assert!(
        matches!(&inputs.target.source, OrbitalSelectionSource::Custom { label } if label == "HST")
    );
    assert!(
        inputs.chaser.gp.is_none(),
        "an elements body has no GP record"
    );
    assert_eq!(inputs.chaser.elements.semi_major_axis_m, 6_914_000.0);
    assert_eq!(inputs.chaser.elements.epoch, fixed_epoch());
}

#[test]
fn a_minimal_manifest_needs_no_optional_key() {
    let mut value = doc();
    for key in ["active_plan", "built_with"] {
        value.as_object_mut().unwrap().remove(key);
    }
    for body in ["target", "chaser"] {
        for key in ["source", "mass_kg"] {
            value[body].as_object_mut().unwrap().remove(key);
        }
    }
    let report = read(&value);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let manifest = report.manifest.unwrap();
    assert_eq!(manifest.active_plan, None);
    assert_eq!(manifest.sim_inputs().unwrap().active_plan, None);
}

#[test]
fn a_newer_or_missing_or_unknown_schema_version_is_refused() {
    let mut value = doc();
    value["schema_version"] = json!(2);
    value["something_new"] = json!(true);
    assert_error(
        &value,
        &[
            "[project.json] /schema_version: 2 is not supported; this version reads schema_version 1 (the file was written by a newer version)",
        ],
    );

    value["schema_version"] = json!(0);
    assert_error(
        &value,
        &[
            "[project.json] /schema_version: 0 is not supported; this version reads schema_version 1",
        ],
    );

    value.as_object_mut().unwrap().remove("schema_version");
    assert_error(&value, &["[project.json] /schema_version: missing"]);
}

#[test]
fn an_unknown_key_is_refused_with_the_key_named_and_where() {
    for (pointer_path, key, at) in [
        (vec![], "colour", "/"),
        (vec!["target"], "colour", "/target"),
        (vec!["chaser", "source"], "colour", "/chaser/source"),
        (vec!["built_with"], "colour", "/built_with"),
        (
            vec!["chaser", "orbit", "elements"],
            "colour",
            "/chaser/orbit",
        ),
    ] {
        let mut value = doc();
        value["chaser"]["source"] = json!({ "dataset": "d.json", "norad_id": 1 });
        let mut node = &mut value;
        for step in &pointer_path {
            node = &mut node[*step];
        }
        node[key] = json!(1);

        let report = read(&value);

        assert_eq!(
            report.errors.len(),
            1,
            "{pointer_path:?}: {:?}",
            report.errors
        );
        let text = &report.errors[0];
        assert!(
            text.starts_with(&format!("[project.json] {at}: ")) && text.contains("colour"),
            "{pointer_path:?}: {text}"
        );
        assert!(report.manifest.is_none());
    }
}

#[test]
fn a_wrong_type_is_reported_where_it_is() {
    let mut value = doc();
    value["target"]["label"] = json!(5);
    let report = read(&value);
    assert_eq!(report.errors.len(), 1);
    assert!(
        report.errors[0].starts_with("[project.json] /target: "),
        "{}",
        report.errors[0]
    );
    assert!(
        report.errors[0].contains("invalid type"),
        "{}",
        report.errors[0]
    );
}

#[test]
fn an_orbit_must_have_exactly_one_known_key() {
    let mut value = doc();
    value["chaser"]["orbit"] = json!({ "kepler": {} });
    let report = read(&value);
    assert_eq!(report.errors.len(), 1);
    assert!(
        report.errors[0].starts_with("[project.json] /chaser/orbit: "),
        "{}",
        report.errors[0]
    );
    assert!(
        report.errors[0].contains("kepler"),
        "the unknown key is named"
    );
}

// ---------------------------------------------------------------------------
// the validator: one row per rule
// ---------------------------------------------------------------------------

#[test]
fn each_validation_rule_reports_its_message() {
    type Mutation = fn(&mut Value);
    let rows: [(&str, Mutation, &str); 17] = [
        (
            "blank name",
            |v| v["name"] = json!("  "),
            "[project.json] /name: must not be blank",
        ),
        (
            "bad start epoch",
            |v| v["start_epoch_utc"] = json!("2026-04-03"),
            "[project.json] /start_epoch_utc: `2026-04-03` is not a UTC epoch (YYYY-MM-DDTHH:MM:SS.ffffffZ)",
        ),
        (
            "blank active plan",
            |v| v["active_plan"] = json!(""),
            "[project.json] /active_plan: must not be blank",
        ),
        (
            "blank target label",
            |v| v["target"]["label"] = json!(""),
            "[project.json] /target/label: must not be blank",
        ),
        (
            "blank chaser label",
            |v| v["chaser"]["label"] = json!(" "),
            "[project.json] /chaser/label: must not be blank",
        ),
        (
            "GP without an element",
            |v| {
                v["target"]["orbit"]["gp"]
                    .as_object_mut()
                    .unwrap()
                    .remove("MEAN_MOTION");
            },
            "[project.json] /target/orbit/gp/MEAN_MOTION: missing, or not a number",
        ),
        (
            "GP element that is a string",
            |v| v["target"]["orbit"]["gp"]["ECCENTRICITY"] = json!("0.0002"),
            "[project.json] /target/orbit/gp/ECCENTRICITY: missing, or not a number",
        ),
        (
            "GP without a NORAD id",
            |v| {
                v["target"]["orbit"]["gp"]
                    .as_object_mut()
                    .unwrap()
                    .remove("NORAD_CAT_ID");
            },
            "[project.json] /target/orbit/gp/NORAD_CAT_ID: missing, or not a whole number",
        ),
        (
            "semi-major axis",
            |v| v["chaser"]["orbit"]["elements"]["semi_major_axis_m"] = json!(0.0),
            "[project.json] /chaser/orbit/elements/semi_major_axis_m: must be greater than 0",
        ),
        (
            "eccentricity",
            |v| v["chaser"]["orbit"]["elements"]["eccentricity"] = json!(1.0),
            "[project.json] /chaser/orbit/elements/eccentricity: must be at least 0 and below 1",
        ),
        (
            "elements epoch",
            |v| v["chaser"]["orbit"]["elements"]["epoch_utc"] = json!("yesterday"),
            "[project.json] /chaser/orbit/elements/epoch_utc: `yesterday` is not a UTC epoch (YYYY-MM-DDTHH:MM:SS.ffffffZ)",
        ),
        (
            "blank dataset path",
            |v| v["target"]["source"]["dataset"] = json!(" "),
            "[project.json] /target/source/dataset: must not be blank",
        ),
        (
            "target mass",
            |v| v["target"]["mass_kg"] = json!(0.0),
            "[project.json] /target/mass_kg: must be greater than 0",
        ),
        (
            "chaser mass is not supported yet",
            |v| v["chaser"]["mass_kg"] = json!(100.0),
            "[project.json] /chaser/mass_kg: the chaser's mass is not supported yet; it is applied from v0.4.0-rc.4",
        ),
        (
            "inertia is not supported yet",
            |v| {
                v["target"]["inertia_kg_m2"] =
                    json!([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
            },
            "[project.json] /target/inertia_kg_m2: the inertia is not supported yet; it is applied from v0.4.0-rc.4",
        ),
        (
            "a chaser relative to the target is not supported yet",
            |v| {
                v["chaser"]["orbit"] =
                    json!({ "relative_to_target": { "preset": "v_bar", "offset_m": 100.0 } })
            },
            "[project.json] /chaser/orbit/relative_to_target: an orbit relative to the target is not supported yet; it is applied from v0.4.0-rc.4",
        ),
        (
            "a target cannot be relative to itself",
            |v| {
                v["target"]["orbit"] =
                    json!({ "relative_to_target": { "preset": "r_bar", "offset_m": 100.0 } })
            },
            "[project.json] /target/orbit/relative_to_target: only the chaser can be placed relative to the target",
        ),
    ];
    for (rule, mutate, expected) in rows {
        let mut value = doc();
        mutate(&mut value);
        let report = read(&value);
        assert_eq!(report.errors, [expected], "{rule}");
        assert!(
            report.manifest.is_none(),
            "{rule}: a manifest with errors does not load"
        );
    }
}

#[test]
fn independent_errors_are_reported_in_field_order() {
    let mut value = doc();
    value["name"] = json!("");
    value["chaser"]["label"] = json!("");
    value["target"]["mass_kg"] = json!(-1.0);

    let report = read(&value);

    assert_eq!(
        report.errors,
        [
            "[project.json] /name: must not be blank",
            "[project.json] /target/mass_kg: must be greater than 0",
            "[project.json] /chaser/label: must not be blank",
        ]
    );
}

#[test]
fn sim_inputs_of_an_invalid_manifest_is_the_validators_errors() {
    let mut manifest = read(&doc()).manifest.unwrap();
    manifest.chaser.orbit = ManifestOrbit::RelativeToTarget(RelativeToTarget {
        preset: RelativePreset::VBar,
        offset_m: 10.0,
    });

    let errors = manifest.sim_inputs().expect_err("refused");

    assert_eq!(errors, manifest.validate());
    assert!(errors[0].contains("applied from v0.4.0-rc.4"), "{errors:?}");
}

#[test]
fn a_manifest_that_says_another_schema_version_does_not_validate() {
    let mut manifest = read(&doc()).manifest.unwrap();
    manifest.schema_version = 3;
    assert_eq!(
        manifest.validate(),
        ["[project.json] /schema_version: must be 1"]
    );
}

#[test]
fn non_finite_elements_are_reported_one_by_one() {
    let mut manifest = read(&doc()).manifest.unwrap();
    manifest.chaser.orbit = ManifestOrbit::Elements(ManifestElements {
        semi_major_axis_m: f64::NAN,
        eccentricity: f64::INFINITY,
        inclination_rad: 0.0,
        raan_rad: 0.0,
        arg_perigee_rad: 0.0,
        mean_anomaly_rad: f64::NEG_INFINITY,
        epoch_utc: START.to_string(),
    });
    assert_eq!(
        manifest.validate(),
        [
            "[project.json] /chaser/orbit/elements/semi_major_axis_m: must be a finite number",
            "[project.json] /chaser/orbit/elements/eccentricity: must be a finite number",
            "[project.json] /chaser/orbit/elements/mean_anomaly_rad: must be a finite number",
        ]
    );
    manifest.target.orbit = ManifestOrbit::RelativeToTarget(RelativeToTarget {
        preset: RelativePreset::RBar,
        offset_m: f64::NAN,
    });
    assert!(
        manifest
            .validate()
            .iter()
            .any(|e| e.ends_with("/offset_m: must be a finite number"))
    );
}

// ---------------------------------------------------------------------------
// files
// ---------------------------------------------------------------------------

#[test]
fn a_folder_without_project_json_has_no_report() {
    let dir = TempDir::new("manifest-absent");
    assert!(read_manifest_file(dir.path()).is_none());
}

#[test]
fn unreadable_project_json_is_reported() {
    let dir = TempDir::new("manifest-garbage");
    dir.write(MANIFEST_FILE, "{ not json");
    let report = read_manifest_file(dir.path()).expect("the file exists");
    assert_eq!(report.errors.len(), 1);
    assert!(
        report.errors[0].starts_with("[project.json] Invalid JSON: "),
        "{}",
        report.errors[0]
    );

    // A directory named project.json is not a file; bytes that are not UTF-8 cannot be read.
    std::fs::write(dir.path().join(MANIFEST_FILE), [0x7b, 0xff, 0xfe]).unwrap();
    let report = read_manifest_file(dir.path()).expect("the file exists");
    assert!(
        report.errors[0].starts_with("[project.json] Could not read file: "),
        "{}",
        report.errors[0]
    );
}

#[test]
fn a_saved_manifest_reads_back_equal_and_pretty_with_the_version_first() {
    let dir = TempDir::new("manifest-save");
    let manifest = read(&doc()).manifest.unwrap();

    save_manifest(dir.path(), &manifest).unwrap();

    let text = std::fs::read_to_string(dir.path().join(MANIFEST_FILE)).unwrap();
    assert_eq!(text, manifest_to_json(&manifest));
    assert!(text.starts_with("{\n  \"schema_version\": 1,"), "{text}");
    let report = read_manifest_file(dir.path()).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.manifest.unwrap(), manifest);
}

#[test]
fn a_missing_dataset_is_a_warning_and_the_stored_orbit_is_used() {
    let dir = TempDir::new("manifest-dataset");
    let manifest = read(&doc()).manifest.unwrap();
    save_manifest(dir.path(), &manifest).unwrap();

    let report = read_manifest_file(dir.path()).unwrap();
    assert!(report.errors.is_empty());
    assert_eq!(
        report.warnings,
        [
            "[project.json] /target/source/dataset: `datasets/dataset_2026_04_03.json` does not exist (the stored orbit is used)"
        ]
    );

    dir.write("datasets/dataset_2026_04_03.json", "[]");
    assert!(read_manifest_file(dir.path()).unwrap().warnings.is_empty());
}

#[test]
fn a_manifest_from_another_build_gets_a_note() {
    let dir = TempDir::new("manifest-built-with");
    let mut manifest = read(&doc()).manifest.unwrap();
    manifest.built_with = Some(BuiltWith {
        app_version: "0.0.1".to_string(),
        target: "nowhere-os".to_string(),
        avian_features: vec!["parallel".to_string()],
    });
    save_manifest(dir.path(), &manifest).unwrap();

    let report = read_manifest_file(dir.path()).unwrap();

    let running = BuiltWith::current();
    assert_eq!(
        report.notes,
        [format!(
            "[project.json] /built_with: written by app version 0.0.1 (this build is {}), platform nowhere-os (this build is {}), avian features [parallel] (this build has [simd, parallel]); results are bit-identical only on the same build and platform",
            running.app_version, running.target
        )]
    );

    manifest.built_with = Some(BuiltWith::current());
    save_manifest(dir.path(), &manifest).unwrap();
    assert!(read_manifest_file(dir.path()).unwrap().notes.is_empty());
}

#[test]
fn built_with_differences_name_each_difference() {
    let running = BuiltWith::current();
    assert!(running.differences(&running).is_empty());
    let mut other = running.clone();
    other.app_version = "9.9.9".to_string();
    assert_eq!(
        other.differences(&running),
        [format!(
            "app version 9.9.9 (this build is {})",
            running.app_version
        )]
    );
    assert_eq!(running.avian_features, ["simd", "parallel"]);
    assert!(running.target.contains('-'));
}

// ---------------------------------------------------------------------------
// selection <-> manifest
// ---------------------------------------------------------------------------

fn catalog_pick(norad_id: u64, label: &str) -> SelectedOrbitalObject {
    let (_, gp, _) = gp_record_to_elements(&q13::fixture_record(norad_id), fixed_epoch()).unwrap();
    SelectedOrbitalObject {
        source: OrbitalSelectionSource::Catalog {
            catalog_index: 3,
            gpu_index: 3,
            label: label.to_string(),
        },
        elements: EditableOrbitalElements::from_gp(&gp),
        gp: Some(gp),
        mass_kg: None,
    }
}

#[test]
fn a_selection_survives_the_manifest_and_a_file() {
    let dir = TempDir::new("manifest-selection");
    let mut edited = catalog_pick(q13::CHASER_NORAD_ID, "Chaser");
    edited.gp = None; // elements edited: no catalog record any more
    edited.elements.eccentricity = 0.00123456789012345;
    edited.elements.epoch = parse_utc("2026-04-03T16:03:47.002464").unwrap();
    let selection = OrbitalSelectionState {
        target: Some(catalog_pick(q13::HST_NORAD_ID, "HST")),
        chaser: Some(edited.clone()),
    };

    let manifest =
        ProjectManifest::from_selection("proj", fixed_epoch(), Some("example_plan"), &selection)
            .unwrap();
    save_manifest(dir.path(), &manifest).unwrap();
    let inputs = read_manifest_file(dir.path())
        .unwrap()
        .manifest
        .expect("a manifest built from a selection is valid")
        .sim_inputs()
        .unwrap();

    assert_eq!(inputs.start_epoch, fixed_epoch());
    assert_eq!(inputs.active_plan.as_deref(), Some("example_plan"));
    let original = selection.target.as_ref().unwrap();
    assert_eq!(
        inputs.target.gp, original.gp,
        "the GP elements come back equal"
    );
    assert_eq!(
        inputs.target.elements.to_vec6(),
        original.elements.to_vec6(),
        "and so do the elements derived from them"
    );
    assert_eq!(inputs.target.elements.epoch, original.elements.epoch);
    assert!(
        matches!(&inputs.target.source, OrbitalSelectionSource::Custom { label } if label == "HST")
    );
    assert!(inputs.chaser.gp.is_none());
    assert_eq!(inputs.chaser.elements.to_vec6(), edited.elements.to_vec6());
    assert_eq!(inputs.chaser.elements.epoch, edited.elements.epoch);
    assert_eq!(manifest.start_epoch_utc, format_utc(fixed_epoch()));
    assert_eq!(manifest.built_with, Some(BuiltWith::current()));
    let ManifestOrbit::Gp(record) = &manifest.target.orbit else {
        panic!("a catalog pick is written as its GP record");
    };
    assert_eq!(
        record.object_name.as_deref(),
        Some("HST"),
        "OBJECT_NAME is the label"
    );
}

#[test]
fn a_selection_needs_both_bodies() {
    let selection = OrbitalSelectionState {
        target: Some(catalog_pick(q13::HST_NORAD_ID, "HST")),
        chaser: None,
    };
    assert_eq!(
        ProjectManifest::from_selection("p", fixed_epoch(), None, &selection).unwrap_err(),
        "no chaser is selected"
    );
    let selection = OrbitalSelectionState {
        target: None,
        chaser: selection.target,
    };
    assert_eq!(
        ProjectManifest::from_selection("p", fixed_epoch(), None, &selection).unwrap_err(),
        "no target is selected"
    );
}

// ---------------------------------------------------------------------------
// files -> sim inputs
// ---------------------------------------------------------------------------

#[test]
fn read_sim_inputs_distinguishes_no_manifest_a_bad_one_and_a_good_one() {
    let dir = TempDir::new("manifest-inputs");
    assert!(
        read_sim_inputs(dir.path()).is_none(),
        "a v0 folder has no manifest"
    );

    dir.write(MANIFEST_FILE, "{ not json");
    let errors = read_sim_inputs(dir.path()).unwrap().unwrap_err();
    assert!(
        errors[0].starts_with("[project.json] Invalid JSON: "),
        "{errors:?}"
    );

    let mut value = doc();
    value["chaser"]["mass_kg"] = json!(5.0);
    dir.write(MANIFEST_FILE, &value.to_string());
    let errors = read_sim_inputs(dir.path()).unwrap().unwrap_err();
    assert_eq!(
        errors,
        [
            "[project.json] /chaser/mass_kg: the chaser's mass is not supported yet; it is applied from v0.4.0-rc.4"
        ]
    );

    dir.write(MANIFEST_FILE, &doc().to_string());
    let inputs = read_sim_inputs(dir.path()).unwrap().unwrap();
    assert_eq!(inputs.start_epoch, fixed_epoch());
}

#[test]
fn save_and_reload_starts_from_what_was_read() {
    let dir = TempDir::new("manifest-reload");
    let selection = OrbitalSelectionState {
        target: Some(catalog_pick(q13::HST_NORAD_ID, "HST")),
        chaser: Some(catalog_pick(q13::CHASER_NORAD_ID, "Chaser")),
    };
    let manifest =
        ProjectManifest::from_selection("proj", fixed_epoch(), Some("example_plan"), &selection)
            .unwrap();

    let (inputs, write_error) = save_and_reload(dir.path(), &manifest).unwrap();

    assert_eq!(write_error, None);
    assert!(
        dir.path().join(MANIFEST_FILE).is_file(),
        "the manifest was written"
    );
    assert_eq!(inputs.target.gp, selection.target.as_ref().unwrap().gp);
    assert_eq!(inputs.chaser.gp, selection.chaser.as_ref().unwrap().gp);
    assert_eq!(inputs.active_plan.as_deref(), Some("example_plan"));
    assert_eq!(inputs.start_epoch, fixed_epoch());
}

/// F-55: the record's `OBJECT_ID` is part of the GP snapshot in `project.json`; an
/// empty one is no id.
#[test]
fn a_catalog_pick_keeps_its_object_id_through_a_project_json() {
    let dir = TempDir::new("manifest-object-id");
    let selection = OrbitalSelectionState {
        target: Some(catalog_pick(q13::HST_NORAD_ID, "HST")),
        chaser: Some(catalog_pick(q13::CHASER_NORAD_ID, "Chaser")),
    };
    let manifest =
        ProjectManifest::from_selection("proj", fixed_epoch(), None, &selection).unwrap();

    let (inputs, write_error) = save_and_reload(dir.path(), &manifest).unwrap();

    assert_eq!(write_error, None);
    let object_id =
        |body: &SelectedOrbitalObject| body.gp.as_ref().and_then(|gp| gp.object_id.clone());
    assert_eq!(object_id(&inputs.target).as_deref(), Some("1990-037B"));
    assert_eq!(object_id(&inputs.chaser).as_deref(), Some("P0-CHASER-1"));
    let mut record = q13::fixture_record(q13::HST_NORAD_ID);
    record.object_id = Some(String::new());
    let (_, gp, _) = gp_record_to_elements(&record, fixed_epoch()).unwrap();
    assert_eq!(gp.object_id, None, "an empty OBJECT_ID is none");
}

#[test]
fn save_and_reload_without_a_writable_folder_starts_from_the_manifest_itself() {
    let dir = TempDir::new("manifest-reload-unwritable");
    let missing = dir.path().join("not").join("there");
    let selection = OrbitalSelectionState {
        target: Some(catalog_pick(q13::HST_NORAD_ID, "HST")),
        chaser: Some(catalog_pick(q13::CHASER_NORAD_ID, "Chaser")),
    };
    let manifest =
        ProjectManifest::from_selection("proj", fixed_epoch(), None, &selection).unwrap();

    let (inputs, write_error) = save_and_reload(&missing, &manifest).unwrap();

    let message = write_error.expect("the write failed");
    assert!(
        message.starts_with("could not write project.json: "),
        "{message}"
    );
    assert_eq!(
        inputs.target.gp,
        selection.target.as_ref().unwrap().gp,
        "in-memory inputs"
    );
    assert!(!missing.exists());
}

#[test]
fn save_and_reload_refuses_a_manifest_that_is_not_valid() {
    let dir = TempDir::new("manifest-reload-invalid");
    let mut manifest = read(&doc()).manifest.unwrap();
    manifest.name = String::new();
    manifest.chaser.mass_kg = Some(1.0);

    let errors = save_and_reload(dir.path(), &manifest).unwrap_err();

    assert_eq!(errors.len(), 2, "{errors:?}");
    assert_eq!(errors[0], "[project.json] /name: must not be blank");
    // And when the folder cannot be written, the manifest itself is checked.
    let errors = save_and_reload(&dir.path().join("gone"), &manifest).unwrap_err();
    assert_eq!(errors.len(), 2, "{errors:?}");
}

// ---------------------------------------------------------------------------
// properties (fixed seeds; the seed and the case are printed on failure)
// ---------------------------------------------------------------------------

/// A random number with all 17 significant digits a double can need.
fn full_precision(rng: &mut Lcg, lo: f64, hi: f64) -> f64 {
    rng.range(lo, hi)
}

fn generated_elements(rng: &mut Lcg) -> ManifestElements {
    ManifestElements {
        semi_major_axis_m: full_precision(rng, 6.4e6, 4.3e7),
        eccentricity: full_precision(rng, 0.0, 0.9),
        inclination_rad: full_precision(rng, 0.0, std::f64::consts::PI),
        raan_rad: full_precision(rng, 0.0, std::f64::consts::TAU),
        arg_perigee_rad: full_precision(rng, 0.0, std::f64::consts::TAU),
        mean_anomaly_rad: full_precision(rng, 0.0, std::f64::consts::TAU),
        epoch_utc: format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
            2000 + rng.below(60),
            1 + rng.below(12),
            1 + rng.below(28),
            rng.below(24),
            rng.below(60),
            rng.below(60),
            rng.below(1_000_000)
        ),
    }
}

fn bits(e: &ManifestElements) -> [u64; 6] {
    [
        e.semi_major_axis_m,
        e.eccentricity,
        e.inclination_rad,
        e.raan_rad,
        e.arg_perigee_rad,
        e.mean_anomaly_rad,
    ]
    .map(f64::to_bits)
}

/// Elements with 17 significant digits (metres and radians) survive writing and
/// reading `project.json` bit for bit. `serde_json`'s `float_roundtrip` feature
/// makes the reader exact (without it, about one 17-digit number in ten reads back
/// one ULP off).
#[test]
fn seventeen_digit_elements_survive_a_project_json_write_and_read_bit_for_bit() {
    let seed = 0x17D1_617E;
    let mut rng = Lcg::new(seed);
    let dir = TempDir::new("manifest-bits");
    let mut seventeen = 0;
    for case in 0..500 {
        let elements = generated_elements(&mut rng);
        let mut manifest = read(&doc()).manifest.unwrap();
        manifest.chaser.orbit = ManifestOrbit::Elements(elements.clone());

        save_manifest(dir.path(), &manifest).unwrap();
        let report = read_manifest_file(dir.path()).unwrap();

        let ManifestOrbit::Elements(read_back) = report.manifest.expect("valid").chaser.orbit
        else {
            panic!("seed {seed:#x} case {case}: the orbit kind changed");
        };
        assert_eq!(
            bits(&read_back),
            bits(&elements),
            "seed {seed:#x} case {case}: {elements:?} read back as {read_back:?}"
        );
        assert_eq!(
            read_back.epoch_utc, elements.epoch_utc,
            "seed {seed:#x} case {case}"
        );
        seventeen += bits_with_17_digits(&elements);
    }
    println!(
        "seed {seed:#x}: 500 element sets written and read back bit for bit ({seventeen} of 3000 numbers have 17 significant digits)"
    );
    assert!(
        seventeen > 500,
        "the generator must produce 17-digit numbers"
    );
}

fn bits_with_17_digits(e: &ManifestElements) -> usize {
    [
        e.semi_major_axis_m,
        e.eccentricity,
        e.inclination_rad,
        e.raan_rad,
        e.arg_perigee_rad,
        e.mean_anomaly_rad,
    ]
    .iter()
    .filter(|v| {
        format!("{:e}", v)
            .split('e')
            .next()
            .unwrap()
            .replace(['.', '-'], "")
            .len()
            >= 17
    })
    .count()
}

fn generated_record(rng: &mut Lcg, case: usize) -> JsonOrbital {
    let number = |rng: &mut Lcg, lo, hi| Some(Value::from(rng.range(lo, hi)));
    JsonOrbital {
        object_name: rng.chance(0.8).then(|| format!("OBJ {case}")),
        object_id: rng.chance(0.5).then(|| format!("2026-{:03}A", case % 1000)),
        epoch: Some(format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}",
            2020 + rng.below(10),
            1 + rng.below(12),
            1 + rng.below(28),
            rng.below(24),
            rng.below(60),
            rng.below(60),
            rng.below(1_000_000)
        )),
        mean_motion: number(rng, 0.5, 16.5),
        eccentricity: number(rng, 0.0, 0.7),
        inclination: number(rng, 0.0, 180.0),
        ra_of_asc_node: number(rng, 0.0, 360.0),
        arg_of_pericenter: number(rng, 0.0, 360.0),
        mean_anomaly: number(rng, 0.0, 360.0),
        ephemeris_type: rng.chance(0.5).then(|| Value::from(rng.below(3) as u64)),
        classification_type: rng.chance(0.5).then(|| "U".to_string()),
        norad_cat_id: Some(Value::from(1 + rng.below(99_999) as u64)),
        element_set_no: rng.chance(0.3).then(|| Value::from(rng.below(999) as u64)),
        rev_at_epoch: rng
            .chance(0.3)
            .then(|| Value::from(rng.below(99_999) as u64)),
        bstar: rng.chance(0.7).then(|| Value::from(rng.range(-1e-3, 1e-3))),
        mean_motion_dot: rng.chance(0.7).then(|| Value::from(rng.range(-1e-4, 1e-4))),
        mean_motion_ddot: rng.chance(0.7).then(|| Value::from(rng.range(-1e-6, 1e-6))),
    }
}

/// A GP record put in a manifest, written, read back and turned into elements gives
/// exactly the `GpElements` the record itself gives.
#[test]
fn generated_gp_records_give_the_same_elements_through_a_project_json() {
    let seed = 0x6E0_5EED;
    let mut rng = Lcg::new(seed);
    let dir = TempDir::new("manifest-gp");
    let reference = fixed_epoch();
    for case in 0..300 {
        let record = generated_record(&mut rng, case);
        let (_, expected, _) = gp_record_to_elements(&record, reference).unwrap_or_else(|| {
            panic!("seed {seed:#x} case {case}: the generator made an invalid record")
        });
        let mut manifest = read(&doc()).manifest.unwrap();
        manifest.target = ManifestBody {
            label: format!("Body {case}"),
            orbit: ManifestOrbit::Gp(Box::new(record.clone())),
            source: rng.chance(0.5).then(|| ManifestSource {
                dataset: "datasets/x.json".to_string(),
                norad_id: 1,
            }),
            mass_kg: None,
            inertia_kg_m2: None,
        };

        save_manifest(dir.path(), &manifest).unwrap();
        let read_back = read_manifest_file(dir.path()).unwrap();
        assert!(
            read_back.errors.is_empty(),
            "seed {seed:#x} case {case}: {:?}",
            read_back.errors
        );
        let inputs = read_back.manifest.unwrap().sim_inputs().unwrap();

        assert_eq!(
            inputs.target.gp.as_ref(),
            Some(&expected),
            "seed {seed:#x} case {case}: {record:?}"
        );
        assert_eq!(
            inputs.target.elements.to_vec6(),
            EditableOrbitalElements::from_gp(&expected).to_vec6(),
            "seed {seed:#x} case {case}"
        );
    }
    println!("seed {seed:#x}: 300 GP records give equal elements after a project.json round trip");
}

/// A selection written and read again, 300 times over generated GP records.
#[test]
fn generated_selections_survive_from_selection_and_sim_inputs() {
    let seed = 0x5E1_EC7;
    let mut rng = Lcg::new(seed);
    for case in 0..300 {
        let record = generated_record(&mut rng, case);
        let (_, gp, _) = gp_record_to_elements(&record, fixed_epoch()).unwrap();
        let picked = SelectedOrbitalObject {
            source: OrbitalSelectionSource::Custom {
                label: format!("B{case}"),
            },
            elements: EditableOrbitalElements::from_gp(&gp),
            gp: Some(gp.clone()),
            mass_kg: None,
        };
        let edited = SelectedOrbitalObject {
            source: OrbitalSelectionSource::Custom {
                label: format!("E{case}"),
            },
            elements: {
                let e = generated_elements(&mut rng);
                EditableOrbitalElements {
                    semi_major_axis_m: e.semi_major_axis_m,
                    eccentricity: e.eccentricity,
                    inclination_rad: e.inclination_rad,
                    raan_rad: e.raan_rad,
                    arg_perigee_rad: e.arg_perigee_rad,
                    mean_anomaly_rad: e.mean_anomaly_rad,
                    epoch: parse_utc(&e.epoch_utc).unwrap(),
                }
            },
            gp: None,
            mass_kg: None,
        };
        let selection = OrbitalSelectionState {
            target: Some(picked),
            chaser: Some(edited.clone()),
        };

        let manifest =
            ProjectManifest::from_selection("p", fixed_epoch(), None, &selection).unwrap();
        let text = manifest_to_json(&manifest);
        let report = read_manifest_value(&serde_json::from_str(&text).unwrap());
        assert!(
            report.errors.is_empty(),
            "seed {seed:#x} case {case}: {:?}",
            report.errors
        );
        let inputs = report.manifest.unwrap().sim_inputs().unwrap();

        assert_eq!(inputs.target.gp, Some(gp), "seed {seed:#x} case {case}");
        assert_eq!(
            inputs.chaser.elements.to_vec6(),
            edited.elements.to_vec6(),
            "seed {seed:#x} case {case}"
        );
        assert_eq!(
            inputs.chaser.elements.epoch, edited.elements.epoch,
            "seed {seed:#x} case {case}"
        );
    }
    println!("seed {seed:#x}: 300 selections survive from_selection, a JSON text and sim_inputs");
}

#[test]
fn the_targets_mass_reaches_the_selection_and_survives_writing_it_back() {
    // P6 S4: `target.mass_kg` is applied by the sim, so it travels with the selection.
    let manifest = read(&doc()).manifest.unwrap();
    let inputs = manifest.sim_inputs().unwrap();
    assert_eq!(inputs.target.mass_kg, Some(2500.0));
    assert_eq!(inputs.chaser.mass_kg, None);

    let mut value = doc();
    value["target"]["mass_kg"] = json!(3200.5);
    let inputs = read(&value).manifest.unwrap().sim_inputs().unwrap();
    assert_eq!(inputs.target.mass_kg, Some(3200.5));

    let selection = OrbitalSelectionState {
        target: Some(inputs.target),
        chaser: Some(inputs.chaser),
    };
    let written = ProjectManifest::from_selection("p", fixed_epoch(), None, &selection).unwrap();
    assert_eq!(written.target.mass_kg, Some(3200.5));
    assert_eq!(written.chaser.mass_kg, None);
    let text = manifest_to_json(&written);
    let reread = read_manifest_value(&serde_json::from_str(&text).unwrap());
    assert_eq!(reread.errors, Vec::<String>::new());
    assert_eq!(
        reread
            .manifest
            .unwrap()
            .sim_inputs()
            .unwrap()
            .target
            .mass_kg,
        Some(3200.5)
    );
}

// ---------------------------------------------------------------------------
// the published JSON Schema
// ---------------------------------------------------------------------------

#[test]
fn project_schema_file_is_current() {
    check_schema::<ProjectManifest>("project.v1.schema.json");
}

#[test]
fn the_project_schema_requires_the_v1_keys_and_refuses_unknown_ones() {
    let schema = serde_json::to_value(schemars::schema_for!(ProjectManifest)).unwrap();
    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    required.sort();
    assert_eq!(
        required,
        [
            "chaser",
            "name",
            "schema_version",
            "start_epoch_utc",
            "target"
        ]
    );
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(schema["properties"]["schema_version"]["minimum"], json!(1));
    assert_eq!(schema["properties"]["schema_version"]["maximum"], json!(1));
}
