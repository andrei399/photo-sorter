//! Cloud uploads, mocks B + C1: provider cards on the Settings › Cloud uploads page, and an upload chip in the
//! bottom panel whose popover switches each connected service on or off.

use super::sessions::{primary_button, secondary_button, styled_text_edit};
use crate::app::{PhotoSorterApp, SettingsDraft, SettingsPage, WebDavForm};
use crate::core::Command;
use crate::integrations::cloud::CloudProblem;
use crate::models::cloud::{CloudProvider, WebDavServer};
use crate::processor::cloud_sync::CloudStatus;
use crate::ui::palette::*;
use egui::{
    Align, Align2, Button, Color32, CornerRadius, Frame, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, pos2, vec2,
};

const CARD_SIZE: egui::Vec2 = vec2(305.0, 160.0);
const CARD_GAP: f32 = 14.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    Quiet,
    Busy,
    Trouble,
}

impl Tone {
    fn text_color(self) -> Color32 {
        match self {
            Self::Quiet => MUTED_TEXT,
            Self::Busy => AMBER,
            Self::Trouble => CONFLICT_TEXT,
        }
    }

    /// Fill, border.
    fn surface(self) -> (Color32, Color32) {
        match self {
            Self::Quiet => (CARD_FILL, CARD_BORDER),
            Self::Busy => (ACTIVE_CARD_FILL, WARNING_BORDER),
            Self::Trouble => (CONFLICT_FILL, ERROR),
        }
    }
}

enum CloudChoice {
    SetUploads(CloudProvider, bool),
    Connect(CloudProvider),
    CancelConnect,
    AskDisconnect(CloudProvider),
    Disconnect(CloudProvider),
    KeepConnected,
    OpenSettings,
}

fn apply(app: &mut PhotoSorterApp, draft: Option<&mut SettingsDraft>, choice: CloudChoice) {
    match choice {
        CloudChoice::SetUploads(provider, enabled) => app.send(Command::SetCloudUploads { provider, enabled }),
        CloudChoice::Connect(provider) => match draft {
            // From inside Settings the form goes on this draft; `connect_cloud` would open a fresh dialog.
            Some(draft) if provider == CloudProvider::WebDav => {
                let server = app
                    .cloud_status(provider)
                    .and_then(|status| status.account.as_ref()?.webdav.clone());
                draft.webdav_form = Some(server.map_or_else(WebDavForm::default, |server| WebDavForm {
                    url: server.url,
                    username: server.username,
                    password: String::new(),
                }));
                app.cloud_connect = None;
            }
            _ => app.connect_cloud(provider),
        },
        // The browser sign-in keeps listening until it times out; a late "Allow" still connects.
        CloudChoice::CancelConnect => app.cloud_connect = None,
        CloudChoice::AskDisconnect(provider) => {
            if let Some(draft) = draft {
                draft.confirm_disconnect = Some(provider);
            }
        }
        CloudChoice::Disconnect(provider) => {
            if let Some(draft) = draft {
                draft.confirm_disconnect = None;
            }
            app.send(Command::DisconnectCloud(provider));
        }
        CloudChoice::KeepConnected => {
            if let Some(draft) = draft {
                draft.confirm_disconnect = None;
            }
        }
        CloudChoice::OpenSettings => app.open_settings_at(SettingsPage::Cloud),
    }
}

fn provider_state(status: &CloudStatus) -> (String, Tone) {
    let Some(account) = &status.account else {
        return ("Not connected".into(), Tone::Quiet);
    };
    if !account.enabled {
        return ("Off".into(), Tone::Quiet);
    }
    match &status.problem {
        Some(CloudProblem::NeedsReconnect) => (format!("Sign-in expired · {} waiting", status.pending), Tone::Trouble),
        Some(CloudProblem::Rejected(message)) => (message.clone(), Tone::Trouble),
        Some(CloudProblem::Retrying(_)) => (format!("Retrying · {} waiting", status.pending), Tone::Busy),
        None if status.pending > 0 => (format!("↑ {} left", status.pending), Tone::Busy),
        None => ("Up to date".into(), Tone::Quiet),
    }
}

