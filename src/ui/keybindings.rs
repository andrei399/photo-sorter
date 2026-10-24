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
    /// Uses the physical key when the platform reports it, so bindings survive keyboard layout switches.
    pub fn triggered_actions(&self, ctx: &egui::Context) -> Vec<Action> {
        if ctx.egui_wants_keyboard_input() {
            return Vec::new();
        }
        ctx.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    // Ctrl on macOS isn't the command modifier, and no binding can express it.
                    egui::Event::Key {
                        key,
                        physical_key,
                        pressed: true,
                        modifiers,
                        ..
                    } if !(modifiers.ctrl && !modifiers.command) => self
                        .action_by_combo
                        .get(&KeyCombo {
                            command: modifiers.command,
                            alt: modifiers.alt,
                            shift: modifiers.shift,
                            key: physical_key.unwrap_or(*key),
                        })
                        .copied(),
                    _ => None,
                })
                .collect()
        })
    }
}
