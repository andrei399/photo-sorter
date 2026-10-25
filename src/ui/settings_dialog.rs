//! Settings window, layout G2: left nav (Shortcuts, Storage, Cloud uploads), grouped shortcut rows with key chips.

use super::sessions::{primary_button, secondary_button};
use crate::app::{KeyCapture, PhotoSorterApp, SettingsDraft, SettingsPage};
use crate::core::Command;
use crate::models::settings::{Action, KeyCombo, default_keybindings};
use crate::processor::settings::{KeybindingProblem, find_keybinding_problem};
use crate::ui::keybindings::combo_of_key_event;
use crate::ui::palette::*;
use egui::{Align, Button, Color32, Frame, Key, Layout, Margin, RichText, ScrollArea, Stroke, vec2};

const WINDOW_SIZE: egui::Vec2 = vec2(880.0, 640.0);
const NAV_WIDTH: f32 = 200.0;
const FOOTER_HEIGHT: f32 = 64.0;
/// No servers, no telemetry: the policy says what the app does with photos and where they go.
const PRIVACY_POLICY: &str = "https://github.com/andrei399/photo-sorter/blob/main/PRIVACY.md";
const GROUPS: [&str; 3] = ["BROWSING", "FAVOURITES", "FILES"];

fn group_of(action: Action) -> &'static str {
    match action {
        Action::Next | Action::Previous | Action::First | Action::Last | Action::GoToPhoto => "BROWSING",
        Action::ToggleFavourite | Action::NewSession | Action::QuickSwitcher => "FAVOURITES",
        Action::OpenFolder => "FILES",
    }
}

/// Returns whether the dialog stays open.
pub(super) fn settings_dialog(ui: &mut egui::Ui, app: &mut PhotoSorterApp, draft: &mut SettingsDraft) -> bool {
    if let Some(capture) = draft.capture {
        capture_key(ui.ctx(), draft, capture);
    }
    let problem = find_keybinding_problem(&draft.settings.keybindings);
    let mut save = false;
    let mut cancel = false;
    let mut pick_root = false;

    let modal = egui::Modal::new(egui::Id::unique("settings_dialog"))
        .frame(
            Frame::NONE
                .fill(POPUP_FILL)
                .stroke(Stroke::new(1.0, POPUP_BORDER))
                .corner_radius(10),
        )
        .show(ui.ctx(), |ui| {
            ui.set_min_size(WINDOW_SIZE);
            ui.set_max_size(WINDOW_SIZE);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.horizontal_top(|ui| {
                nav(ui, draft, problem.is_some());
                ui.vertical(|ui| {
                    Frame::NONE.inner_margin(Margin::symmetric(28, 24)).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                        let body_size = vec2(WINDOW_SIZE.x - NAV_WIDTH - 56.0, WINDOW_SIZE.y - FOOTER_HEIGHT - 48.0);
                        ui.set_min_size(body_size);
                        ScrollArea::vertical().max_height(body_size.y).show(ui, |ui| {
                            ui.set_width(body_size.x);
                            match draft.page {
                                SettingsPage::Shortcuts => shortcuts_page(ui, draft, problem),
                                SettingsPage::Storage => pick_root = storage_page(ui, draft),
                                SettingsPage::Cloud => super::cloud::cloud_page(ui, app, draft),
                            }
                        });
                    });
                    footer(ui, draft, problem.is_none(), &mut save, &mut cancel);
                });
            });
        });

    if pick_root
        && let Some(folder) = rfd::FileDialog::new()
            .set_directory(&draft.settings.favourites_root)
            .pick_folder()
    {
        draft.settings.favourites_root = folder;
        draft.error = None;
    }
    if save {
        draft.saving = true;
        draft.error = None;
        app.send(Command::UpdateSettings(draft.settings.clone()));
    }
    !(cancel || modal.should_close())
}

/// Takes the next key press as a binding. Every key event is swallowed while capturing,
/// so Esc doesn't close the dialog and Enter doesn't press a focused button.
fn capture_key(ctx: &egui::Context, draft: &mut SettingsDraft, capture: KeyCapture) {
    let pressed = ctx.input_mut(|input| {
        let combo = input.events.iter().find_map(combo_of_key_event);
        input
            .events
            .retain(|event| !matches!(event, egui::Event::Key { .. } | egui::Event::Text(_)));
        combo
    });
    let Some(combo) = pressed else { return };
    let combos = draft.settings.keybindings.entry(capture.action).or_default();
    let removes = [KeyCombo::plain(Key::Delete), KeyCombo::plain(Key::Backspace)].contains(&combo);
    match capture.replacing {
        _ if combo == KeyCombo::plain(Key::Escape) => {}
        Some(index) if removes && index < combos.len() => {
            combos.remove(index);
        }
        Some(index) => {
            if let Some(replaced) = combos.get_mut(index) {
                *replaced = combo;
            }
        }
        None if !combos.contains(&combo) => combos.push(combo),
        None => {}
    }
    draft.capture = None;
    draft.error = None;
}