/// `None` hides the chip: nothing is connected, so there's nothing to report.
fn chip_summary(statuses: &[CloudStatus]) -> Option<(String, Tone)> {
    if statuses.iter().all(|status| status.account.is_none()) {
        return None;
    }
    let enabled: Vec<&CloudStatus> = statuses
        .iter()
        .filter(|status| status.account.as_ref().is_some_and(|account| account.enabled))
        .collect();
    let pending: usize = enabled.iter().map(|status| status.pending).sum();
    let has =
        |wanted: fn(&CloudProblem) -> bool| enabled.iter().any(|status| status.problem.as_ref().is_some_and(wanted));
    Some(if enabled.is_empty() {
        ("Uploads off".into(), Tone::Quiet)
    } else if has(|problem| matches!(problem, CloudProblem::NeedsReconnect | CloudProblem::Rejected(_))) {
        ("Needs attention".into(), Tone::Trouble)
    } else if has(|problem| matches!(problem, CloudProblem::Retrying(_))) {
        (format!("{pending} waiting"), Tone::Busy)
    } else if pending > 0 {
        (format!("{pending} left"), Tone::Busy)
    } else {
        ("Uploaded".into(), Tone::Quiet)
    })
}

/// The bottom panel's upload chip; its popover opens upward with one switch per connected service.
pub fn upload_chip(ui: &mut egui::Ui, app: &mut PhotoSorterApp) {
    let Some((summary, tone)) = chip_summary(&app.cloud_statuses) else {
        return;
    };
    let (fill, border) = match tone {
        Tone::Quiet => (CARD_FILL, POPUP_BORDER),
        Tone::Busy => (WARNING_FILL, WARNING_BORDER),
        Tone::Trouble => (CONFLICT_FILL, ERROR),
    };
    let ink = match tone {
        Tone::Quiet => MUTED_TEXT,
        Tone::Busy => WARNING_TEXT,
        Tone::Trouble => CONFLICT_TEXT,
    };
    let galley = ui.painter().layout_no_wrap(summary.clone(), semibold(12.0), ink);
    let (rect, chip) = ui.allocate_exact_size(vec2(galley.size().x + 44.0, 30.0), Sense::click());
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(15),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    paint_cloud(painter, pos2(rect.left() + 20.0, rect.center().y), ink);
    painter.galley(
        pos2(rect.left() + 34.0, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    chip.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Cloud uploads: {summary}")));
    let chip = chip.on_hover_cursor(egui::CursorIcon::PointingHand);

    let statuses = app.cloud_statuses.clone();
    let choice = egui::Popup::from_toggle_button_response(&chip)
        .align(egui::RectAlign::TOP_END)
        .gap(6.0)
        .width(340.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            Frame::popup(ui.style())
                .fill(POPUP_FILL)
                .stroke(Stroke::new(1.0, POPUP_BORDER))
                .corner_radius(10)
                .inner_margin(8),
        )
        .show(|ui| upload_menu(ui, &statuses))
        .and_then(|response| response.inner);
    if let Some(choice) = choice {
        if matches!(choice, CloudChoice::Connect(_) | CloudChoice::OpenSettings) {
            egui::Popup::close_all(ui.ctx());
        }
        apply(app, None, choice);
    }
}

fn upload_menu(ui: &mut egui::Ui, statuses: &[CloudStatus]) -> Option<CloudChoice> {
    let mut choice = None;
    ui.spacing_mut().item_spacing.y = 2.0;
    for status in statuses {
        let Some(account) = &status.account else {
            continue;
        };
        let (state, tone) = provider_state(status);
        let row_fill = if tone == Tone::Trouble {
            CONFLICT_FILL
        } else {
            Color32::TRANSPARENT
        };
        Frame::NONE
            .fill(row_fill)
            .corner_radius(6)
            .inner_margin(Margin::symmetric(8, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    provider_tile(ui, status.provider, 28.0);
                    ui.add_space(4.0);
                    ui.label(RichText::new(status.provider.label()).size(13.0).color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if upload_switch(ui, account.enabled, status.provider).clicked() {
                            choice = Some(CloudChoice::SetUploads(status.provider, !account.enabled));
                        }
                        if status.problem == Some(CloudProblem::NeedsReconnect) {
                            let reconnect =
                                Button::new(RichText::new("Reconnect").font(semibold(12.0)).color(ERROR)).frame(false);
                            if ui.add(reconnect).clicked() {
                                choice = Some(CloudChoice::Connect(status.provider));
                            }
                        } else {
                            let label = ui.add(
                                egui::Label::new(RichText::new(&state).size(12.0).color(tone.text_color())).truncate(),
                            );
                            if let Some(CloudProblem::Retrying(reason) | CloudProblem::Rejected(reason)) =
                                &status.problem
                            {
                                label.on_hover_text(reason);
                            }
                        }
                    });
                });
            });
    }
    ui.add_space(4.0);
    ui.separator();
    let manage = Button::new(RichText::new("Manage accounts in Settings").size(12.0).color(AMBER)).frame(false);
    if ui.add(manage).clicked() {
        choice = Some(CloudChoice::OpenSettings);
    }
    choice
}

