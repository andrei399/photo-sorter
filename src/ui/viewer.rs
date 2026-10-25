//! Layout C: photo on black, a bottom panel with controls, a ±5 filmstrip and a whole-folder scrub bar.

use crate::app::PhotoSorterApp;
use crate::imaging::raw_preview::is_raw_file;
use crate::models::settings::{Action, KeyCombo};
use crate::processor::photo_folder::{Photo, PhotoFolder};
use crate::ui::palette::*;
use crate::ui::sessions;
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextEdit,
    UiBuilder, Vec2, pos2, vec2,
};
use std::path::Path;
use std::time::Duration;

const PANEL_HEIGHT: f32 = 154.0;
const FAILED_STAR_VISIBLE: Duration = Duration::from_millis(2500);
const THUMB_SIZE: Vec2 = vec2(96.0, 64.0);
const THUMB_GAP: f32 = 8.0;
const THUMBS_EACH_SIDE: usize = 5;
const EDGE_BUMP: Duration = Duration::from_millis(150);
const SCRUB_HEIGHT: f32 = 16.0;
const SCRUB_KNOB_RADIUS: f32 = 6.0;
const GO_TO_FIELD_WIDTH: f32 = 56.0;

pub fn draw(ui: &mut egui::Ui, app: &mut PhotoSorterApp, photo_folder: &PhotoFolder) {
    let mut jump_to: Option<usize> = None;

    egui::Panel::bottom("filmstrip")
        .exact_size(PANEL_HEIGHT)
        .frame(
            egui::Frame::NONE
                .fill(PANEL_FILL)
                .inner_margin(egui::Margin::symmetric(16, 8)),
        )
        .show_separator_line(false)
        .show(ui, |ui| {
            let top = ui.max_rect().expand2(vec2(16.0, 8.0)).top();
            ui.painter().hline(
                ui.max_rect().x_range().expand(16.0),
                top,
                Stroke::new(1.0, PANEL_BORDER),
            );
            controls_row(ui, app, photo_folder);
            ui.add_space(8.0);
            let clicked_thumbnail = filmstrip(ui, app, photo_folder);
            ui.add_space(6.0);
            jump_to = scrub_bar(ui, app, photo_folder).or(clicked_thumbnail);
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::BLACK))
        .show(ui, |ui| {
            if let Some(index) = photo_area(ui, app, photo_folder) {
                jump_to = Some(index);
            }
        });

    if let Some(index) = jump_to {
        app.index = index;
    }
}