fn nav(ui: &mut egui::Ui, draft: &mut SettingsDraft, shortcuts_have_problem: bool) {
    Frame::NONE
        .fill(PANEL_FILL)
        .inner_margin(Margin::symmetric(12, 20))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(NAV_WIDTH - 24.0);
                ui.set_min_height(WINDOW_SIZE.y - 40.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Settings").font(semibold(16.0)).color(TEXT));
                });
                ui.add_space(14.0);
                for (page, label) in [
                    (SettingsPage::Shortcuts, "Shortcuts"),
                    (SettingsPage::Storage, "Storage"),
                    (SettingsPage::Cloud, "Cloud uploads"),
                ] {
                    let is_current = draft.page == page;
                    let item = ui.add(
                        Button::new(RichText::new(label).size(13.0).color(if is_current {
                            WARNING_TEXT
                        } else {
                            MUTED_TEXT
                        }))
                        .fill(if is_current {
                            ACTIVE_CARD_FILL
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(6)
                        .min_size(vec2(ui.available_width(), 36.0)),
                    );
                    if page == SettingsPage::Shortcuts && shortcuts_have_problem {
                        let badge =
                            egui::Rect::from_center_size(item.rect.right_center() - vec2(18.0, 0.0), vec2(18.0, 18.0));
                        ui.painter().circle_filled(badge.center(), 9.0, ERROR);
                        ui.painter().text(
                            badge.center(),
                            egui::Align2::CENTER_CENTER,
                            "1",
                            semibold(11.0),
                            Color32::from_rgb(0x1a, 0, 0),
                        );
                    }
                    if item.clicked() {
                        draft.page = page;
                        draft.capture = None;
                        draft.webdav_form = None;
                        draft.confirm_disconnect = None;
                    }
                }
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(format!("Photo Sorter v{} ·", env!("CARGO_PKG_VERSION")))
                                .size(12.0)
                                .color(MUTED_TEXT),
                        );
                        ui.hyperlink_to(RichText::new("Privacy").size(12.0).color(MUTED_TEXT), PRIVACY_POLICY);
                    });
                });
            })
        });
}

fn page_header(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.label(RichText::new(title).font(semibold(18.0)).color(TEXT));
    ui.label(RichText::new(description).size(13.0).color(MUTED_TEXT));
    ui.add_space(12.0);
}

fn shortcuts_page(ui: &mut egui::Ui, draft: &mut SettingsDraft, problem: Option<KeybindingProblem>) {
    page_header(
        ui,
        "Shortcuts",
        &format!(
            "Click a key to replace it, or + to add another. {} always opens Settings.",
            KeyCombo::command(Key::Comma)
        ),
    );
    let mut start_capture = None;
    for group in GROUPS {
        ui.label(RichText::new(group).font(semibold(11.0)).color(MUTED_TEXT));
        ui.add_space(4.0);
        for action in Action::ALL.into_iter().filter(|action| group_of(*action) == group) {
            let combos = draft.settings.keybindings.get(&action).cloned().unwrap_or_default();
            ui.horizontal(|ui| {
                ui.set_min_height(34.0);
                ui.label(RichText::new(action.label()).size(13.0).color(TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let capturing = |replacing| draft.capture == Some(KeyCapture { action, replacing });
                    if capturing(None) {
                        capture_chip(ui, false);
                    } else if add_button(ui, action).clicked() {
                        start_capture = Some(KeyCapture {
                            action,
                            replacing: None,
                        });
                    }
                    for (index, combo) in combos.iter().enumerate().rev() {
                        if capturing(Some(index)) {
                            capture_chip(ui, true);
                        } else if key_chip(
                            ui,
                            action,
                            *combo,
                            problem.and_then(KeybindingProblem::combo) == Some(*combo),
                        )
                        .clicked()
                        {
                            start_capture = Some(KeyCapture {
                                action,
                                replacing: Some(index),
                            });
                        }
                    }
                });
            });
            if let Some(problem) = problem.filter(|problem| problem.action() == action) {
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    ui.label(RichText::new(problem.to_string()).size(12.0).color(ERROR));
                });
            }
        }
        ui.add_space(14.0);
    }
    if start_capture.is_some() {
        draft.capture = start_capture;
    }
}