/// The Settings page. Changes apply at once, like the chip's switches; Save and Cancel only cover the other pages.
pub(super) fn cloud_page(ui: &mut egui::Ui, app: &mut PhotoSorterApp, draft: &mut SettingsDraft) {
    if draft.webdav_form.is_some() {
        webdav_form(ui, app, draft);
        return;
    }
    let enabled_count = app
        .cloud_statuses
        .iter()
        .filter(|status| status.account.as_ref().is_some_and(|account| account.enabled))
        .count();
    let pending: usize = app.cloud_statuses.iter().map(|status| status.pending).sum();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new("Cloud uploads").font(semibold(18.0)).color(TEXT));
            ui.label(
                RichText::new("Favourites upload to every service that's on, all at once.")
                    .size(13.0)
                    .color(MUTED_TEXT),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
            let summary = match pending {
                0 => format!("{enabled_count} of {} on", CloudProvider::ALL.len()),
                _ => format!("↑ {pending} left · {enabled_count} of {} on", CloudProvider::ALL.len()),
            };
            let color = if pending > 0 { AMBER } else { MUTED_TEXT };
            ui.label(RichText::new(summary).font(semibold(13.0)).color(color));
        });
    });
    ui.add_space(14.0);

    let mut choice = None;
    let statuses = app.cloud_statuses.clone();
    for pair in statuses.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = CARD_GAP;
            for status in pair {
                choice = provider_card(ui, app, draft, status).or(choice.take());
            }
        });
        ui.add_space(CARD_GAP);
    }
    ui.label(
        RichText::new(
            "Sign-ins stay in your system keychain. Files go straight from this computer to each service; \
             switching one on uploads the favourites of every session.",
        )
        .size(12.0)
        .color(MUTED_TEXT),
    );
    if let Some(choice) = choice {
        apply(app, Some(draft), choice);
    }
}

fn destination_hint(provider: CloudProvider) -> &'static str {
    match provider {
        CloudProvider::GoogleDrive => "Uploads into My Drive/Photo Sorter; sees only its own files.",
        CloudProvider::OneDrive | CloudProvider::Dropbox => "Uploads into Apps/Photo Sorter only.",
        CloudProvider::WebDav => "Your own server or NAS, into a Photo Sorter folder.",
    }
}