fn controls_row(ui: &mut egui::Ui, app: &mut PhotoSorterApp, photo_folder: &PhotoFolder) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    let side_width = (row.width() * 0.36).min(440.0);
    let left = Rect::from_min_size(row.min, vec2(side_width, row.height()));
    let right = Rect::from_min_max(pos2(row.right() - side_width, row.top()), row.max);
    let center = Rect::from_min_max(pos2(left.right(), row.top()), pos2(right.left(), row.bottom()));

    ui.scope_builder(
        UiBuilder::new()
            .max_rect(left)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            sessions::session_button(ui, app);
            if let Some(session) = app.sessions.active_summary()
                && folder_icon_button(ui)
                    .on_hover_text("Open the session folder")
                    .clicked()
                && let Err(error) = opener::open(&session.dir)
            {
                app.error_message = Some(format!("Couldn't open {}: {error}", session.dir.display()));
            }
        },
    );

    let mut label = egui::text::LayoutJob::default();
    let muted = egui::TextFormat::simple(FontId::proportional(13.0), MUTED_TEXT);
    match photo_folder.photos.get(app.index) {
        Some(photo) => {
            label.append(
                &format!(
                    "{} · {} / {}",
                    photo.file_name,
                    app.index + 1,
                    photo_folder.photos.len()
                ),
                0.0,
                muted.clone(),
            );
            if let Some(captured_at) = app
                .image_cache
                .entry(&photo.path)
                .and_then(|entry| entry.captured_at.as_deref())
            {
                label.append(&format!(" · {captured_at}"), 0.0, muted);
            }
            if let Some(badge) = file_types_badge(photo) {
                let chip = egui::TextFormat {
                    background: CHIP_FILL,
                    ..egui::TextFormat::simple(FontId::proportional(11.0), TEXT)
                };
                label.append(&format!(" {badge} "), 8.0, chip);
            }
        }
        None => label.append("No photos in this folder", 0.0, muted),
    }
    let current_file_name = photo_folder.photos.get(app.index).map(|photo| photo.file_name.as_str());
    match current_file_name {
        Some(file_name) if app.go_to_number_draft.is_some() => {
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(center)
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| go_to_number_field(ui, app, file_name, photo_folder.photos.len()),
            );
        }
        _ => {
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(center)
                    .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
                |ui| {
                    let go_to_hint = format!("Go to photo number ({})", app.key_map.hint(Action::GoToPhoto));
                    if ui
                        .label(label)
                        .interact(Sense::click())
                        .on_hover_cursor(egui::CursorIcon::Text)
                        .on_hover_text(go_to_hint)
                        .clicked()
                    {
                        app.perform(Action::GoToPhoto);
                    }
                },
            );
        }
    }

    let current = photo_folder.photos.get(app.index).map(|photo| photo.path.clone());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(right)
            .layout(Layout::right_to_left(Align::Center)),
        |ui| {
            if let Some(source) = current
                && favourite_button(ui, app, &source).clicked()
            {
                app.toggle_current_favourite();
            }
            ui.add_space(8.0);
            let settings_hint = format!("Settings ({})", KeyCombo::command(egui::Key::Comma));
            if settings_icon_button(ui).on_hover_text(settings_hint).clicked() {
                app.open_settings();
            }
            ui.add_space(8.0);
            super::cloud::upload_chip(ui, app);
        },
    );
}

/// Replaces the counter while jumping: `name · [____] / count`. Enter jumps, Esc or clicking away cancels.
fn go_to_number_field(ui: &mut egui::Ui, app: &mut PhotoSorterApp, file_name: &str, photo_count: usize) {
    let font = FontId::proportional(13.0);
    let prefix = format!("{file_name} ·");
    let suffix = format!("/ {photo_count}");
    let text_width = |text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), font.clone(), MUTED_TEXT)
            .size()
            .x
    };
    let row_width = text_width(&prefix) + GO_TO_FIELD_WIDTH + text_width(&suffix) + 2.0 * ui.spacing().item_spacing.x;
    ui.add_space(((ui.available_width() - row_width) / 2.0).max(0.0));
    ui.label(RichText::new(prefix).font(font.clone()).color(MUTED_TEXT));
    let draft = app.go_to_number_draft.get_or_insert_default();
    let response = ui.add(
        TextEdit::singleline(draft)
            .desired_width(GO_TO_FIELD_WIDTH)
            .font(font.clone())
            .align(Align2::RIGHT_CENTER),
    );
    draft.retain(|c| c.is_ascii_digit());
    response.widget_info(|| egui::WidgetInfo::text_edit(true, &*draft, &*draft, "Go to photo number"));
    ui.label(RichText::new(suffix).font(font).color(MUTED_TEXT));

    if response.lost_focus() {
        let typed_number = app
            .go_to_number_draft
            .take()
            .and_then(|draft| draft.parse::<usize>().ok());
        if let Some(number) = typed_number
            && ui.input(|input| input.key_pressed(egui::Key::Enter))
        {
            app.index = number.clamp(1, photo_count) - 1;
        }
    } else if !response.has_focus() {
        response.request_focus();
    }
}

