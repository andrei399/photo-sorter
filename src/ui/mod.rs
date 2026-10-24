use crate::app::PhotoSorterApp;
use crate::integrations::updater::UpdateInstall;
use crate::models::settings::Action;

pub mod keybindings;
mod palette;

pub use palette::install_theme;
mod sessions;
mod settings_dialog;
mod viewer;

pub fn draw(ui: &mut egui::Ui, app: &mut PhotoSorterApp) {
    // Taken for the frame so the viewer can read the folder while mutating the rest of the app.
    match app.photo_folder.take() {
        Some(photo_folder) => {
            viewer::draw(ui, app, &photo_folder);
            app.photo_folder = Some(photo_folder);
        }
        None => start_screen(ui, app),
    }
    sessions::draw_modal(ui, app);
    error_toast(ui, app);
    update_notice(ui, app);
}

fn start_screen(ui: &mut egui::Ui, app: &mut PhotoSorterApp) {
    egui::CentralPanel::default().show(ui, |ui| {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.label("Open a folder to start");
                if ui
                    .button("Open folder")
                    .on_hover_text(app.key_map.hint(Action::OpenFolder))
                    .clicked()
                {
                    app.perform(Action::OpenFolder);
                }
            });
        });
    });
}

fn error_toast(ui: &mut egui::Ui, app: &mut PhotoSorterApp) {
    let Some(message) = app.error_message.clone() else {
        return;
    };
    egui::Area::new(egui::Id::unique("error_toast"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(0xff, 0x8a, 0x80), message);
                    if ui.small_button("×").clicked() {
                        app.error_message = None;
                    }
                });
            });
        });
}

/// Non-blocking: the update waits, already downloaded, until the user picks a moment to restart.
fn update_notice(ui: &mut egui::Ui, app: &mut PhotoSorterApp) {
    let Some(update) = &app.available_update else {
        return;
    };
    let (message, action) = match update.install {
        UpdateInstall::Ready(_) => (format!("Update to v{} available", update.version), "Restart to update"),
        UpdateInstall::FromReleasePage => (format!("Photo Sorter v{} is out", update.version), "Download"),
    };
    let mut act = false;
    let mut dismiss = false;
    egui::Area::new(egui::Id::unique("update_notice"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 16.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(message);
                    act = ui.add(sessions::primary_button(action)).clicked();
                    dismiss = ui.small_button("×").on_hover_text("Remind me next launch").clicked();
                });
            });
        });
    if act {
        app.restart_to_update(ui.ctx());
    } else if dismiss {
        app.available_update = None;
    }
}
