/// This is the main entrypoint for the application.
/// Shared logic between the app and test environments
/// can be found in lib.rs
fn main() {
    // When launched from a macOS .app bundle the working directory defaults to '/'.
    // Bevy resolves the 'assets/' folder relative to CWD, but cargo-bundle places
    // assets in Contents/Resources/assets/. Relocate CWD to Contents/Resources/
    // so Bevy finds the correct path. This only activates inside an actual bundle;
    // normal `cargo run` development is unaffected.
    #[cfg(target_os = "macos")]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(resources) = exe
            .parent()                           // Contents/MacOS/
            .and_then(|p| p.parent())           // Contents/
            .map(|p| p.join("Resources"))       // Contents/Resources/
        {
            if resources.is_dir() {
                let _ = std::env::set_current_dir(&resources);
            }
        }
    }

    ssg_tether_capture::run();
}
