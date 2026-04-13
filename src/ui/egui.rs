use bevy::prelude::*;
use bevy_egui::{
    EguiContexts,
    egui::{self, Pos2},
};
use egui_plot::{Legend, Line, Plot, PlotPoints};

use crate::{
    components::capture_components::CaptureComponent,
    resources::{
        capture_plans::CapturePlanLibrary, data_collection::DataCollection,
        working_directory::WorkingDirectory,
    },
    ui::state::SelectedProject,
};

pub fn egui_plots(
    capture_entities: Query<Entity, With<CaptureComponent>>,
    mut contexts: EguiContexts,
    mut data_collection: ResMut<DataCollection>,
    working_directory: Res<WorkingDirectory>,
    selected_project: Res<SelectedProject>,
    capture_plan_lib: Res<CapturePlanLibrary>,
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

    let settings = &data_collection.settings;

    let mut export_clicked = false;
    let mut selected_filename: String = settings.csv_export_filename.clone();
    let mut num_exports = settings.num_exports_completed.clone();

    egui::Window::new("Data Collection")
        .default_pos(Pos2::new(0.0, 1920.0))
        .show(contexts.ctx_mut().unwrap(), |ui| {
            let pos_points: PlotPoints = pos_data
                .into_iter()
                .map(|(epoch, vel)| [*epoch, *vel])
                .collect();
            let vel_points: PlotPoints = vel_data
                .into_iter()
                .map(|(epoch, vel)| [*epoch, *vel])
                .collect();
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
                let export_dir = working_directory.path.clone() + "/";
                let export_filepath = export_dir.clone();
                ui.label(export_dir);
                ui.text_edit_singleline(&mut selected_filename);
                if ui.button("Export").clicked() {
                    export_clicked = false;
                    let export_path = export_filepath + &selected_filename;
                    info!(
                        path = %export_path,
                        rows = pos_data.len().min(vel_data.len()),
                        "Exporting egui data collection CSV"
                    );

                    let mut wtr = match csv::Writer::from_path(&export_path) {
                        Ok(writer) => writer,
                        Err(e) => {
                            error!(path = %export_path, error = %e, "Unable to create CSV writer");
                            return;
                        }
                    };

                    if let Err(e) =
                        wtr.serialize(("seconds_since_start", "rel_position", "rel_velocity"))
                    {
                        error!(path = %export_path, error = %e, "Failed to write CSV header row");
                        return;
                    }

                    for (pos_point, vel_point) in
                        Iterator::zip(pos_data.into_iter(), vel_data.into_iter())
                    {
                        if let Err(e) = wtr.serialize((pos_point.0, pos_point.1, vel_point.1)) {
                            error!(path = %export_path, error = %e, "Failed to write CSV data row");
                            return;
                        }
                    }

                    if let Err(e) = wtr.flush() {
                        error!(path = %export_path, error = %e, "Failed to flush CSV");
                        return;
                    }

                    num_exports += 1;
                    info!(
                        path = %export_path,
                        exports_completed = num_exports,
                        "Completed egui data collection CSV export"
                    );
                }
            } else if ui.button("Export to CSV").clicked() {
                let project_id = selected_project
                    .project_id
                    .clone()
                    .expect("No project selected, when one should always be when in Sim state");
                let Some(project) = capture_plan_lib.plans.get(&project_id) else {
                    warn!(
                        project_id = %project_id,
                        "Unable to prepare egui CSV export filename: selected project not found"
                    );
                    return;
                };
                selected_filename = format!(
                    "{}-{}.csv",
                    project.name,
                    chrono::Local::now().format("%d-%m-%Y_%H-%M-%S").to_string()
                );
                export_clicked = true;
                info!(
                    project_id = %project_id,
                    filename = %selected_filename,
                    "Preparing egui data collection CSV export"
                );
            }
        });

    data_collection.settings.selecting_csv_dir = export_clicked;
    data_collection.settings.csv_export_filename = selected_filename;
    data_collection.settings.num_exports_completed = num_exports;
}
