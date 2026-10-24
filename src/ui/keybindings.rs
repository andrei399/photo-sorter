//! Single place mapping keys to actions; phase 04 swaps the hard-coded table for user settings.

use egui::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ToggleFavourite,
    NewSession,
    QuickSwitcher,
    Next,
    Previous,
    First,
    Last,
    OpenFolder,
}

const DEFAULT_BINDINGS: &[(Modifiers, Key, Action)] = &[
    (Modifiers::NONE, Key::Enter, Action::ToggleFavourite),
    (Modifiers::COMMAND, Key::N, Action::NewSession),
    (Modifiers::COMMAND, Key::K, Action::QuickSwitcher),
    (Modifiers::NONE, Key::ArrowRight, Action::Next),
    (Modifiers::NONE, Key::ArrowLeft, Action::Previous),
    (Modifiers::NONE, Key::Home, Action::First),
    (Modifiers::NONE, Key::End, Action::Last),
    (Modifiers::COMMAND, Key::O, Action::OpenFolder),
];

/// Actions triggered this frame, including key auto-repeat (held arrows browse continuously).
pub fn triggered_actions(ctx: &egui::Context) -> Vec<Action> {
    if ctx.egui_wants_keyboard_input() {
        return Vec::new();
    }
    ctx.input(|input| {
        input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    let key = physical_key.unwrap_or(*key);
                    DEFAULT_BINDINGS
                        .iter()
                        .find(|(bound_modifiers, bound_key, _)| {
                            *bound_key == key && modifiers.matches_exact(*bound_modifiers)
                        })
                        .map(|(_, _, action)| *action)
                }
                _ => None,
            })
            .collect()
    })
}
