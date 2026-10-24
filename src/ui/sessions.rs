//! Session switcher (dropdown of session cards), quick switcher, New Session dialog, forget confirmation,
//! and the dispatch of every modal (one at a time).

use crate::app::{Modal, NewSessionDraft, PhotoSorterApp, QuickSwitcherState};
use crate::core::Command;
use crate::models::session::SessionId;
use crate::models::settings::{Action, KeyCombo};
use crate::processor::sessions::{SessionSummary, session_name_to_folder_name};
use crate::ui::palette::*;
use egui::text::LayoutJob;
use egui::{
    Align, Button, FontId, Frame, Key, Layout, Margin, Modifiers, RichText, Sense, Stroke, TextEdit, TextFormat, vec2,
};
use std::path::PathBuf;

enum MenuChoice {
    Activate(SessionId),
    NewSession,
    Forget(SessionId),
}

/// The session button in the bottom panel; its menu opens upward with one card per session.
pub fn session_button(ui: &mut egui::Ui, app: &mut PhotoSorterApp) {
    let label = match app.sessions.active_summary() {
        Some(session) => format!("{} · {} ★", session.name, session.favourite_count),
        None => "Create a session".to_owned(),
    };
    let button = ui.add(
        Button::new(RichText::new(label).size(13.0).color(TEXT))
            .fill(CARD_FILL)
            .stroke(Stroke::new(1.0, POPUP_BORDER))
            .corner_radius(6)
            .min_size(vec2(0.0, 30.0)),
    );
    let choice = egui::Popup::from_toggle_button_response(&button)
        .align(egui::RectAlign::TOP_START)
        .gap(6.0)
        .width(360.0)
        .frame(
            Frame::popup(ui.style())
                .fill(POPUP_FILL)
                .stroke(Stroke::new(1.0, POPUP_BORDER))
                .corner_radius(8)
                .inner_margin(8),
        )
        .show(|ui| {
            session_menu(
                ui,
                &app.sessions.sessions,
                app.sessions.active,
                &app.key_map.hint(Action::NewSession),
            )
        })
        .and_then(|response| response.inner);

    match choice {
        Some(MenuChoice::Activate(id)) => app.send(Command::SetActiveSession(id)),
        Some(MenuChoice::NewSession) => app.open_new_session_dialog(String::new()),
        Some(MenuChoice::Forget(id)) => app.modal = Some(Modal::ForgetSession(id)),
        None => {}
    }
}

fn session_menu(
    ui: &mut egui::Ui,
    sessions: &[SessionSummary],
    active: Option<SessionId>,
    new_session_hint: &str,
) -> Option<MenuChoice> {
    let mut choice = None;
    ui.label(RichText::new("SESSIONS").font(semibold(11.0)).color(MUTED_TEXT));
    ui.add_space(4.0);
    if sessions.is_empty() {
        ui.label(RichText::new("No sessions yet. Each event you shoot gets its own folder.").color(MUTED_TEXT));
    }
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        for session in sessions {
            if session_card(ui, session, Some(session.id) == active).clicked() {
                choice = Some(MenuChoice::Activate(session.id));
            }
            ui.add_space(6.0);
        }
    });
    ui.add_space(2.0);
    let new_session = Button::new(
        RichText::new(format!("+ New session ({new_session_hint})"))
            .font(semibold(13.0))
            .color(AMBER_INK),
    )
    .fill(AMBER)
    .corner_radius(6);
    if ui.add_sized(vec2(ui.available_width(), 36.0), new_session).clicked() {
        choice = Some(MenuChoice::NewSession);
    }
    if let Some(active) = active {
        let forget = Button::new(RichText::new("Forget this session… (files are kept)").color(MUTED_TEXT)).frame(false);
        if ui.add(forget).clicked() {
            choice = Some(MenuChoice::Forget(active));
        }
    }
    choice
}

