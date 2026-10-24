//! Look from the picked mocks (layout C, switcher D1+D2, dialog E2, ★ states F): Inter on near-black, amber accent.

use egui::epaint::text::{FontInsert, InsertFontFamily};
use egui::{Color32, FontData, FontFamily, FontId, FontPriority};

const SEMIBOLD_FAMILY: &str = "semibold";

/// Inter also covers the ★ ☆ ⌘ ↑ ↓ glyphs the UI uses (egui's bundled font doesn't) and Romanian ș/ț.
pub fn install_theme(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.add_font(FontInsert {
        name: "Inter-Regular".into(),
        data: FontData::from_static(include_bytes!("../../assets/fonts/Inter-Regular.ttf")),
        families: vec![InsertFontFamily {
            family: FontFamily::Proportional,
            priority: FontPriority::Highest,
        }],
    });
    ctx.add_font(FontInsert {
        name: "Inter-SemiBold".into(),
        data: FontData::from_static(include_bytes!("../../assets/fonts/Inter-SemiBold.ttf")),
        families: vec![InsertFontFamily {
            family: FontFamily::Name(SEMIBOLD_FAMILY.into()),
            priority: FontPriority::Highest,
        }],
    });
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD_FAMILY.into()))
}

pub const AMBER: Color32 = Color32::from_rgb(0xf5, 0xb7, 0x00);
pub const AMBER_INK: Color32 = Color32::from_rgb(0x1a, 0x14, 0x00);
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0x8a, 0x80);
pub const TEXT: Color32 = Color32::from_rgb(0xec, 0xec, 0xec);
pub const MUTED_TEXT: Color32 = Color32::from_rgb(0xa8, 0xa8, 0xa8);
pub const PANEL_FILL: Color32 = Color32::from_rgb(0x12, 0x12, 0x12);
pub const PANEL_BORDER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2a);
pub const POPUP_FILL: Color32 = Color32::from_rgb(0x1b, 0x1b, 0x1b);
pub const POPUP_BORDER: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);
pub const CARD_FILL: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1c);
pub const CARD_BORDER: Color32 = Color32::from_rgb(0x2f, 0x2f, 0x2f);
pub const ACTIVE_CARD_FILL: Color32 = Color32::from_rgb(0x22, 0x1d, 0x0d);
pub const INPUT_FILL: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
pub const SELECTED_ROW_FILL: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x2b);
pub const WARNING_FILL: Color32 = Color32::from_rgb(0x2a, 0x22, 0x10);
pub const WARNING_BORDER: Color32 = Color32::from_rgb(0x6b, 0x54, 0x10);
pub const WARNING_TEXT: Color32 = Color32::from_rgb(0xf3, 0xe2, 0xb0);
