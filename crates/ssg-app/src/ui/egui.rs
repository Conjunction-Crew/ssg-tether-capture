use bevy::ecs::system::SystemParam;
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
use ssg_sim::telemetry::{DataCollection, LogEvent, LogLevel};
use ssg_sim::time::WorldTime;

use crate::ui::form::UnitSystem;
use crate::ui::settings::Settings;

/// CSV export state of the data collection window (`DataCollection.settings`
/// before P3, F-18).
#[derive(Resource, Debug)]
pub struct CsvExportState {
    pub selecting_csv_dir: bool,
    pub csv_export_filename: String,
    pub num_exports_completed: u32,
    /// The last export's failure, shown in the window until the next export (F-33).
    pub export_error: Option<String>,
    /// Which session the plots belong to: their view is kept per session (N8).
    pub plot_session: u32,
}

impl Default for CsvExportState {
    fn default() -> Self {
        Self {
            selecting_csv_dir: false,
            csv_export_filename: String::from("capture0.csv"),
            num_exports_completed: 0,
            export_error: None,
            plot_session: 0,
        }
    }
}

/// Entering the sim: the plots start from a fresh view, not the last project's
/// scroll and zoom (N8).
pub fn begin_plot_session(mut csv_export: ResMut<CsvExportState>) {
    csv_export.plot_session = csv_export.plot_session.wrapping_add(1);
    csv_export.export_error = None;
}