fn key_chip(ui: &mut egui::Ui, action: Action, combo: KeyCombo, has_problem: bool) -> egui::Response {
    let (fill, border, text) = if has_problem {
        (CONFLICT_FILL, ERROR, CONFLICT_TEXT)
    } else {
        (CHIP_FILL, POPUP_BORDER, TEXT)
    };
    let response = ui.add(
        Button::new(RichText::new(combo.to_string()).size(12.0).color(text))
            .fill(fill)
            .stroke(Stroke::new(1.0, border))
            .corner_radius(4)
            .min_size(vec2(0.0, 26.0)),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{}: {combo}", action.label())));
    response
}

fn add_button(ui: &mut egui::Ui, action: Action) -> egui::Response {
    let response = ui.add(
        Button::new(RichText::new("+").size(12.0).color(MUTED_TEXT))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, POPUP_BORDER))
            .corner_radius(4)
            .min_size(vec2(26.0, 26.0)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Add shortcut for {}", action.label()))
    });
    response.on_hover_text("Add another shortcut")
}

fn capture_chip(ui: &mut egui::Ui, replacing: bool) {
    let hint = if replacing {
        "Press a key…  Esc cancels · Delete removes"
    } else {
        "Press a key…  Esc cancels"
    };
    Frame::NONE
        .fill(ACTIVE_CARD_FILL)
        .stroke(Stroke::new(1.5, AMBER))
        .corner_radius(4)
        .inner_margin(Margin::symmetric(10, 5))
        .show(ui, |ui| ui.label(RichText::new(hint).size(12.0).color(WARNING_TEXT)));
}

/// Returns whether "Change…" was clicked.
fn storage_page(ui: &mut egui::Ui, draft: &mut SettingsDraft) -> bool {
    page_header(ui, "Storage", "Where favourites are copied, and what.");
    ui.label(
        RichText::new("New sessions are created in")
            .size(13.0)
            .color(MUTED_TEXT),
    );
    let mut change = false;
    ui.horizontal(|ui| {
        let change_width = 96.0;
        Frame::NONE
            .fill(INPUT_FILL)
            .stroke(Stroke::new(1.0, POPUP_BORDER))
            .corner_radius(6)
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width() - change_width - 32.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(draft.settings.favourites_root.display().to_string())
                            .size(13.0)
                            .color(TEXT),
                    )
                    .truncate(),
                );
            });
        ui.add_space(8.0);
        change = ui
            .add_sized(vec2(change_width, 38.0), secondary_button("Change…"))
            .clicked();
    });
    ui.add_space(4.0);

    let drive_root = draft.google_drive_root.clone().filter(|root| {
        !draft
            .settings
            .favourites_root
            .starts_with(root.parent().unwrap_or(root))
    });
    if let Some(drive_root) = drive_root {
        Frame::NONE
            .fill(WARNING_FILL)
            .stroke(Stroke::new(1.0, WARNING_BORDER))
            .corner_radius(6)
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Google Drive detected. Favourites saved there upload automatically.")
                            .size(13.0)
                            .color(WARNING_TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(primary_button("Use Drive")).clicked() {
                            draft.settings.favourites_root = drive_root;
                            draft.error = None;
                        }
                    });
                });
            });
    } else {
        ui.label(
            RichText::new("Tip: choose a folder inside Google Drive to upload favourites automatically.")
                .size(12.0)
                .color(MUTED_TEXT),
        );
    }
    ui.label(
        RichText::new("Existing sessions keep their folders.")
            .size(12.0)
            .color(MUTED_TEXT),
    );
    ui.add_space(16.0);
    ui.checkbox(
        &mut draft.settings.copy_companions,
        RichText::new("Include RAW and sidecar files when favouriting")
            .size(13.0)
            .color(TEXT),
    );
    change
}

fn footer(ui: &mut egui::Ui, draft: &mut SettingsDraft, keybindings_valid: bool, save: &mut bool, cancel: &mut bool) {
    let top = ui.cursor().top();
    ui.painter()
        .hline(ui.max_rect().x_range(), top, Stroke::new(1.0, PANEL_BORDER));
    Frame::NONE.inner_margin(Margin::symmetric(28, 14)).show(ui, |ui| {
        ui.set_width(WINDOW_SIZE.x - NAV_WIDTH - 56.0);
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.horizontal(|ui| {
            let reset = Button::new(RichText::new("Reset to defaults").size(13.0).color(MUTED_TEXT)).frame(false);
            if ui.add(reset).clicked() {
                draft.settings.keybindings = default_keybindings();
                draft.capture = None;
                draft.error = None;
            }
            if let Some(error) = &draft.error {
                ui.add_space(12.0);
                ui.add(egui::Label::new(RichText::new(error).size(12.0).color(ERROR)).truncate());
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let can_save = keybindings_valid && !draft.saving && draft.capture.is_none();
                *save |= ui.add_enabled(can_save, primary_button("Save")).clicked();
                *cancel |= ui.add(secondary_button("Cancel")).clicked();
            });
        });
    });
}