fn session_card(ui: &mut egui::Ui, session: &SessionSummary, is_active: bool) -> egui::Response {
    let (fill, border) = if is_active {
        (ACTIVE_CARD_FILL, AMBER)
    } else {
        (CARD_FILL, CARD_BORDER)
    };
    let card = Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(8)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(&session.name).font(semibold(14.0)).color(TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let count_color = if is_active { AMBER } else { MUTED_TEXT };
                    ui.label(RichText::new(format!("{} ★", session.favourite_count)).color(count_color));
                });
            });
            ui.label(
                RichText::new(session.dir.display().to_string())
                    .size(12.0)
                    .color(MUTED_TEXT),
            );
            if is_active {
                ui.label(RichText::new("Active").size(12.0).color(AMBER));
            }
        });
    ui.interact(
        card.response.rect,
        ui.scope_id().with(("session_card", session.id.0)),
        Sense::click(),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn draw_modal(ui: &mut egui::Ui, app: &mut PhotoSorterApp) {
    match app.modal.take() {
        Some(Modal::NewSession(mut draft)) => {
            let keep_open = new_session_dialog(ui, app, &mut draft);
            if keep_open && app.modal.is_none() {
                app.modal = Some(Modal::NewSession(draft));
            } else if !keep_open {
                app.sessions.favourite_after_create = None;
            }
        }
        Some(Modal::QuickSwitcher(mut state)) => {
            if quick_switcher(ui, app, &mut state) && app.modal.is_none() {
                app.modal = Some(Modal::QuickSwitcher(state));
            }
        }
        Some(Modal::ForgetSession(id)) if forget_confirmation(ui, app, id) && app.modal.is_none() => {
            app.modal = Some(Modal::ForgetSession(id));
        }
        Some(Modal::Settings(mut draft)) => {
            if super::settings_dialog::settings_dialog(ui, app, &mut draft) && app.modal.is_none() {
                app.modal = Some(Modal::Settings(draft));
            }
        }
        Some(Modal::ForgetSession(_)) | None => {}
    }
}

fn dialog_frame() -> Frame {
    Frame::NONE
        .fill(POPUP_FILL)
        .stroke(Stroke::new(1.0, POPUP_BORDER))
        .corner_radius(10)
        .inner_margin(24)
}

pub(super) fn primary_button(text: &str) -> Button<'_> {
    Button::new(RichText::new(text).font(semibold(13.0)).color(AMBER_INK))
        .fill(AMBER)
        .corner_radius(6)
        .min_size(vec2(0.0, 36.0))
}

pub(super) fn secondary_button(text: &str) -> Button<'_> {
    Button::new(RichText::new(text).color(TEXT))
        .fill(egui::Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, POPUP_BORDER))
        .corner_radius(6)
        .min_size(vec2(0.0, 36.0))
}

fn styled_text_edit(text: &mut String, focused: bool) -> TextEdit<'_> {
    TextEdit::singleline(text)
        .font(FontId::proportional(15.0))
        .margin(Margin::symmetric(12, 10))
        .background_color(INPUT_FILL)
        .desired_width(f32::INFINITY)
        .text_color(if focused { egui::Color32::WHITE } else { TEXT })
}

