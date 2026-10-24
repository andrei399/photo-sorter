use crate::core::{Command, CoreHandle, Event, spawn_core};
use crate::error::AppResult;
use crate::imaging::prefetch::ImageCache;
use crate::integrations::updater::{self, AvailableUpdate, UpdateInstall};
use crate::models::session::SessionId;
use crate::models::settings::{Action, Settings};
use crate::models::state::PersistedState;
use crate::processor::photo_folder::{Photo, PhotoFolder};
use crate::processor::sessions::{SessionDirInfo, SessionSummary};
use crate::ui::keybindings::KeyMap;
use crossbeam_channel::Receiver;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

/// The UI's view of sessions; ★ flips optimistically and the core confirms or reverts.
#[derive(Default)]
pub struct SessionsView {
    pub sessions: Vec<SessionSummary>,
    pub active: Option<SessionId>,
    /// Sources favourited in the active session.
    pub favourited: HashSet<PathBuf>,
    /// Toggles sent to the core and not yet confirmed.
    pub saving: HashSet<PathBuf>,
    /// Enter pressed with no session: favourite this photo once the new session exists.
    pub favourite_after_create: Option<PathBuf>,
    /// Last toggle the core rejected, so the ★ can show its failed state briefly.
    pub failed: Option<(PathBuf, Instant)>,
}

impl SessionsView {
    pub fn active_summary(&self) -> Option<&SessionSummary> {
        self.sessions.iter().find(|session| Some(session.id) == self.active)
    }
}

/// At most one modal at a time; viewer shortcuts are suspended while one is open.
pub enum Modal {
    NewSession(NewSessionDraft),
    QuickSwitcher(QuickSwitcherState),
    ForgetSession(SessionId),
    Settings(SettingsDraft),
}

/// Edits apply to this copy; nothing changes until Save is confirmed by the core.
pub struct SettingsDraft {
    pub settings: Settings,
    pub page: SettingsPage,
    pub capture: Option<KeyCapture>,
    /// From `DetectGoogleDrive`: the suggested favourites root inside Google Drive for desktop.
    pub google_drive_root: Option<PathBuf>,
    /// Sent to the core; the dialog closes when it confirms.
    pub saving: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    Shortcuts,
    Storage,
}

/// Waiting for the next key press to bind to `action`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct KeyCapture {
    pub action: Action,
    /// Index of the combo being replaced; `None` adds a new one.
    pub replacing: Option<usize>,
}

pub struct NewSessionDraft {
    pub name: String,
    pub folder: String,
    /// Once the user edits or picks the folder, it stops following the name.
    pub folder_edited: bool,
    /// Name sent to the core; the dialog closes when that session becomes active.
    pub submitted_name: Option<String>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct QuickSwitcherState {
    pub query: String,
    pub selected: usize,
}

pub struct PhotoSorterApp {
    pub core: CoreHandle,
    pub state: Option<PersistedState>,
    pub photo_folder: Option<PhotoFolder>,
    /// Same order as `photo_folder.photos`; kept separately because the cache works on paths.
    photo_paths: Vec<PathBuf>,
    pub index: usize,
    pub image_cache: ImageCache,
    pub sessions: SessionsView,
    pub key_map: KeyMap,
    pub modal: Option<Modal>,
    /// Latest answer to `InspectSessionDir`, for the dialog's "folder already has N files" warning.
    pub inspected_session_dir: Option<(PathBuf, SessionDirInfo)>,
    /// When the user last tried to move past the first/last photo (drives the edge-bump feedback).
    pub edge_bumped_at: Option<Instant>,
    /// Shown while the current photo has nothing decoded yet, so navigation never flashes black.
    pub last_displayed_texture: Option<egui::TextureHandle>,
    /// A file dropped on the window: select it once its folder has been opened.
    pending_focus: Option<PathBuf>,
    pub error_message: Option<String>,
    /// A newer release, found and downloaded in the background; offered until the user restarts.
    pub available_update: Option<AvailableUpdate>,
    available_update_rx: Receiver<AvailableUpdate>,
}

impl PhotoSorterApp {
    pub fn new(egui_ctx: &egui::Context, state_path: PathBuf) -> AppResult<Self> {
        crate::ui::install_theme(egui_ctx);
        let repaint_ctx = egui_ctx.clone();
        let (available_update_tx, available_update_rx) = crossbeam_channel::bounded(1);
        let update_ctx = egui_ctx.clone();
        updater::spawn_background_update(state_path.with_file_name("update-checked"), move |update| {
            let _ = available_update_tx.send(update);
            update_ctx.request_repaint();
        });
        let core = spawn_core(state_path, move || repaint_ctx.request_repaint())?;
        Ok(Self {
            core,
            state: None,
            photo_folder: None,
            photo_paths: Vec::new(),
            index: 0,
            image_cache: ImageCache::new(),
            sessions: SessionsView::default(),
            key_map: KeyMap::default(),
            modal: None,
            inspected_session_dir: None,
            edge_bumped_at: None,
            last_displayed_texture: None,
            pending_focus: None,
            error_message: None,
            available_update: None,
            available_update_rx,
        })
    }

