//! Renders UI scenes to PNGs for eyeballing layout:
//! `PREVIEW_PHOTOS=<folder> PREVIEW_OUT=<dir> cargo test --test render_preview -- --ignored`
mod common;

use common::{AppHarness, app_harness};
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use photo_sorter::core::Command;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn settle(harness: &mut AppHarness, steps: usize) {
    for _ in 0..steps {
        harness.step();
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn save(harness: &mut AppHarness, out: &Path, name: &str) {
    harness.render().expect("render").save(out.join(name)).unwrap();
}

#[test]
#[ignore]
fn render_ui_scenes() {
    let photos = PathBuf::from(std::env::var("PREVIEW_PHOTOS").expect("PREVIEW_PHOTOS=<folder>"));
    let out = PathBuf::from(std::env::var("PREVIEW_OUT").expect("PREVIEW_OUT=<dir>"));
    let state_dir = tempfile::tempdir().unwrap();
    let mut harness = app_harness(state_dir.path());
    for name in ["Bucharest Marathon", "Client: Studio portraits", "Wedding Ana & Mihai"] {
        harness.state_mut().send(Command::CreateSession {
            name: name.into(),
            dir: None,
        });
    }
    harness.state_mut().send(Command::OpenFolder(photos));
    settle(&mut harness, 40);
    harness.key_press(Key::ArrowRight);
    harness.key_press(Key::Enter);
    settle(&mut harness, 40);
    save(&mut harness, &out, "1_viewer.png");

    harness.get_by_label_contains("Wedding Ana & Mihai ·").click();
    settle(&mut harness, 10);
    save(&mut harness, &out, "2_session_menu.png");
    harness.key_press(Key::Escape);
    settle(&mut harness, 5);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::N);
    settle(&mut harness, 5);
    harness
        .get_all_by_role(egui::Role::TextInput)
        .next()
        .unwrap()
        .type_text("Mihai's party");
    settle(&mut harness, 20);
    save(&mut harness, &out, "3_new_session.png");
    harness.key_press(Key::Escape);
    settle(&mut harness, 5);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::K);
    settle(&mut harness, 5);
    harness.get_by_role(egui::Role::TextInput).type_text("mar");
    settle(&mut harness, 10);
    save(&mut harness, &out, "4_quick_switcher.png");
}