/// Layout E2. Returns whether the dialog stays open.
fn new_session_dialog(ui: &mut egui::Ui, app: &mut PhotoSorterApp, draft: &mut NewSessionDraft) -> bool {
    let favourites_root = app.favourites_root();
    let photo_to_favourite = app
        .sessions
        .favourite_after_create
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    let mut stay_open = true;
    let mut submit = false;
    let mut pick_folder = false;
    let mut folder_to_inspect = None;

    let modal = egui::Modal::new(egui::Id::unique("new_session_dialog"))
        .frame(dialog_frame())
        .show(ui.ctx(), |ui| {
            ui.set_width(512.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(RichText::new("New session").font(semibold(18.0)).color(TEXT));
            if let Some(file_name) = &photo_to_favourite {
                ui.label(
                    RichText::new(format!("{file_name} will be favourited into it right away.")).color(MUTED_TEXT),
                );
            }
            ui.add_space(8.0);

            ui.label(RichText::new("Event name").color(MUTED_TEXT));
            let name_field = ui.add(styled_text_edit(&mut draft.name, true).hint_text("e.g. Wedding Ana & Mihai"));
            if ui.memory(|memory| memory.focused().is_none()) {
                name_field.request_focus();
            }
            if name_field.changed() {
                draft.error = None;
                draft.follow_name(&favourites_root);
            }
            let name_problem = name_problem(&draft.name, &app.sessions.sessions);
            if let Some(problem) = name_problem
                .as_ref()
                .filter(|_| !draft.name.trim().is_empty())
                .or(draft.error.as_ref())
            {
                ui.label(RichText::new(problem).size(12.0).color(ERROR));
            }

            ui.add_space(4.0);
            ui.label(RichText::new("Folder").color(MUTED_TEXT));
            ui.horizontal(|ui| {
                let change = secondary_button("Change…");
                let change_width = 96.0;
                let folder_field = ui.add_sized(
                    vec2(ui.available_width() - change_width - 8.0, 40.0),
                    styled_text_edit(&mut draft.folder, false).font(FontId::proportional(13.0)),
                );
                if folder_field.changed() {
                    draft.folder_edited = true;
                }
                pick_folder = ui.add_sized(vec2(change_width, 40.0), change).clicked();
            });

            let folder = PathBuf::from(draft.folder.trim());
            match &app.inspected_session_dir {
                Some((dir, info)) if *dir == folder && info.file_count > 0 => {
                    Frame::NONE
                        .fill(WARNING_FILL)
                        .stroke(Stroke::new(1.0, WARNING_BORDER))
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(format!(
                        "This folder already has {} files. They stay untouched; favourites are added next to them.",
                        info.file_count
                    )).color(WARNING_TEXT));
                        });
                }
                Some((dir, _)) if *dir == folder => {}
                _ => folder_to_inspect = Some(folder.clone()),
            }
            if is_inside_google_drive(&draft.folder) {
                ui.label(
                    RichText::new("Inside Google Drive: favourites will upload automatically.")
                        .size(12.0)
                        .color(MUTED_TEXT),
                );
            }

            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let create_label = if photo_to_favourite.is_some() {
                    "Create & favourite   Enter"
                } else {
                    "Create   Enter"
                };
                let can_create =
                    name_problem.is_none() && draft.submitted_name.is_none() && !draft.folder.trim().is_empty();
                submit |= ui.add_enabled(can_create, primary_button(create_label)).clicked();
                stay_open &= !ui.add(secondary_button("Cancel   Esc")).clicked();
            });
            submit |= ui.input(|input| input.key_pressed(Key::Enter))
                && name_problem.is_none()
                && draft.submitted_name.is_none()
                && !draft.folder.trim().is_empty();
        });
    stay_open &= !modal.should_close();

    if let Some(folder) = folder_to_inspect {
        // Remember the request so it's sent once per folder, not every frame until the answer arrives.
        app.inspected_session_dir = Some((
            folder.clone(),
            crate::processor::sessions::SessionDirInfo {
                exists: false,
                file_count: 0,
            },
        ));
        app.send(Command::InspectSessionDir(folder));
    }
    if pick_folder && let Some(folder) = rfd::FileDialog::new().set_directory(&favourites_root).pick_folder() {
        draft.folder = folder.display().to_string();
        draft.folder_edited = true;
    }
    if submit && stay_open {
        draft.submitted_name = Some(draft.name.trim().to_owned());
        app.send(Command::CreateSession {
            name: draft.name.trim().to_owned(),
            dir: Some(PathBuf::from(draft.folder.trim())),
        });
    }
    stay_open
}