    pub fn current_photo(&self) -> Option<&Photo> {
        self.photo_folder.as_ref()?.photos.get(self.index)
    }

    pub fn send(&mut self, command: Command) {
        if self.core.commands.send(command).is_err() {
            self.error_message = Some("internal error: the app core stopped; please restart".into());
        }
    }

    pub fn perform(&mut self, action: Action) {
        let photo_count = self.photo_paths.len();
        match action {
            Action::Next if self.index + 1 < photo_count => self.index += 1,
            Action::Previous if self.index > 0 => self.index -= 1,
            Action::Next | Action::Previous => self.edge_bumped_at = Some(Instant::now()),
            Action::First => self.index = 0,
            Action::Last => self.index = photo_count.saturating_sub(1),
            Action::ToggleFavourite => self.toggle_current_favourite(),
            Action::NewSession => self.open_new_session_dialog(String::new()),
            Action::QuickSwitcher => self.modal = Some(Modal::QuickSwitcher(QuickSwitcherState::default())),
            Action::OpenFolder => {
                // Blocking is fine: the native dialog is modal.
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.send(Command::OpenFolder(folder));
                }
            }
        }
    }

    pub fn toggle_current_favourite(&mut self) {
        let Some(source) = self.current_photo().map(|photo| photo.path.clone()) else {
            return;
        };
        if self.sessions.active.is_none() {
            self.sessions.favourite_after_create = Some(source);
            self.open_new_session_dialog(String::new());
            return;
        }
        self.toggle_favourite_optimistically(source);
    }

    pub fn open_settings(&mut self) {
        let Some(state) = &self.state else { return };
        self.modal = Some(Modal::Settings(SettingsDraft {
            settings: state.settings.clone(),
            page: SettingsPage::Shortcuts,
            capture: None,
            google_drive_root: None,
            saving: false,
            error: None,
        }));
        self.send(Command::DetectGoogleDrive);
    }

    pub fn favourites_root(&self) -> PathBuf {
        self.state
            .as_ref()
            .map(|state| state.settings.favourites_root.clone())
            .unwrap_or_default()
    }

    pub fn open_new_session_dialog(&mut self, name: String) {
        let mut draft = NewSessionDraft {
            name,
            folder: String::new(),
            folder_edited: false,
            submitted_name: None,
            error: None,
        };
        draft.follow_name(&self.favourites_root());
        self.modal = Some(Modal::NewSession(draft));
    }

    /// Closing without creating also drops the "favourite after create" intent.
    pub fn close_modal(&mut self) {
        if matches!(self.modal, Some(Modal::NewSession(_))) {
            self.sessions.favourite_after_create = None;
        }
        self.modal = None;
    }

    /// The user clicked the update notice. Every change is already saved to disk, so closing loses nothing.
    pub fn restart_to_update(&mut self, ctx: &egui::Context) {
        let Some(update) = self.available_update.take() else {
            return;
        };
        match update.install {
            UpdateInstall::Ready(ready) => {
                let (pending, bytes) = *ready;
                match updater::install_and_relaunch(&pending, bytes) {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(error) => self.error_message = Some(error.to_string()),
                }
            }
            UpdateInstall::FromReleasePage => {
                if let Err(error) = opener::open_browser(updater::RELEASES_PAGE) {
                    self.error_message = Some(format!("Couldn't open {}: {error}", updater::RELEASES_PAGE));
                }
            }
        }
    }

    fn toggle_favourite_optimistically(&mut self, source: PathBuf) {
        flip_membership(&mut self.sessions.favourited, &source);
        self.sessions.saving.insert(source.clone());
        self.send(crate::core::Command::ToggleFavourite(source));
    }