fn provider_card(
    ui: &mut egui::Ui,
    app: &PhotoSorterApp,
    draft: &SettingsDraft,
    status: &CloudStatus,
) -> Option<CloudChoice> {
    let mut choice = None;
    let (state, tone) = provider_state(status);
    let attempt = app
        .cloud_connect
        .as_ref()
        .filter(|attempt| attempt.provider == status.provider);
    let card_tone = match (&status.account, tone) {
        (Some(account), Tone::Busy) if account.enabled => Tone::Busy,
        (_, Tone::Trouble) => Tone::Trouble,
        _ => Tone::Quiet,
    };
    let (fill, border) = card_tone.surface();
    let content_size = CARD_SIZE - vec2(32.0, 32.0);
    ui.allocate_ui_with_layout(CARD_SIZE, Layout::top_down(Align::Min), |ui| {
        Frame::NONE
            .fill(fill)
            .stroke(Stroke::new(1.0, border))
            .corner_radius(10)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.set_min_size(content_size);
                ui.set_max_size(content_size);
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    provider_tile(ui, status.provider, 36.0);
                    ui.add_space(4.0);
                    ui.label(RichText::new(status.provider.label()).font(semibold(14.0)).color(TEXT));
                    if let Some(account) = &status.account {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if upload_switch(ui, account.enabled, status.provider).clicked() {
                                choice = Some(CloudChoice::SetUploads(status.provider, !account.enabled));
                            }
                        });
                    }
                });
                let subtitle = status
                    .account
                    .as_ref()
                    .map_or(destination_hint(status.provider), |account| {
                        account.account_label.as_str()
                    });
                ui.add(egui::Label::new(RichText::new(subtitle).size(12.0).color(MUTED_TEXT)).truncate());
                if status.account.is_some() {
                    ui.add(egui::Label::new(RichText::new(&state).size(12.0).color(tone.text_color())).truncate());
                }
                if let Some(error) = attempt.and_then(|attempt| attempt.error.as_deref()) {
                    ui.add(egui::Label::new(RichText::new(error).size(12.0).color(ERROR)).truncate());
                }

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        let waiting = attempt.is_some_and(|attempt| attempt.error.is_none());
                        let needs_sign_in =
                            status.account.is_none() || status.problem == Some(CloudProblem::NeedsReconnect);
                        if !status.can_connect && status.account.is_none() {
                            ui.label(
                                RichText::new("Not available in this build")
                                    .size(12.0)
                                    .color(MUTED_TEXT),
                            );
                        } else if waiting {
                            ui.add(egui::Spinner::new().size(14.0).color(AMBER));
                            ui.label(RichText::new("Waiting for the browser…").size(12.0).color(TEXT));
                            let cancel = Button::new(RichText::new("Cancel").size(12.0).color(MUTED_TEXT)).frame(false);
                            if ui.add(cancel).clicked() {
                                choice = Some(CloudChoice::CancelConnect);
                            }
                        } else if draft.confirm_disconnect == Some(status.provider) {
                            ui.label(RichText::new("Files stay uploaded.").size(12.0).color(MUTED_TEXT));
                            let disconnect =
                                Button::new(RichText::new("Disconnect").font(semibold(12.0)).color(ERROR)).frame(false);
                            if ui.add(disconnect).clicked() {
                                choice = Some(CloudChoice::Disconnect(status.provider));
                            }
                            let keep = Button::new(RichText::new("Keep").size(12.0).color(TEXT)).frame(false);
                            if ui.add(keep).clicked() {
                                choice = Some(CloudChoice::KeepConnected);
                            }
                        } else if needs_sign_in {
                            let label = if status.account.is_some() {
                                "Reconnect…"
                            } else {
                                "Connect…"
                            };
                            let button = ui.add_sized(vec2(ui.available_width(), 34.0), primary_button(label));
                            button.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    true,
                                    format!("{label} {}", status.provider.label()),
                                )
                            });
                            if button.clicked() {
                                choice = Some(CloudChoice::Connect(status.provider));
                            }
                        } else {
                            let disconnect =
                                Button::new(RichText::new("Disconnect").size(12.0).color(MUTED_TEXT)).frame(false);
                            let disconnect = ui.add(disconnect);
                            disconnect.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    true,
                                    format!("Disconnect {}", status.provider.label()),
                                )
                            });
                            if disconnect.clicked() {
                                choice = Some(CloudChoice::AskDisconnect(status.provider));
                            }
                        }
                    });
                });
            });
    });
    choice
}

