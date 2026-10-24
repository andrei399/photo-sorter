//! Layout C: photo on black, a bottom panel with controls and a ±5 filmstrip.

use crate::app::PhotoSorterApp;
use crate::models::settings::{Action, KeyCombo};
use crate::processor::photo_folder::PhotoFolder;
use crate::ui::palette::*;
use crate::ui::sessions;
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, UiBuilder, Vec2, pos2,
    vec2,
};
use std::path::Path;
use std::time::Duration;

const PANEL_HEIGHT: f32 = 132.0;
const FAILED_STAR_VISIBLE: Duration = Duration::from_millis(2500);
const THUMB_SIZE: Vec2 = vec2(96.0, 64.0);
const THUMB_GAP: f32 = 8.0;
const THUMBS_EACH_SIDE: usize = 5;
const EDGE_BUMP: Duration = Duration::from_millis(150);

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
            jump_to = filmstrip(ui, app, photo_folder);
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

    let label = match photo_folder.photos.get(app.index) {
        Some(photo) => format!(
            "{} · {} / {}",
            photo.file_name,
            app.index + 1,
            photo_folder.photos.len()
        ),
        None => "No photos in this folder".to_owned(),
    };
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(center)
            .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
        |ui| {
            ui.label(egui::RichText::new(label).size(13.0).color(MUTED_TEXT));
        },
    );

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
        },
    );
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