    /// A dropped folder opens directly; a dropped file opens its folder and jumps to it.
    fn open_dropped_path(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| input.raw.dropped_files.first().map(|file| file.path().to_path_buf()));
        let Some(path) = dropped else { return };
        if path.is_dir() {
            self.send(Command::OpenFolder(path));
        } else if let Some(folder) = path.parent() {
            self.send(Command::OpenFolder(folder.to_path_buf()));
            self.pending_focus = Some(path);
        }
    }

    fn apply(&mut self, event: Event) {
        match event {
            Event::Snapshot(state) => {
                self.key_map = KeyMap::from_settings(&state.settings);
                self.state = Some(state);
            }
            Event::SettingsChanged(settings) => {
                self.key_map = KeyMap::from_settings(&settings);
                if let Some(state) = &mut self.state {
                    state.settings = settings;
                }
                if matches!(&self.modal, Some(Modal::Settings(draft)) if draft.saving) {
                    self.modal = None;
                }
            }
            Event::SettingsUpdateFailed(error) => match &mut self.modal {
                Some(Modal::Settings(draft)) => {
                    draft.saving = false;
                    draft.error = Some(error.to_string());
                }
                _ => self.error_message = Some(error.to_string()),
            },
            Event::GoogleDriveDetected(root) => {
                if let Some(Modal::Settings(draft)) = &mut self.modal {
                    draft.google_drive_root = root;
                }
            }
            Event::FolderOpened(photo_folder) => {
                self.photo_paths = photo_folder.photos.iter().map(|photo| photo.path.clone()).collect();
                self.index = self
                    .pending_focus
                    .take()
                    .and_then(|focus| self.photo_paths.iter().position(|path| *path == focus))
                    .unwrap_or(0);
                self.photo_folder = Some(photo_folder);
                self.last_displayed_texture = None;
                self.error_message = None;
            }
            Event::SessionsChanged { sessions, active } => {
                self.sessions.sessions = sessions;
                self.sessions.active = active;
                let created = |draft: &NewSessionDraft| {
                    draft.submitted_name.as_deref().is_some_and(|name| {
                        self.sessions
                            .active_summary()
                            .is_some_and(|session| session.name == name.trim())
                    })
                };
                if let Some(Modal::NewSession(draft)) = &self.modal
                    && created(draft)
                {
                    self.modal = None;
                }
            }
            Event::SessionCreateFailed(error) => match &mut self.modal {
                Some(Modal::NewSession(draft)) => {
                    draft.submitted_name = None;
                    draft.error = Some(error.to_string());
                }
                _ => self.error_message = Some(error.to_string()),
            },
            Event::FavouritesOfActive { session_id, sources } => {
                self.sessions.favourited = sources;
                // Arrives right after a session is created, so the optimistic ★ isn't wiped by this reset.
                if session_id.is_some()
                    && let Some(source) = self.sessions.favourite_after_create.take()
                {
                    self.toggle_favourite_optimistically(source);
                }
            }
            Event::SessionDirInspected { dir, info } => self.inspected_session_dir = Some((dir, info)),
            Event::FavouriteToggled { source, favourited } => {
                self.sessions.saving.remove(&source);
                if favourited {
                    self.sessions.favourited.insert(source);
                } else {
                    self.sessions.favourited.remove(&source);
                }
            }
            Event::FavouriteFailed { source, error } => {
                self.sessions.saving.remove(&source);
                self.sessions.failed = Some((source.clone(), Instant::now()));
                flip_membership(&mut self.sessions.favourited, &source);
                self.error_message = Some(error.to_string());
            }
            Event::Error(error) => self.error_message = Some(error.to_string()),
        }
    }
}

impl eframe::App for PhotoSorterApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.core.events.try_recv() {
            self.apply(event);
        }
        if let Ok(update) = self.available_update_rx.try_recv() {
            self.available_update = Some(update);
        }
        if self.modal.is_none() {
            // Not rebindable, so a bad binding can always be undone.
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)) {
                self.open_settings();
            }
            for action in self.key_map.triggered_actions(ctx) {
                self.perform(action);
            }
        }
        self.open_dropped_path(ctx);
        // Decode at physical pixels so HiDPI screens get sharp photos.
        let viewport_px = (ctx.content_rect().size() * ctx.pixels_per_point()).round();
        self.image_cache.want(
            ctx,
            &self.photo_paths,
            self.index,
            [viewport_px.x as u32, viewport_px.y as u32],
        );
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::ui::draw(ui, self);
    }
}

impl NewSessionDraft {
    /// Default folder: `<favourites root>/<name made filesystem-safe>`, until the user picks their own.
    pub fn follow_name(&mut self, favourites_root: &std::path::Path) {
        if !self.folder_edited {
            let folder_name = crate::processor::sessions::session_name_to_folder_name(&self.name).unwrap_or_default();
            self.folder = favourites_root.join(folder_name).display().to_string();
        }
    }
}

fn flip_membership(set: &mut HashSet<PathBuf>, path: &PathBuf) {
    if !set.remove(path) {
        set.insert(path.clone());
    }
}

/// `state.json` in the OS data dir (e.g. ~/.local/share/photosorter on Linux).
pub fn default_state_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("com", "Nodus Veritatis", "Photo Sorter")
        .map(|dirs| dirs.data_dir().join("state.json"))
}