/// `JPG+RAW+XMP`: what favouriting this shot will copy. Plain single JPEGs get no badge.
fn file_types_badge(photo: &Photo) -> Option<String> {
    if photo.companions.is_empty() && !is_raw_file(&photo.path) {
        return None;
    }
    let mut labels: Vec<String> = Vec::new();
    for file in std::iter::once(&photo.path).chain(&photo.companions) {
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or_default();
        let label = match extension.to_ascii_uppercase().as_str() {
            _ if is_raw_file(file) => "RAW".to_owned(),
            "JPEG" => "JPG".to_owned(),
            other => other.to_owned(),
        };
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    Some(labels.join("+"))
}

enum StarState {
    Off,
    On,
    Saving,
    Failed,
}

/// The ★ states from mock F: off (outline), on (filled), saving (filled + spinner), failed (red, reverted).
fn favourite_button(ui: &mut egui::Ui, app: &PhotoSorterApp, source: &Path) -> egui::Response {
    let recently_failed = app
        .sessions
        .failed
        .as_ref()
        .filter(|(path, at)| path == source && at.elapsed() < FAILED_STAR_VISIBLE);
    let state = if app.sessions.saving.contains(source) {
        StarState::Saving
    } else if let Some((_, failed_at)) = recently_failed {
        ui.ctx()
            .request_repaint_after(FAILED_STAR_VISIBLE.saturating_sub(failed_at.elapsed()));
        StarState::Failed
    } else if app.sessions.favourited.contains(source) {
        StarState::On
    } else {
        StarState::Off
    };
    let (fill, border, ink, label) = match state {
        StarState::Off => (Color32::TRANSPARENT, AMBER, AMBER, "☆ Favourite"),
        StarState::On => (AMBER, AMBER, AMBER_INK, "★ Favourited"),
        StarState::Saving => (AMBER, AMBER, AMBER_INK, "Saving…"),
        StarState::Failed => (Color32::TRANSPARENT, ERROR, ERROR, "☆ Favourite"),
    };

    let (rect, response) = ui.allocate_exact_size(vec2(168.0, 30.0), Sense::click());
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(6),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    let mut text_left = rect.left() + 12.0;
    if matches!(state, StarState::Saving) {
        let spinner_rect = Rect::from_center_size(pos2(text_left + 6.0, rect.center().y), vec2(12.0, 12.0));
        egui::Spinner::new().size(12.0).color(ink).paint_at(ui, spinner_rect);
        text_left += 18.0;
    }
    painter.text(
        pos2(text_left, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        ink,
    );
    let chip = painter.layout_no_wrap(
        app.key_map.hint(Action::ToggleFavourite),
        FontId::proportional(11.0),
        if matches!(state, StarState::On | StarState::Saving) {
            AMBER_INK
        } else {
            TEXT
        },
    );
    let chip_rect = Rect::from_min_size(
        pos2(rect.right() - 10.0 - chip.size().x - 12.0, rect.center().y - 9.0),
        vec2(chip.size().x + 12.0, 18.0),
    );
    let chip_fill = if matches!(state, StarState::On | StarState::Saving) {
        Color32::from_black_alpha(38)
    } else {
        Color32::from_gray(0x2a)
    };
    painter.rect_filled(chip_rect, CornerRadius::same(4), chip_fill);
    painter.galley(chip_rect.center() - chip.size() / 2.0, chip, TEXT);

    let accessible_label = if matches!(state, StarState::On) {
        "Favourited"
    } else {
        "Favourite"
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, accessible_label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A 30 px borderless button that paints its own icon (Inter lacks folder and gear glyphs).
fn icon_button(
    ui: &mut egui::Ui,
    accessible_label: &str,
    paint_icon: impl FnOnce(&egui::Painter, Rect),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), Color32::from_gray(0x24));
    }
    paint_icon(ui.painter(), rect);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, accessible_label));
    response
}

fn folder_icon_button(ui: &mut egui::Ui) -> egui::Response {
    icon_button(ui, "Open session folder", |painter, rect| {
        let icon = Rect::from_center_size(rect.center(), vec2(16.0, 12.0));
        let outline = vec![
            icon.left_top(),
            icon.left_top() + vec2(6.0, 0.0),
            icon.left_top() + vec2(8.0, 2.0),
            icon.right_top() + vec2(0.0, 2.0),
            icon.right_bottom(),
            icon.left_bottom(),
            icon.left_top(),
        ];
        painter.add(egui::Shape::line(outline, Stroke::new(1.5, TEXT)));
    })
}

