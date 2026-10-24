use egui::{Key, ModifierNames, Modifiers};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Action {
    ToggleFavourite,
    Next,
    Previous,
    First,
    Last,
    NewSession,
    QuickSwitcher,
    OpenFolder,
}

impl Action {
    pub const ALL: [Self; 8] = [
        Self::ToggleFavourite,
        Self::Next,
        Self::Previous,
        Self::First,
        Self::Last,
        Self::NewSession,
        Self::QuickSwitcher,
        Self::OpenFolder,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ToggleFavourite => "Toggle favourite",
            Self::Next => "Next photo",
            Self::Previous => "Previous photo",
            Self::First => "First photo",
            Self::Last => "Last photo",
            Self::NewSession => "New session",
            Self::QuickSwitcher => "Switch session",
            Self::OpenFolder => "Open folder",
        }
    }
}

/// `command` = Cmd on macOS, Ctrl elsewhere (egui's `Modifiers::COMMAND`), so one stored binding works on every OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyCombo {
    pub command: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: Key,
}

impl KeyCombo {
    pub const fn plain(key: Key) -> Self {
        Self {
            command: false,
            alt: false,
            shift: false,
            key,
        }
    }

    pub const fn command(key: Key) -> Self {
        Self {
            command: true,
            ..Self::plain(key)
        }
    }

    pub const fn alt(key: Key) -> Self {
        Self {
            alt: true,
            ..Self::plain(key)
        }
    }
}

/// "⌘N" on macOS, "Ctrl+N" elsewhere.
impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let is_mac = cfg!(target_os = "macos");
        let names = if is_mac {
            ModifierNames::SYMBOLS
        } else {
            ModifierNames::NAMES
        };
        let modifiers = Modifiers {
            alt: self.alt,
            shift: self.shift,
            command: self.command,
            ..Modifiers::NONE
        };
        let modifiers = names.format(&modifiers, is_mac);
        // egui's own arrow symbols (⏴⏵) are missing from Inter; its punctuation symbols (",") read better than names.
        let key = match self.key {
            Key::ArrowLeft => "←",
            Key::ArrowRight => "→",
            Key::ArrowUp => "↑",
            Key::ArrowDown => "↓",
            key => key.symbol_or_name(),
        };
        if modifiers.is_empty() {
            f.write_str(key)
        } else {
            write!(f, "{modifiers}{}{key}", names.concat)
        }
    }
}

/// Several combos per action are allowed; the first one is shown in on-screen hints.
pub type Keybindings = BTreeMap<Action, Vec<KeyCombo>>;

pub fn default_keybindings() -> Keybindings {
    Action::ALL
        .into_iter()
        .map(|action| {
            let combo = match action {
                Action::ToggleFavourite => KeyCombo::plain(Key::Enter),
                Action::Next => KeyCombo::plain(Key::ArrowRight),
                Action::Previous => KeyCombo::plain(Key::ArrowLeft),
                Action::First => KeyCombo::plain(Key::Home),
                Action::Last => KeyCombo::plain(Key::End),
                Action::NewSession => KeyCombo::command(Key::N),
                Action::QuickSwitcher => KeyCombo::command(Key::K),
                Action::OpenFolder => KeyCombo::command(Key::O),
            };
            (action, vec![combo])
        })
        .collect()
}

/// Actions added in a later version get their defaults; actions a newer version wrote are dropped, not an error.
fn deserialize_keybindings_with_defaults<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Keybindings, D::Error> {
    let stored = BTreeMap::<String, Vec<KeyCombo>>::deserialize(deserializer)?;
    let mut keybindings = default_keybindings();
    for (action_name, combos) in stored {
        let action = Action::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(
            &action_name,
        ));
        match action {
            Ok(action) => {
                keybindings.insert(action, combos);
            }
            Err(_) => log::warn!("ignoring keybinding for unknown action {action_name:?}"),
        }
    }
    Ok(keybindings)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Parent of new session folders. Pointing it inside a Google Drive for desktop folder syncs everything.
    pub favourites_root: PathBuf,
    #[serde(
        default = "default_keybindings",
        deserialize_with = "deserialize_keybindings_with_defaults"
    )]
    pub keybindings: Keybindings,
    /// Favouriting copies the shot's RAW and `.xmp` files too. Some photographers only hand JPEGs to clients.
    #[serde(default = "copy_companions_by_default")]
    pub copy_companions: bool,
}

fn copy_companions_by_default() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        let pictures = directories::UserDirs::new()
            .and_then(|dirs| {
                dirs.picture_dir()
                    .map(PathBuf::from)
                    .or_else(|| Some(dirs.home_dir().join("Pictures")))
            })
            .unwrap_or_else(|| PathBuf::from("Pictures"));
        Self {
            favourites_root: pictures.join("Photo Sorter"),
            keybindings: default_keybindings(),
            copy_companions: copy_companions_by_default(),
        }
    }
}
