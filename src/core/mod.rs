//! Endpoint layer: the UI talks to the app only through `Command`s and hears back through `Event`s.
//! One thread owns the `Store`, so every state change is serialized without locks.

use crate::error::{AppError, AppResult};
use crate::integrations::cloud::{
    CloudConfig, CloudProblem, CloudReport, CloudWorker, WorkerRequest, spawn_cloud_worker,
};
use crate::models::cloud::{CloudProvider, WebDavServer};
use crate::models::session::SessionId;
use crate::models::settings::Settings;
use crate::models::state::PersistedState;
use crate::models::store::Store;
use crate::processor::cloud_sync::{self, CloudStatus};
use crate::processor::favourites::{self, MoveToTrash, ToggleOutcome};
use crate::processor::photo_folder::{self, PhotoFolder};
use crate::processor::sessions::{self, SessionDirInfo, SessionSummary};
use crate::processor::settings;
use crossbeam_channel::{Receiver, Sender, select, unbounded};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

#[derive(Debug)]
pub enum Command {
    OpenFolder(PathBuf),
    CreateSession {
        name: String,
        dir: Option<PathBuf>,
    },
    InspectSessionDir(PathBuf),
    SetActiveSession(SessionId),
    ForgetSession(SessionId),
    ToggleFavourite(PathBuf),
    UpdateSettings(Settings),
    DetectGoogleDrive,
    /// Opens the provider's sign-in page in the browser.
    ConnectCloud(CloudProvider),
    ConnectWebDav {
        server: WebDavServer,
        password: String,
    },
    DisconnectCloud(CloudProvider),
    SetCloudUploads {
        provider: CloudProvider,
        enabled: bool,
    },
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
    /// One entry per provider, in `CloudProvider::ALL` order. Sent at startup and whenever any of it changes.
    CloudStatusChanged(Vec<CloudStatus>),
    /// Kept apart from `Error` so Settings can show it next to the provider.
    CloudConnectFailed {
        provider: CloudProvider,
        error: String,
    },
    Error(AppError),
}

pub struct CoreHandle {
    pub commands: Sender<Command>,
    pub events: Receiver<Event>,
}

/// `wake_ui` runs after every event (eframe passes `ctx.request_repaint`; tests pass a no-op).
/// The core thread exits when the UI drops its `CoreHandle`.
pub fn spawn_core(state_path: PathBuf, wake_ui: impl Fn() + Send + 'static) -> AppResult<CoreHandle> {
    spawn_core_with(
        state_path,
        favourites::move_to_os_trash,
        CloudConfig::production(),
        wake_ui,
    )
}

pub fn spawn_core_with(
    state_path: PathBuf,
    move_to_trash: MoveToTrash,
    cloud_config: CloudConfig,
    wake_ui: impl Fn() + Send + 'static,
) -> AppResult<CoreHandle> {
    let mut store = Store::load_or_default(state_path)?;
    let (command_tx, command_rx) = unbounded::<Command>();
    let (event_tx, event_rx) = unbounded::<Event>();
    let connectable = CloudProvider::ALL
        .into_iter()
        .filter(|provider| cloud_config.can_connect(*provider))
        .collect();
    let mut cloud = CloudSync {
        worker: spawn_cloud_worker(cloud_config)?,
        connectable,
        reports_applied: 0,
        problems: BTreeMap::new(),
        last_statuses: Vec::new(),
    };

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
        startup.extend(cloud.sync(&store));
        startup.extend(photo_folder::reopen_last_folder(&store).map(Event::FolderOpened));
        emit_all(startup);
        loop {
            let mut events = select! {
                recv(command_rx) -> command => match command {
                    Ok(command) => handle(&mut store, &mut cloud, command, move_to_trash),
                    Err(_) => return,
                },
                recv(cloud.worker.reports) -> report => match report {
                    Ok((sequence, report)) => cloud.apply(&mut store, sequence, report),
                    Err(_) => return,
                },
            };
            events.extend(cloud.sync(&store));
            emit_all(events);
        }
    })?;

    Ok(CoreHandle {
        commands: command_tx,
        events: event_rx,
    })
}

/// The core's side of uploading: keeps the worker's plan current and the UI's status fresh.
struct CloudSync {
    worker: CloudWorker,
    connectable: Vec<CloudProvider>,
    reports_applied: u64,
    /// Only the worker knows about network trouble, so this isn't persisted.
    problems: BTreeMap<CloudProvider, CloudProblem>,
    last_statuses: Vec<CloudStatus>,
}

impl CloudSync {
    fn request(&self, request: WorkerRequest) {
        // The worker thread only stops when this sender is dropped, so the send can't fail before then.
        let _ = self.worker.requests.send(request);
    }

    /// Hands the worker the current job list; returns a status event when something the UI shows changed.
    fn sync(&mut self, store: &Store) -> Option<Event> {
        let plan = cloud_sync::build_cloud_plan(&store.state, self.reports_applied);
        let statuses = cloud_sync::cloud_statuses(&store.state, &plan, &self.problems, |provider| {
            self.connectable.contains(&provider)
        });
        self.request(WorkerRequest::Plan(plan));
        (statuses != self.last_statuses).then(|| {
            self.last_statuses = statuses.clone();
            Event::CloudStatusChanged(statuses)
        })
    }

    fn apply(&mut self, store: &mut Store, sequence: u64, report: CloudReport) -> Vec<Event> {
        self.reports_applied = sequence;
        if let CloudReport::Problem { provider, problem } = &report {
            match problem {
                Some(problem) => self.problems.insert(*provider, problem.clone()),
                None => self.problems.remove(provider),
            };
        }
        match cloud_sync::apply_cloud_report(store, report) {
            Ok(None) => Vec::new(),
            Ok(Some((provider, error))) => vec![Event::CloudConnectFailed { provider, error }],
            Err(error) => vec![Event::Error(error)],
        }
    }
}

fn handle(store: &mut Store, cloud: &mut CloudSync, command: Command, move_to_trash: MoveToTrash) -> Vec<Event> {
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
        Command::ConnectCloud(provider) => {
            cloud.request(WorkerRequest::Connect(provider));
            Ok(Vec::new())
        }
        Command::ConnectWebDav { server, password } => match reqwest::Url::parse(&server.url) {
            Ok(url) if matches!(url.scheme(), "https" | "http") => {
                cloud.request(WorkerRequest::ConnectWebDav { server, password });
                Ok(Vec::new())
            }
            _ => Ok(vec![Event::CloudConnectFailed {
                provider: CloudProvider::WebDav,
                error: format!("\"{}\" isn't a web address", server.url),
            }]),
        },
        Command::DisconnectCloud(provider) => cloud_sync::disconnect_cloud(store, provider).map(|()| {
            cloud.problems.remove(&provider);
            cloud.request(WorkerRequest::Disconnect(provider));
            Vec::new()
        }),
        Command::SetCloudUploads { provider, enabled } => {
            cloud_sync::set_cloud_uploads_enabled(store, provider, enabled).map(|()| Vec::new())
        }
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
        Ok(ToggleOutcome::Unfavourited { kept_files_warning }) => {
            let mut events = vec![
                Event::FavouriteToggled {
                    source,
                    favourited: false,
                },
                sessions_changed(store),
            ];
            events.extend(kept_files_warning.map(|message| Event::Error(AppError::Io(message))));
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