fn settings_icon_button(ui: &mut egui::Ui) -> egui::Response {
    icon_button(ui, "Settings", |painter, rect| {
        let center = rect.center();
        painter.circle_stroke(center, 4.0, Stroke::new(1.5, TEXT));
        for tooth in 0..8 {
            let direction = egui::Vec2::angled(tooth as f32 * std::f32::consts::FRAC_PI_4);
            painter.line_segment(
                [center + direction * 5.5, center + direction * 8.0],
                Stroke::new(2.0, TEXT),
            );
        }
    })
}

/// Returns the index of a clicked thumbnail.
fn filmstrip(ui: &mut egui::Ui, app: &PhotoSorterApp, photo_folder: &PhotoFolder) -> Option<usize> {
    let photo_count = photo_folder.photos.len();
    if photo_count == 0 {
        return None;
    }
    let first = app.index.saturating_sub(THUMBS_EACH_SIDE);
    let last = (app.index + THUMBS_EACH_SIDE).min(photo_count - 1);
    // Anchor the current thumbnail at the centre, so the strip slides instead of re-centring a shorter row.
    let row_width = ui.available_width();
    let center_x = ui.min_rect().left() + row_width / 2.0;
    let (strip, _) = ui.allocate_exact_size(vec2(row_width, THUMB_SIZE.y), Sense::hover());

    let mut clicked = None;
    for index in first..=last {
        let offset = index as f32 - app.index as f32;
        let thumb_center = pos2(center_x + offset * (THUMB_SIZE.x + THUMB_GAP), strip.center().y);
        let thumb_rect = Rect::from_center_size(thumb_center, THUMB_SIZE);
        let photo = &photo_folder.photos[index];
        let response = ui
            .interact(thumb_rect, ui.scope_id().with(("thumb", index)), Sense::click())
            .on_hover_text(&photo.file_name);
        let painter = ui.painter();
        painter.rect_filled(thumb_rect, CornerRadius::same(4), Color32::from_gray(0x24));
        if let Some(texture) = app.image_cache.best_texture(&photo.path) {
            painter.image(
                texture.id(),
                fit_inside(texture.size_vec2(), thumb_rect),
                uv_full(),
                Color32::WHITE,
            );
        }
        if app.sessions.favourited.contains(&photo.path) {
            painter.text(
                thumb_rect.right_top() + vec2(-4.0, 2.0),
                Align2::RIGHT_TOP,
                "★",
                FontId::proportional(13.0),
                AMBER,
            );
        }
        if index == app.index {
            painter.rect_stroke(
                thumb_rect.expand(2.0),
                CornerRadius::same(5),
                Stroke::new(2.0, Color32::WHITE),
                StrokeKind::Outside,
            );
        }
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

/// The whole folder as one track: click or drag to jump, amber ticks show where the favourites are.
fn scrub_bar(ui: &mut egui::Ui, app: &PhotoSorterApp, photo_folder: &PhotoFolder) -> Option<usize> {
    let photo_count = photo_folder.photos.len();
    if photo_count == 0 {
        return None;
    }
    let (bar, response) = ui.allocate_exact_size(vec2(ui.available_width(), SCRUB_HEIGHT), Sense::click_and_drag());
    let track = bar.shrink2(vec2(SCRUB_KNOB_RADIUS, 0.0));
    let last_index = photo_count - 1;
    let x_of = |index: usize| track.left() + track.width() * index as f32 / last_index.max(1) as f32;
    let index_at = |x: f32| {
        let fraction = ((x - track.left()) / track.width()).clamp(0.0, 1.0);
        (fraction * last_index as f32).round() as usize
    };

    let painter = ui.painter();
    let y = bar.center().y;
    painter.hline(track.x_range(), y, Stroke::new(2.0, PANEL_BORDER));
    painter.hline(
        track.left()..=x_of(app.index),
        y,
        Stroke::new(2.0, Color32::from_gray(0x6a)),
    );
    for (index, photo) in photo_folder.photos.iter().enumerate() {
        if app.sessions.favourited.contains(&photo.path) {
            painter.vline(x_of(index), (y - 4.0)..=(y + 4.0), Stroke::new(2.0, AMBER));
        }
    }
    let hovered = response.hover_pos().map(|pointer| index_at(pointer.x));
    if let Some(hovered) = hovered {
        painter.vline(x_of(hovered), (y - 5.0)..=(y + 5.0), Stroke::new(1.0, MUTED_TEXT));
    }
    painter.circle_filled(pos2(x_of(app.index), y), SCRUB_KNOB_RADIUS, TEXT);

    let target = response.interact_pointer_pos().map(|pointer| index_at(pointer.x));
    response.widget_info(|| egui::WidgetInfo::slider(true, (app.index + 1) as f64, "Photo position"));
    if let Some(hovered) = hovered {
        response.on_hover_text_at_pointer(format!("{} · {}", hovered + 1, photo_folder.photos[hovered].file_name));
    }
    target
}

/// Paints the current photo; left/right thirds are click zones. Returns a navigation target.
fn photo_area(ui: &mut egui::Ui, app: &mut PhotoSorterApp, photo_folder: &PhotoFolder) -> Option<usize> {
    let area = ui.max_rect();
    let Some(photo) = photo_folder.photos.get(app.index) else {
        ui.painter().text(
            area.center(),
            Align2::CENTER_CENTER,
            "No photos in this folder",
            FontId::proportional(16.0),
            MUTED_TEXT,
        );
        return None;
    };

    let bump_offset = edge_bump_offset(ui.ctx(), app);
    let failure = app.image_cache.entry(&photo.path).and_then(|entry| match &entry.full {
        crate::imaging::prefetch::Slot::Failed(message) => Some(message.clone()),
        _ => None,
    });
    // Never blank: keep the previous photo on screen until the new one has something to show.
    if let Some(texture) = app.image_cache.best_texture(&photo.path) {
        app.last_displayed_texture = Some(texture.clone());
    }
    match (&failure, &app.last_displayed_texture) {
        (Some(message), _) if app.image_cache.best_texture(&photo.path).is_none() => {
            let text = format!("Can't display {}\n{message}", photo.file_name);
            ui.painter().text(
                area.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(15.0),
                MUTED_TEXT,
            );
        }
        (_, Some(texture)) => {
            let rect = fit_inside(texture.size_vec2(), area).translate(vec2(bump_offset, 0.0));
            ui.painter().image(texture.id(), rect, uv_full(), Color32::WHITE);
        }
        (_, None) => {}
    }

    let third = area.width() / 3.0;
    let left_zone = Rect::from_min_size(area.min, vec2(third, area.height()));
    let right_zone = Rect::from_min_size(pos2(area.right() - third, area.top()), vec2(third, area.height()));
    let photo_count = photo_folder.photos.len();
    if ui
        .interact(left_zone, ui.scope_id().with("previous_zone"), Sense::click())
        .clicked()
    {
        return Some(app.index.saturating_sub(1));
    }
    if ui
        .interact(right_zone, ui.scope_id().with("next_zone"), Sense::click())
        .clicked()
    {
        return Some((app.index + 1).min(photo_count - 1));
    }
    None
}

fn edge_bump_offset(ctx: &egui::Context, app: &PhotoSorterApp) -> f32 {
    let Some(bumped_at) = app.edge_bumped_at else {
        return 0.0;
    };
    let elapsed = bumped_at.elapsed();
    if elapsed >= EDGE_BUMP {
        return 0.0;
    }
    ctx.request_repaint();
    let progress = elapsed.as_secs_f32() / EDGE_BUMP.as_secs_f32();
    (progress * std::f32::consts::TAU * 3.0).sin() * 8.0 * (1.0 - progress)
}

/// Aspect-fit `content_size` inside `bounds`, centred.
fn fit_inside(content_size: Vec2, bounds: Rect) -> Rect {
    let scale = (bounds.width() / content_size.x).min(bounds.height() / content_size.y);
    Rect::from_center_size(bounds.center(), content_size * scale)
}

fn uv_full() -> Rect {
    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0))
}