/// Same rules as the core (which stays authoritative), checked live for instant feedback.
fn name_problem(name: &str, sessions: &[SessionSummary]) -> Option<String> {
    if let Err(error) = session_name_to_folder_name(name) {
        return Some(error.to_string());
    }
    sessions
        .iter()
        .any(|session| session.name.to_lowercase() == name.trim().to_lowercase())
        .then(|| format!("A session named \"{}\" already exists", name.trim()))
}

fn is_inside_google_drive(folder: &str) -> bool {
    ["My Drive", "Google Drive", "GoogleDrive-"]
        .iter()
        .any(|marker| folder.contains(marker))
}

enum SwitcherItem<'a> {
    Session(&'a SessionSummary),
    Create(String),
}

/// Layout D3. Returns whether the switcher stays open.
fn quick_switcher(ui: &mut egui::Ui, app: &mut PhotoSorterApp, state: &mut QuickSwitcherState) -> bool {
    let query = state.query.trim().to_owned();
    let query_lower = query.to_lowercase();
    let mut items: Vec<SwitcherItem> = app
        .sessions
        .sessions
        .iter()
        .filter(|session| session.name.to_lowercase().contains(&query_lower))
        .map(SwitcherItem::Session)
        .collect();
    let exact_match = app
        .sessions
        .sessions
        .iter()
        .any(|session| session.name.to_lowercase() == query_lower);
    if !query.is_empty() && !exact_match {
        items.push(SwitcherItem::Create(query.clone()));
    }

    // Consumed before the text field sees them, so ↑/↓ move the selection instead of the cursor.
    let (up, down) = ui.input_mut(|input| {
        (
            input.consume_key(Modifiers::NONE, Key::ArrowUp),
            input.consume_key(Modifiers::NONE, Key::ArrowDown),
        )
    });
    if down {
        state.selected = (state.selected + 1).min(items.len().saturating_sub(1));
    }
    if up {
        state.selected = state.selected.saturating_sub(1);
    }
    state.selected = state.selected.min(items.len().saturating_sub(1));
    let create_now = ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter));
    let pick_selected = ui.input(|input| input.key_pressed(Key::Enter));

    let mut clicked: Option<usize> = None;
    let area = egui::Modal::default_area(egui::Id::unique("quick_switcher"))
        .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 140.0));
    let modal = egui::Modal::new(egui::Id::unique("quick_switcher"))
        .area(area)
        .frame(
            Frame::NONE
                .fill(POPUP_FILL)
                .stroke(Stroke::new(1.0, POPUP_BORDER))
                .corner_radius(10),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(520.0);
            Frame::NONE.inner_margin(Margin::symmetric(16, 10)).show(ui, |ui| {
                let search = ui.add(
                    TextEdit::singleline(&mut state.query)
                        .hint_text("Switch session…")
                        .font(FontId::proportional(15.0))
                        .frame(Frame::NONE)
                        .desired_width(f32::INFINITY),
                );
                search.request_focus();
                if search.changed() {
                    state.selected = 0;
                }
            });
            ui.separator();
            Frame::NONE.inner_margin(6).show(ui, |ui| {
                if items.is_empty() {
                    ui.label(RichText::new("No sessions yet: type a name to create one").color(MUTED_TEXT));
                }
                for (index, item) in items.iter().enumerate() {
                    let fill = if index == state.selected {
                        SELECTED_ROW_FILL
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let row = Frame::NONE
                        .fill(fill)
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(12, 12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| match item {
                                SwitcherItem::Session(session) => {
                                    ui.label(highlighted(
                                        &session.name,
                                        &query_lower,
                                        Some(session.id) == app.sessions.active,
                                    ));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(format!("{} ★", session.favourite_count))
                                                .size(12.0)
                                                .color(MUTED_TEXT),
                                        );
                                    });
                                }
                                SwitcherItem::Create(name) => {
                                    ui.label(
                                        RichText::new(format!("+ Create session “{name}”"))
                                            .size(14.0)
                                            .color(TEXT),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(KeyCombo::command(Key::Enter).to_string())
                                                .size(12.0)
                                                .color(MUTED_TEXT),
                                        );
                                    });
                                }
                            });
                        });
                    if ui
                        .interact(
                            row.response.rect,
                            ui.scope_id().with(("switcher_row", index)),
                            Sense::click(),
                        )
                        .clicked()
                    {
                        clicked = Some(index);
                    }
                }
            });
            ui.separator();
            Frame::NONE.inner_margin(Margin::symmetric(16, 8)).show(ui, |ui| {
                ui.label(
                    RichText::new(format!(
                        "↑↓ choose    Enter switch    {} new    Esc close",
                        app.key_map.hint(Action::NewSession)
                    ))
                    .size(12.0)
                    .color(MUTED_TEXT),
                );
            });
        });

    let chosen = clicked
        .or(pick_selected.then_some(state.selected))
        .and_then(|index| items.get(index));
    if create_now && !query.is_empty() && !exact_match {
        app.send(Command::CreateSession { name: query, dir: None });
        return false;
    }
    match chosen {
        Some(SwitcherItem::Session(session)) => {
            let id = session.id;
            app.send(Command::SetActiveSession(id));
            false
        }
        Some(SwitcherItem::Create(name)) => {
            let name = name.clone();
            app.send(Command::CreateSession { name, dir: None });
            false
        }
        None => !modal.should_close(),
    }
}

