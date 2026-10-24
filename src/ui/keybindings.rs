use crate::models::settings::{Action, KeyCombo, Settings};
use std::collections::{BTreeMap, HashMap};

/// Rebuilt only when settings change, so matching a key press is one hash lookup.
pub struct KeyMap {
    action_by_combo: HashMap<KeyCombo, Action>,
    /// First combo of each action, shown in on-screen hints.
    hint_combo_by_action: BTreeMap<Action, KeyCombo>,
}

impl Default for KeyMap {
    fn default() -> Self {
        Self::from_settings(&Settings::default())
    }
}

impl KeyMap {
    pub fn from_settings(settings: &Settings) -> Self {
        let bindings = || settings.keybindings.iter();
        Self {
            action_by_combo: bindings()
                .flat_map(|(action, combos)| combos.iter().map(|combo| (*combo, *action)))
                .collect(),
            hint_combo_by_action: bindings()
                .filter_map(|(action, combos)| Some((*action, *combos.first()?)))
                .collect(),
        }
    }

    pub fn hint(&self, action: Action) -> String {
        self.hint_combo_by_action
            .get(&action)
            .map(ToString::to_string)
            .unwrap_or_default()
    }

    /// Actions triggered this frame, including key auto-repeat (held arrows browse continuously).
    pub fn triggered_actions(&self, ctx: &egui::Context) -> Vec<Action> {
        if ctx.egui_wants_keyboard_input() {
            return Vec::new();
        }
        ctx.input(|input| {
            input
                .events
                .iter()
                .filter_map(combo_of_key_event)
                .filter_map(|combo| self.action_by_combo.get(&combo).copied())
                .collect()
        })
    }
}

/// The binding a key press stands for, using the physical key when the platform reports it.
/// Ctrl on macOS isn't the command modifier and no binding can express it, so it yields nothing.
pub fn combo_of_key_event(event: &egui::Event) -> Option<KeyCombo> {
    match event {
        egui::Event::Key {
            key,
            physical_key,
            pressed: true,
            modifiers,
            ..
        } if !(modifiers.ctrl && !modifiers.command) => Some(KeyCombo {
            command: modifiers.command,
            alt: modifiers.alt,
            shift: modifiers.shift,
            key: physical_key.unwrap_or(*key),
        }),
        _ => None,
    }
}
