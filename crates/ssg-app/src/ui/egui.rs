use bevy::prelude::*;
use bevy_egui::{
    EguiContexts,
    egui::{self, Pos2},
};
use egui_plot::{Legend, Line, Plot, PlotPoints};

use std::path::{Path, PathBuf};

use ssg_sim::capture::CaptureComponent;
use ssg_sim::orbit::selection::{OrbitalSelectionSource, OrbitalSelectionState};
use ssg_sim::project::export::{OemObject, sanitize_file_stem, write_capture_csv, write_oem};
use ssg_sim::project::{SelectedProject, WorkingDirectory};
use ssg_sim::telemetry::DataCollection;
use ssg_sim::time::WorldTime;

/// CSV export state of the data collection window (`DataCollection.settings`
/// before P3, F-18).
#[derive(Resource, Debug)]
pub struct CsvExportState {
    pub selecting_csv_dir: bool,
    pub csv_export_filename: String,
    pub num_exports_completed: u32,
}

impl Default for CsvExportState {
    fn default() -> Self {
        Self {
            selecting_csv_dir: false,
            csv_export_filename: String::from("capture0.csv"),
            num_exports_completed: 0,
        }
    }
}

/// `<project>/exports/`, created on demand (design D1).
fn exports_dir(working_directory: &WorkingDirectory) -> Result<PathBuf, String> {
    let dir = Path::new(&working_directory.path).join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// The label of a selected body (`OBJECT_NAME` in the OEM file).
fn body_label(selection: &OrbitalSelectionState, chaser: bool) -> String {
    let body = if chaser {
        &selection.chaser
    } else {
        &selection.target
    };
    match body.as_ref().map(|body| &body.source) {
        Some(
            OrbitalSelectionSource::Catalog { label, .. }
            | OrbitalSelectionSource::Custom { label },
        ) => label.clone(),
        None if chaser => "chaser".to_string(),
        None => "target".to_string(),
    }
}

pub fn egui_plots(
    capture_entities: Query<Entity, With<CaptureComponent>>,
    mut contexts: EguiContexts,
    data_collection: Res<DataCollection>,
    mut csv_export: ResMut<CsvExportState>,
    working_directory: Res<WorkingDirectory>,
    selected_project: Res<SelectedProject>,
    selection: Res<OrbitalSelectionState>,
    world_time: Option<Res<WorldTime>>,
) {
    let capture_entity_count = capture_entities.iter().len();
    if capture_entity_count != 1 {
        if capture_entity_count == 0 {
            debug!("Skipping egui plots: no active capture entity");
        } else {
            warn!(
                "Skipping egui plots: expected one active capture entity, found {}",
                capture_entity_count
            );
        }
        return;
    }

    let capture_entity = capture_entities
        .single()
        .expect("capture entity count checked above");

    let Some(vel_data) = data_collection.velocity.get(&capture_entity) else {
        debug!(
            ?capture_entity,
            "Skipping egui plots: missing velocity data for capture entity"
        );
        return;
    };
    let Some(pos_data) = data_collection.position.get(&capture_entity) else {
        debug!(
            ?capture_entity,
            "Skipping egui plots: missing position data for capture entity"
        );
        return;
    };

    let settings = &*csv_export;

    let mut export_clicked = false;
    let mut selected_filename: String = settings.csv_export_filename.clone();
    let mut num_exports = settings.num_exports_completed;

    let ctx = contexts.ctx_mut().unwrap();
    let available_rect = ctx.available_rect();
    egui::Window::new("Data Collection")
        .default_pos(Pos2::new(0.0, 1920.0))
        .constrain_to(available_rect)
        .show(ctx, |ui| {
            let pos_points: PlotPoints =
                pos_data.iter().map(|(epoch, vel)| [*epoch, *vel]).collect();
            let vel_points: PlotPoints =
                vel_data.iter().map(|(epoch, vel)| [*epoch, *vel]).collect();
            let pos_line = Line::new("meters", pos_points);
            let vel_line = Line::new("meters per second", vel_points);

            Plot::new("Relative Distance")
                .view_aspect(2.0)
                .legend(Legend::default().title("Relative Distance"))
                .show(ui, |plot_ui| {
                    plot_ui.line(pos_line);
                });

            Plot::new("Relative Velocity")
                .view_aspect(2.0)
                .legend(Legend::default().title("Relative Velocity"))
                .show(ui, |plot_ui| {
                    plot_ui.line(vel_line);
                });

            if settings.selecting_csv_dir {
                export_clicked = true;
                ui.label(format!(
                    "{}",
                    Path::new(&working_directory.path).join("exports").display()
                ));
                ui.text_edit_singleline(&mut selected_filename);
                if ui.button("Export").clicked() {
                    export_clicked = false;
                    let written = exports_dir(&working_directory).and_then(|dir| {
                        let path = dir.join(&selected_filename);
                        write_capture_csv(&path, pos_data, vel_data).map(|()| path)
                    });
                    match written {
                        Ok(path) => {
                            num_exports += 1;
                            info!(
                                path = %path.display(),
                                rows = pos_data.len().min(vel_data.len()),
                                exports_completed = num_exports,
                                "Completed egui data collection CSV export"
                            );
                        }
                        Err(e) => error!(error = %e, "CSV export failed"),
                    }
                }
            } else {
                ui.horizontal(|ui| {
                    let Some(project_id) = selected_project.project_id.clone() else {
                        return;
                    };
                    // `<plan-id>-<stamp>`, sanitised (F-33's CSV part).
                    let stem = format!(
                        "{}-{}",
                        sanitize_file_stem(&project_id),
                        chrono::Local::now().format("%d-%m-%Y_%H-%M-%S")
                    );
                    if ui.button("Export to CSV").clicked() {
                        selected_filename = format!("{stem}.csv");
                        export_clicked = true;
                        info!(
                            project_id = %project_id,
                            filename = %selected_filename,
                            "Preparing egui data collection CSV export"
                        );
                    }
                    if ui.button("Export OEM").clicked() {
                        export_oem(
                            &working_directory,
                            &format!("{stem}.oem"),
                            &selection,
                            world_time.as_deref(),
                            data_collection.ephemeris.get(&capture_entity),
                        );
                    }
                });
            }
        });

    csv_export.selecting_csv_dir = export_clicked;
    csv_export.csv_export_filename = selected_filename;
    csv_export.num_exports_completed = num_exports;
}

/// Writes the capture's ephemeris samples as a CCSDS OEM file (target, then the
/// tether root as the chaser) to `<project>/exports/<filename>`.
fn export_oem(
    working_directory: &WorkingDirectory,
    filename: &str,
    selection: &OrbitalSelectionState,
    world_time: Option<&WorldTime>,
    samples: Option<&Vec<ssg_sim::telemetry::EphemerisSample>>,
) {
    let (target, chaser) = (body_label(selection, false), body_label(selection, true));
    let samples = samples.map(Vec::as_slice).unwrap_or_default();
    let segment = |name, chaser: bool| OemObject {
        name,
        id: None,
        states: samples
            .iter()
            .map(|e| (e.epoch, if chaser { e.chaser } else { e.target }))
            .collect(),
    };
    let written = world_time
        .ok_or_else(|| "no sim clock".to_string())
        .and_then(|world_time| {
            let path = exports_dir(working_directory)?.join(filename);
            write_oem(
                &path,
                world_time.start_epoch,
                &[segment(&target, false), segment(&chaser, true)],
            )
            .map(|()| path)
        });
    match written {
        Ok(path) => info!(path = %path.display(), states = samples.len(), "Completed OEM export"),
        Err(e) => error!(error = %e, "OEM export failed"),
    }
}
