use ssg_sim::project::manifest::BuiltWith;

use super::{Maker, built_with_line, defect_message, judge, other_build_message};

fn build(version: &str, target: &str, features: &[&str], commit: Option<&str>) -> BuiltWith {
    BuiltWith {
        app_version: version.to_string(),
        target: target.to_string(),
        avian_features: features.iter().map(|f| f.to_string()).collect(),
        commit: commit.map(str::to_string),
    }
}

const ED: &[&str] = &["enhanced-determinism", "parallel"];

/// The judgement over a table: version, avian features and (when both name one)
/// the commit decide; the platform never does.
#[test]
fn judge_decides_by_version_features_and_commit_never_by_target() {
    let current = build("0.5.0", "x86_64-linux", ED, Some("0123456789ab"));
    let other = |what: &[&str]| Maker::OtherBuild {
        what: what.iter().map(|w| w.to_string()).collect(),
    };
    let cases: [(&str, BuiltWith, Maker); 9] = [
        (
            "same everything",
            build("0.5.0", "x86_64-linux", ED, Some("0123456789ab")),
            Maker::ThisBuild,
        ),
        (
            "other version",
            build("0.4.0", "x86_64-linux", ED, Some("0123456789ab")),
            other(&["version 0.4.0 (this build is 0.5.0)"]),
        ),
        (
            "other avian features",
            build(
                "0.5.0",
                "x86_64-linux",
                &["simd", "parallel"],
                Some("0123456789ab"),
            ),
            other(&[
                "avian features [simd, parallel] (this build has [enhanced-determinism, parallel])",
            ]),
        ),
        (
            "other target only",
            build("0.5.0", "aarch64-macos", ED, Some("0123456789ab")),
            Maker::ThisBuild,
        ),
        (
            "both commits, different",
            build("0.5.0", "x86_64-linux", ED, Some("ba9876543210")),
            other(&["commit ba9876543210 (this build is 0123456789ab)"]),
        ),
        (
            "the same commit, one dirty",
            build("0.5.0", "x86_64-linux", ED, Some("0123456789ab-dirty")),
            other(&["commit 0123456789ab-dirty (this build is 0123456789ab)"]),
        ),
        (
            "recording without a commit",
            build("0.5.0", "aarch64-macos", ED, None),
            Maker::ThisBuild,
        ),
        (
            "recording without a commit, other version",
            build("0.4.1", "aarch64-macos", ED, None),
            other(&["version 0.4.1 (this build is 0.5.0)"]),
        ),
        (
            "version, features and commit all differ",
            build(
                "0.4.0",
                "x86_64-windows",
                &["simd", "parallel"],
                Some("ba9876543210"),
            ),
            other(&[
                "version 0.4.0 (this build is 0.5.0)",
                "avian features [simd, parallel] (this build has [enhanced-determinism, parallel])",
                "commit ba9876543210 (this build is 0123456789ab)",
            ]),
        ),
    ];
    for (name, recorded, expected) in cases {
        assert_eq!(judge(&recorded, &current), expected, "{name}");
    }

    // This build without a commit (built outside git): judged by version and features.
    let no_commit = build("0.5.0", "x86_64-linux", ED, None);
    assert_eq!(
        judge(
            &build("0.5.0", "aarch64-macos", ED, Some("ba9876543210")),
            &no_commit
        ),
        Maker::ThisBuild
    );
    assert_eq!(judge(&no_commit, &no_commit), Maker::ThisBuild);
}

#[test]
fn the_messages_name_the_builds() {
    let current = build("0.5.0", "x86_64-linux", ED, Some("0123456789ab"));
    let recorded = build("0.5.0", "aarch64-macos", ED, Some("0123456789ab"));
    assert_eq!(
        defect_message(&recorded, &current),
        "the recording was made by this build (version 0.5.0, commit 0123456789ab, on \
         aarch64-macos) and does not re-simulate identically here (x86_64-linux): a defect; \
         please report it with the recording."
    );
    let dirty = build("0.5.0", "aarch64-macos", ED, Some("0123456789ab-dirty"));
    let message = defect_message(&dirty, &build("0.5.0", "x86_64-linux", ED, None));
    assert!(
        message.ends_with(
            "\none of the two builds had uncommitted changes, which the commit name cannot \
             tell apart."
        ),
        "{message}"
    );
    assert_eq!(
        other_build_message(&build("0.4.0", "aarch64-macos", ED, None), &current),
        "the recording was made by another build (version 0.4.0, commit unknown; this build is \
         version 0.5.0, commit 0123456789ab): a difference is expected. Re-simulation is \
         identical only for the same version."
    );

    assert_eq!(
        built_with_line(&current, &current),
        "built with: this build (commit 0123456789ab)"
    );
    assert_eq!(
        built_with_line(&build("0.5.0", "x86_64-linux", ED, None), &current),
        "built with: this build"
    );
    assert_eq!(
        built_with_line(&recorded, &current),
        "built with: platform aarch64-macos (this build is x86_64-linux)"
    );
    assert_eq!(
        built_with_line(
            &build("0.5.0", "x86_64-linux", ED, Some("ba9876543210")),
            &build("0.5.0", "x86_64-linux", ED, None)
        ),
        "built with: commit ba9876543210 (this build names none)"
    );
}