fn webdav_form(ui: &mut egui::Ui, app: &mut PhotoSorterApp, draft: &mut SettingsDraft) {
    let Some(form) = &mut draft.webdav_form else {
        return;
    };
    let attempt = app
        .cloud_connect
        .as_ref()
        .filter(|attempt| attempt.provider == CloudProvider::WebDav);
    let checking = attempt.is_some_and(|attempt| attempt.error.is_none());
    let error = attempt.and_then(|attempt| attempt.error.clone());

    ui.label(
        RichText::new("Connect Nextcloud / WebDAV")
            .font(semibold(18.0))
            .color(TEXT),
    );
    ui.label(
        RichText::new("Your own server, a NAS or any WebDAV host. Favourites go into a \"Photo Sorter\" folder there.")
            .size(13.0)
            .color(MUTED_TEXT),
    );
    ui.add_space(12.0);
    let field = |ui: &mut egui::Ui, label: &str, text: &mut String, password: bool| {
        let label = ui.label(RichText::new(label).font(semibold(12.0)).color(MUTED_TEXT));
        ui.add(styled_text_edit(text, false).password(password))
            .labelled_by(label.id);
    };
    field(ui, "Server address", &mut form.url, false);
    ui.label(
        RichText::new("Nextcloud: Files › Settings › WebDAV shows this address.")
            .size(12.0)
            .color(MUTED_TEXT),
    );
    ui.add_space(6.0);
    field(ui, "Username", &mut form.username, false);
    ui.add_space(6.0);
    field(ui, "App password", &mut form.password, true);
    ui.add_space(10.0);
    if let Some(error) = &error {
        Frame::NONE
            .fill(CONFLICT_FILL)
            .corner_radius(6)
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(error).size(13.0).color(CONFLICT_TEXT));
            });
        ui.add_space(10.0);
    }

    let mut connect = false;
    let mut cancel = false;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let filled = !form.url.trim().is_empty() && !form.username.trim().is_empty() && !form.password.is_empty();
        let label = if checking { "Checking…" } else { "Connect" };
        connect = ui.add_enabled(filled && !checking, primary_button(label)).clicked();
        cancel = ui.add(secondary_button("Cancel")).clicked();
    });
    if connect {
        app.cloud_connect = Some(crate::app::CloudConnectAttempt {
            provider: CloudProvider::WebDav,
            error: None,
        });
        app.send(Command::ConnectWebDav {
            server: WebDavServer {
                url: form.url.trim().to_owned(),
                username: form.username.trim().to_owned(),
            },
            password: form.password.clone(),
        });
    } else if cancel {
        draft.webdav_form = None;
        app.cloud_connect = None;
    }
}

/// A 40×24 on/off switch; screen readers hear "Upload to <provider>", checked or not.
fn upload_switch(ui: &mut egui::Ui, on: bool, provider: CloudProvider) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(40.0, 24.0), Sense::click());
    let (track, track_border, knob, knob_x) = if on {
        (AMBER, AMBER, AMBER_INK, rect.right() - 12.0)
    } else {
        (CHIP_FILL, POPUP_BORDER, MUTED_TEXT, rect.left() + 12.0)
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(12),
        track,
        Stroke::new(1.0, track_border),
        StrokeKind::Inside,
    );
    painter.circle_filled(pos2(knob_x, rect.center().y), 8.0, knob);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Switch, true, on, format!("Upload to {}", provider.label()))
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A monogram, not the provider's logo: their logos are trademarks with usage rules.
fn provider_tile(ui: &mut egui::Ui, provider: CloudProvider, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same((size / 4.5) as u8), CHIP_FILL);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        &provider.label()[..1],
        semibold(size * 0.42),
        TEXT,
    );
}

/// Inter has no cloud glyph, so it's three bumps over a flat base.
fn paint_cloud(painter: &egui::Painter, center: Pos2, color: Color32) {
    painter.circle_filled(center + vec2(-4.0, 1.5), 3.5, color);
    painter.circle_filled(center + vec2(0.5, -1.0), 4.5, color);
    painter.circle_filled(center + vec2(4.5, 1.5), 3.5, color);
    painter.rect_filled(
        Rect::from_min_max(center + vec2(-4.0, 1.0), center + vec2(4.5, 5.0)),
        CornerRadius::ZERO,
        color,
    );
}