/// `<project>/exports/`, created on demand (design D1).
fn exports_dir(working_directory: &WorkingDirectory) -> Result<PathBuf, String> {
    let dir = Path::new(&working_directory.path).join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// The label of a selected body (`OBJECT_NAME` in the OEM file).
pub(crate) fn body_label(selection: &OrbitalSelectionState, chaser: bool) -> String {
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

/// The `OBJECT_ID` of a selected body's GP record, when it has one.
fn body_object_id(selection: &OrbitalSelectionState, chaser: bool) -> Option<&str> {
    let body = if chaser {
        &selection.chaser
    } else {
        &selection.target
    };
    body.as_ref()?.gp.as_ref()?.object_id.as_deref()
}

/// What the two plots show. The samples are metric; `units` converts them.
pub(crate) struct PlotsView<'a> {
    pub position: &'a [(f64, f64)],
    pub velocity: &'a [(f64, f64)],
    pub units: UnitSystem,
    pub session: u32,
}

/// Egui remembers a plot's scroll and zoom by this id: a new session or a units
/// change starts from a fresh view (N8).
pub(crate) fn plot_id(title: &str, session: u32, units: UnitSystem) -> egui::Id {
    egui::Id::new((title, session, units))
}

/// The legend's unit names, for the distance and the velocity plot.
pub(crate) fn plot_line_names(units: UnitSystem) -> (&'static str, &'static str) {
    match units {
        UnitSystem::Metric => ("meters", "meters per second"),
        UnitSystem::Imperial => ("feet", "feet per second"),
    }
}

fn plot(ui: &mut egui::Ui, title: &str, line: Line, view: &PlotsView) {
    Plot::new(title)
        .id(plot_id(title, view.session, view.units))
        .view_aspect(2.0)
        .legend(Legend::default().title(title))
        .show(ui, |plot_ui| plot_ui.line(line));
}

pub(crate) fn plots_ui(ui: &mut egui::Ui, view: &PlotsView) {
    let (distance_name, speed_name) = plot_line_names(view.units);
    let pos_points: PlotPoints = view
        .position
        .iter()
        .map(|(epoch, m)| [*epoch, view.units.length(*m).0])
        .collect();
    let vel_points: PlotPoints = view
        .velocity
        .iter()
        .map(|(epoch, m_s)| [*epoch, view.units.speed(*m_s).0])
        .collect();
    plot(
        ui,
        "Relative Distance",
        Line::new(distance_name, pos_points),
        view,
    );
    plot(
        ui,
        "Relative Velocity",
        Line::new(speed_name, vel_points),
        view,
    );
}

/// The last export's failure, under the plots (F-33).
pub(crate) fn export_error_ui(ui: &mut egui::Ui, error: Option<&str>) {
    if let Some(error) = error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
}

/// Keeps an export's outcome: a failure is shown in the window and logged (F-33).
pub(crate) fn record_export(
    csv_export: &mut CsvExportState,
    log: &mut MessageWriter<LogEvent>,
    kind: &str,
    outcome: Result<(), String>,
) {
    csv_export.export_error = outcome.err().map(|e| {
        let message = format!("{kind} export failed: {e}");
        error!("{message}");
        log.write(LogEvent {
            level: LogLevel::Error,
            source: "ui",
            message: message.clone(),
        });
        message
    });
}

#[derive(SystemParam)]
pub struct PlotParams<'w, 's> {
    capture_entities: Query<'w, 's, Entity, With<CaptureComponent>>,
    contexts: EguiContexts<'w, 's>,
    data_collection: Res<'w, DataCollection>,
    csv_export: ResMut<'w, CsvExportState>,
    working_directory: Res<'w, WorkingDirectory>,
    selected_project: Res<'w, SelectedProject>,
    selection: Res<'w, OrbitalSelectionState>,
    world_time: Option<Res<'w, WorldTime>>,
    app_settings: Res<'w, Settings>,
    log: MessageWriter<'w, LogEvent>,
}

pub fn egui_plots(p: PlotParams) {
    let PlotParams {
        capture_entities,
        mut contexts,
        data_collection,
        mut csv_export,
        working_directory,
        selected_project,
        selection,
        world_time,
        app_settings,
        mut log,
    } = p;
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
    let mut outcome = None;

    let ctx = contexts.ctx_mut().unwrap();
    let available_rect = ctx.available_rect();
    egui::Window::new("Data Collection")
        .default_pos(Pos2::new(0.0, 1920.0))
        .constrain_to(available_rect)
        .show(ctx, |ui| {
            plots_ui(
                ui,
                &PlotsView {
                    position: pos_data,
                    velocity: vel_data,
                    units: app_settings.unit_system,
                    session: settings.plot_session,
                },
            );
            export_error_ui(ui, settings.export_error.as_deref());

            if settings.selecting_csv_dir {
                export_clicked = true;
                ui.label(format!(
                    "{}",
                    Path::new(&working_directory.path).join("exports").display()
                ));
                ui.text_edit_singleline(&mut selected_filename);
                if ui.button("Export").clicked() {
                    export_clicked = false;
                    let written =
                        export_csv(&working_directory, &selected_filename, pos_data, vel_data);
                    if let Ok(path) = &written {
                        num_exports += 1;
                        info!(
                            path = %path.display(),
                            rows = pos_data.len().min(vel_data.len()),
                            exports_completed = num_exports,
                            "Completed egui data collection CSV export"
                        );
                    }
                    outcome = Some(("CSV", written.map(|_| ())));
                }
            } else {
                ui.horizontal(|ui| {
                    let csv_clicked = ui.button("Export to CSV").clicked();
                    let oem_clicked = ui.button("Export OEM").clicked();
                    if !(csv_clicked || oem_clicked) {
                        return;
                    }
                    let Some(project_id) = selected_project.project_id.clone() else {
                        warn!("No project selected, when one should always be when in Sim state");
                        return;
                    };
                    // `<plan-id>-<stamp>`, sanitised (F-33's CSV part).
                    let stem = format!(
                        "{}-{}",
                        sanitize_file_stem(&project_id),
                        chrono::Local::now().format("%d-%m-%Y_%H-%M-%S")
                    );
                    if csv_clicked {
                        selected_filename = format!("{stem}.csv");
                        export_clicked = true;
                        info!(
                            project_id = %project_id,
                            filename = %selected_filename,
                            "Preparing egui data collection CSV export"
                        );
                    }
                    if oem_clicked {
                        outcome = Some((
                            "OEM",
                            export_oem(
                                &working_directory,
                                &format!("{stem}.oem"),
                                &selection,
                                world_time.as_deref(),
                                data_collection.ephemeris.get(&capture_entity),
                            ),
                        ));
                    }
                });
            }
        });

    csv_export.selecting_csv_dir = export_clicked;
    csv_export.csv_export_filename = selected_filename;
    csv_export.num_exports_completed = num_exports;
    if let Some((kind, outcome)) = outcome {
        record_export(&mut csv_export, &mut log, kind, outcome);
    }
}

/// Writes the plots' samples as CSV to `<project>/exports/<filename>`.
pub(crate) fn export_csv(
    working_directory: &WorkingDirectory,
    filename: &str,
    position: &[(f64, f64)],
    velocity: &[(f64, f64)],
) -> Result<PathBuf, String> {
    let path = exports_dir(working_directory)?.join(filename);
    write_capture_csv(&path, position, velocity)?;
    Ok(path)
}

/// Writes the capture's ephemeris samples as a CCSDS OEM file (target, then the
/// tether root as the chaser) to `<project>/exports/<filename>`.
pub(crate) fn export_oem(
    working_directory: &WorkingDirectory,
    filename: &str,
    selection: &OrbitalSelectionState,
    world_time: Option<&WorldTime>,
    samples: Option<&Vec<ssg_sim::telemetry::EphemerisSample>>,
) -> Result<(), String> {
    let (target, chaser) = (body_label(selection, false), body_label(selection, true));
    let samples = samples.map(Vec::as_slice).unwrap_or_default();
    let segment = |name, id, chaser: bool| OemObject {
        name,
        id,
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
                &[
                    segment(&target, body_object_id(selection, false), false),
                    segment(&chaser, body_object_id(selection, true), true),
                ],
            )
            .map(|()| path)
        });
    let path = written?;
    info!(path = %path.display(), states = samples.len(), "Completed OEM export");
    Ok(())
}
