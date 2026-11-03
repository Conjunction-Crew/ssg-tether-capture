//! egui screen test harness (P8 S1, OQ1 (a)): drives a pure screen function
//! `fn(&mut egui::Ui, &View, &mut Vec<UiEvent>)` under `egui_kittest` with the
//! app's theme. `harness_for` is headless (CI); `snapshot` renders on the GPU
//! for the ignored `p8_screen_snapshots` test only.

use std::path::PathBuf;

use bevy_egui::egui;
use egui_kittest::Harness;

use crate::ui::egui_theme::visuals_from_theme;
use crate::ui::events::UiEvent;
use crate::ui::theme::UiTheme;

/// The harness state: the screen's view and the events its controls emitted.
pub(crate) struct ScreenState<V> {
    pub view: V,
    pub events: Vec<UiEvent>,
}

fn build<'a, V: 'a>(
    view: V,
    size: impl Into<egui::Vec2>,
    screen: impl Fn(&mut egui::Ui, &V, &mut Vec<UiEvent>) + 'a,
    gpu: bool,
) -> Harness<'a, ScreenState<V>> {
    let mut builder = Harness::builder().with_size(size);
    if gpu {
        builder = builder.wgpu();
    }
    // A central panel fills the frame with the theme's panel colour, as the
    // app's panels do.
    let mut harness = builder.build_state(
        move |ctx, state: &mut ScreenState<V>| {
            egui::CentralPanel::default()
                .show(ctx, |ui| screen(ui, &state.view, &mut state.events));
        },
        ScreenState {
            view,
            events: Vec::new(),
        },
    );
    harness
        .ctx
        .set_visuals(visuals_from_theme(&UiTheme::default()));
    harness.run();
    harness
}

/// A headless harness (no GPU) for `screen` showing `view` in a `size` window.
pub(crate) fn harness_for<'a, V: 'a>(
    view: V,
    size: impl Into<egui::Vec2>,
    screen: impl Fn(&mut egui::Ui, &V, &mut Vec<UiEvent>) + 'a,
) -> Harness<'a, ScreenState<V>> {
    build(view, size, screen, false)
}

/// Where `snapshot` writes: `$CARGO_TARGET_DIR/p8-snapshots`, else the
/// workspace `target/p8-snapshots`.
pub(crate) fn snapshot_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"))
        .join("p8-snapshots")
}

/// Renders `screen` showing `view` on the GPU and writes `<name>.png` under
/// [`snapshot_dir`]. Returns the file written. Never compared against a
/// baseline: the PNGs are evidence for review, not golden files (rule 28).
pub(crate) fn snapshot<V>(
    name: &str,
    view: V,
    size: impl Into<egui::Vec2>,
    screen: impl Fn(&mut egui::Ui, &V, &mut Vec<UiEvent>),
) -> PathBuf {
    let mut harness = build(view, size, screen, true);
    let image = harness
        .render()
        .unwrap_or_else(|error| panic!("rendering snapshot {name}: {error}"));
    let dir = snapshot_dir();
    std::fs::create_dir_all(&dir).expect("create the snapshot folder");
    let path = dir.join(format!("{name}.png"));
    image.save(&path).expect("write the snapshot PNG");
    path
}