/// Session name with the typed query highlighted in amber.
fn highlighted(name: &str, query_lower: &str, is_active: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    let plain = TextFormat::simple(FontId::proportional(14.0), TEXT);
    let matched = TextFormat::simple(FontId::proportional(14.0), AMBER);
    match (!query_lower.is_empty())
        .then(|| name.to_lowercase().find(query_lower))
        .flatten()
    {
        // Lowercasing can change byte lengths for some scripts; only highlight when the slice lines up.
        Some(start) if name.is_char_boundary(start) && name.is_char_boundary(start + query_lower.len()) => {
            job.append(&name[..start], 0.0, plain.clone());
            job.append(&name[start..start + query_lower.len()], 0.0, matched);
            job.append(&name[start + query_lower.len()..], 0.0, plain.clone());
        }
        _ => job.append(name, 0.0, plain.clone()),
    }
    if is_active {
        job.append(
            "  (current)",
            0.0,
            TextFormat::simple(FontId::proportional(14.0), MUTED_TEXT),
        );
    }
    job
}

/// Returns whether the confirmation stays open.
fn forget_confirmation(ui: &mut egui::Ui, app: &mut PhotoSorterApp, id: SessionId) -> bool {
    let Some(session) = app.sessions.sessions.iter().find(|session| session.id == id).cloned() else {
        return false;
    };
    let mut stay_open = true;
    let modal = egui::Modal::new(egui::Id::unique("forget_session"))
        .frame(dialog_frame())
        .show(ui.ctx(), |ui| {
            ui.set_width(440.0);
            ui.label(
                RichText::new(format!("Forget “{}”?", session.name))
                    .font(semibold(18.0))
                    .color(TEXT),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "It disappears from Photo Sorter only. The {} photos in {} are kept.",
                    session.favourite_count,
                    session.dir.display()
                ))
                .color(MUTED_TEXT),
            );
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let forget = Button::new(RichText::new("Forget session").strong().color(egui::Color32::BLACK))
                    .fill(ERROR)
                    .corner_radius(6)
                    .min_size(vec2(0.0, 36.0));
                if ui.add(forget).clicked() {
                    app.send(Command::ForgetSession(id));
                    stay_open = false;
                }
                stay_open &= !ui.add(secondary_button("Cancel")).clicked();
            });
        });
    stay_open && !modal.should_close()
}
