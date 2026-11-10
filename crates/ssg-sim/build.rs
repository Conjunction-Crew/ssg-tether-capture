//! Names the commit this build was made from, for the `commit` of a recording's
//! `built_with` (`ssg-sim replay` judges by it). `git rev-parse --short=12 HEAD`,
//! with `-dirty` appended when tracked files have uncommitted changes. Without git,
//! or when the workspace is not the top of a repository (a source archive),
//! nothing is set and `commit` stays absent. This script never fails the build.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
    // The repository must be this workspace's, not one the sources were unpacked
    // into.
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let top = git(&["rev-parse", "--show-toplevel"]).map(std::path::PathBuf::from);
    match (
        top.and_then(|t| t.canonicalize().ok()),
        workspace.canonicalize().ok(),
    ) {
        (Some(top), Some(workspace)) if top == workspace => {}
        _ => return,
    }
    let Some(commit) = git(&["rev-parse", "--short=12", "HEAD"]).filter(|c| !c.is_empty()) else {
        return;
    };
    let Some(status) = git(&["status", "--porcelain", "--untracked-files=no"]) else {
        return;
    };
    let dirty = if status.is_empty() { "" } else { "-dirty" };
    println!("cargo:rustc-env=SSG_BUILD_COMMIT={commit}{dirty}");
}
