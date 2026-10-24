//! Endpoint layer: the UI talks to the app only through `Command`s and hears back through `Event`s.
//! One thread owns the `Store`, so every state change is serialized without locks.

use crate::error::{AppError, AppResult};
use crate::models::session::SessionId;
use crate::models::settings::Settings;
use crate::models::state::PersistedState;
use crate::models::store::Store;
use crate::processor::favourites::{self, MoveToTrash, ToggleOutcome};
use crate::processor::photo_folder::{self, PhotoFolder};
use crate::processor::sessions::{self, SessionDirInfo, SessionSummary};
use crate::processor::settings;
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Command {
    OpenFolder(PathBuf),
    CreateSession { name: String, dir: Option<PathBuf> },
    InspectSessionDir(PathBuf),
    SetActiveSession(SessionId),
    ForgetSession(SessionId),
    ToggleFavourite(PathBuf),
    UpdateSettings(Settings),
    DetectGoogleDrive,
}

#[derive(Debug, Clone)]
pub enum Event {
    Snapshot(PersistedState),
    FolderOpened(PhotoFolder),
    SessionsChanged {
        sessions: Vec<SessionSummary>,
        active: Option<SessionId>,
    },
    /// Replaces the UI's ★ set wholesale.
    FavouritesOfActive {
        session_id: Option<SessionId>,
        sources: HashSet<PathBuf>,
    },
    SessionDirInspected {
        dir: PathBuf,
        info: SessionDirInfo,
    },
    /// Kept apart from `Error` so the New Session dialog can stay open and show it inline.
    SessionCreateFailed(AppError),
    FavouriteToggled {
        source: PathBuf,
        favourited: bool,
    },
    /// Lets the UI revert its optimistic ★ flip for exactly this photo.
    FavouriteFailed {
        source: PathBuf,
        error: AppError,
    },
    SettingsChanged(Settings),
    /// Kept apart from `Error` so the Settings dialog can stay open and show it inline.
    SettingsUpdateFailed(AppError),
    /// Suggested favourites root inside a local Google Drive for desktop folder, if one exists.
    GoogleDriveDetected(Option<PathBuf>),
    Error(AppError),
}

pub struct CoreHandle {
    pub commands: Sender<Command>,
    pub events: Receiver<Event>,
}

/// `wake_ui` runs after every event (eframe passes `ctx.request_repaint`; tests pass a no-op).
/// The core thread exits when the UI drops its `CoreHandle`.
pub fn spawn_core(state_path: PathBuf, wake_ui: impl Fn() + Send + 'static) -> AppResult<CoreHandle> {
    spawn_core_with_trash(state_path, favourites::move_to_os_trash, wake_ui)
}

pub fn spawn_core_with_trash(
    state_path: PathBuf,
    move_to_trash: MoveToTrash,
    wake_ui: impl Fn() + Send + 'static,
) -> AppResult<CoreHandle> {
    let mut store = Store::load_or_default(state_path)?;
    let (command_tx, command_rx) = unbounded::<Command>();
    let (event_tx, event_rx) = unbounded::<Event>();

    std::thread::Builder::new().name("core".into()).spawn(move || {
        let emit_all = |events: Vec<Event>| {
            for event in events {
                // The UI hung up: nothing left to do but finish the current loop iteration.
                let _ = event_tx.send(event);
            }
            wake_ui();
        };
        let mut startup = vec![Event::Snapshot(store.state.clone())];
        startup.extend(sessions_and_favourites(&mut store));
        startup.extend(photo_folder::reopen_last_folder(&store).map(Event::FolderOpened));
        emit_all(startup);
        for command in command_rx {
            emit_all(handle(&mut store, command, move_to_trash));
        }
    })?;

    Ok(CoreHandle {
        commands: command_tx,
        events: event_rx,
    })
}

fn handle(store: &mut Store, command: Command, move_to_trash: MoveToTrash) -> Vec<Event> {
    let result = match command {
        Command::OpenFolder(folder) => {
            photo_folder::open_photo_folder(store, &folder).map(|opened| vec![Event::FolderOpened(opened)])
        }
        Command::CreateSession { name, dir } => Ok(sessions::create_session(store, &name, dir).map_or_else(
            |error| vec![Event::SessionCreateFailed(error)],
            |_| sessions_and_favourites(store),
        )),
        Command::InspectSessionDir(dir) => {
            let info = sessions::inspect_session_dir(&dir);
            Ok(vec![Event::SessionDirInspected { dir, info }])
        }
        Command::SetActiveSession(id) => {
            sessions::set_active_session(store, id).map(|_| sessions_and_favourites(store))
        }
        Command::ForgetSession(id) => sessions::forget_session(store, id).map(|_| sessions_and_favourites(store)),
        Command::ToggleFavourite(source) => Ok(toggle_favourite(store, source, move_to_trash)),
        Command::UpdateSettings(new_settings) => Ok(vec![
            settings::update_settings(store, new_settings)
                .map_or_else(Event::SettingsUpdateFailed, Event::SettingsChanged),
        ]),
        Command::DetectGoogleDrive => Ok(vec![Event::GoogleDriveDetected(
            settings::detect_google_drive_favourites_root(),
        )]),
    };
    result.unwrap_or_else(|error| vec![Event::Error(error)])
}

fn toggle_favourite(store: &mut Store, source: PathBuf, move_to_trash: MoveToTrash) -> Vec<Event> {
    match favourites::toggle_favourite(store, &source, move_to_trash) {
        Ok(ToggleOutcome::Favourited) => {
            vec![
                Event::FavouriteToggled {
                    source,
                    favourited: true,
                },
                sessions_changed(store),
            ]
        }
        Ok(ToggleOutcome::Unfavourited { kept_file }) => {
            let mut events = vec![
                Event::FavouriteToggled {
                    source,
                    favourited: false,
                },
                sessions_changed(store),
            ];
            events.extend(kept_file.map(|message| Event::Error(AppError::Io(message))));
            events
        }
        Err(error) => vec![Event::FavouriteFailed { source, error }],
    }
}

fn sessions_changed(store: &Store) -> Event {
    Event::SessionsChanged {
        sessions: sessions::session_summaries(store),
        active: store.state.active_session_id,
    }
}

fn sessions_and_favourites(store: &mut Store) -> Vec<Event> {
    let favourites = match sessions::favourites_of_active_session(store) {
        Ok((session_id, sources)) => Event::FavouritesOfActive { session_id, sources },
        Err(error) => Event::Error(error),
    };
    vec![sessions_changed(store), favourites]
}
