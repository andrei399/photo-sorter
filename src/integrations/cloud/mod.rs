//! External layer: uploads favourites to every connected and enabled cloud provider at once.
//! Runs on its own thread with a tokio runtime; the core stays the only writer of state. After every change the
//! core sends a `CloudPlan` (every job still outstanding), and the worker answers with numbered `CloudReport`s.
//!
//! ```text
//! core (owns Store) ──WorkerRequest::Plan──▶ cloud worker ──HTTPS──▶ Google / Microsoft / Dropbox / WebDAV
//!       ▲                                         │
//!       └────────── (sequence, CloudReport) ──────┘
//! ```

mod dropbox;
mod google_drive;
mod oauth;
mod onedrive;
mod webdav;

use crate::models::cloud::{CloudAccount, CloudProvider, WebDavServer};
use crate::models::session::SessionId;
use reqwest::{Response, StatusCode};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use tokio::time::Instant;

const MAX_TRANSFERS_PER_PROVIDER: usize = 3;
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5 * 60);
/// A file the provider refused (storage full, too large) is retried rarely, in case the user made room.
const REJECTED_RETRY_DELAY: Duration = Duration::from_secs(60 * 60);
/// Refresh access tokens this long before they expire, so a request never starts with a dying token.
const TOKEN_EXPIRY_MARGIN: Duration = Duration::from_secs(60);
const KEYCHAIN_SERVICE: &str = "photo-sorter";

#[derive(Debug, Clone)]
pub struct OAuthClient {
    pub id: String,
    /// Google requires it even for desktop apps and documents it as not confidential; PKCE does the protecting.
    pub secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderEndpoints {
    pub authorize: String,
    pub token: String,
    /// Metadata API root.
    pub api: String,
    /// File-bytes API root; differs from `api` only on Dropbox.
    pub content: String,
    /// `None`: this build has no OAuth app registered for the provider, so it can't be connected.
    pub client: Option<OAuthClient>,
}

pub struct CloudConfig {
    pub google_drive: ProviderEndpoints,
    pub onedrive: ProviderEndpoints,
    pub dropbox: ProviderEndpoints,
    pub secrets: Arc<dyn SecretStore>,
    /// Shows the provider's sign-in page; injected so tests can play the user's browser.
    pub open_browser: fn(&str) -> std::io::Result<()>,
    /// Doubles per consecutive failure, up to five minutes.
    pub first_retry_delay: Duration,
}

impl CloudConfig {
    /// Client ids come from CI secrets at build time; they are never committed.
    pub fn production() -> Self {
        let client = |id: Option<&'static str>, secret: Option<&'static str>| {
            id.filter(|id| !id.is_empty()).map(|id| OAuthClient {
                id: id.to_owned(),
                secret: secret.filter(|secret| !secret.is_empty()).map(str::to_owned),
            })
        };
        Self {
            google_drive: ProviderEndpoints {
                authorize: "https://accounts.google.com/o/oauth2/v2/auth".into(),
                token: "https://oauth2.googleapis.com/token".into(),
                api: "https://www.googleapis.com".into(),
                content: "https://www.googleapis.com".into(),
                client: client(option_env!("GOOGLE_CLIENT_ID"), option_env!("GOOGLE_CLIENT_SECRET")),
            },
            onedrive: ProviderEndpoints {
                authorize: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
                token: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
                api: "https://graph.microsoft.com/v1.0".into(),
                content: "https://graph.microsoft.com/v1.0".into(),
                client: client(option_env!("ONEDRIVE_CLIENT_ID"), None),
            },
            dropbox: ProviderEndpoints {
                authorize: "https://www.dropbox.com/oauth2/authorize".into(),
                token: "https://api.dropboxapi.com/oauth2/token".into(),
                api: "https://api.dropboxapi.com".into(),
                content: "https://content.dropboxapi.com".into(),
                client: client(option_env!("DROPBOX_APP_KEY"), None),
            },
            secrets: Arc::new(OsKeychain),
            open_browser: |url| opener::open_browser(url).map_err(std::io::Error::other),
            first_retry_delay: Duration::from_secs(2),
        }
    }

    fn endpoints(&self, provider: CloudProvider) -> Option<&ProviderEndpoints> {
        match provider {
            CloudProvider::GoogleDrive => Some(&self.google_drive),
            CloudProvider::OneDrive => Some(&self.onedrive),
            CloudProvider::Dropbox => Some(&self.dropbox),
            CloudProvider::WebDav => None,
        }
    }

    /// WebDAV needs no app registration; the OAuth providers need a client id baked into the build.
    pub fn can_connect(&self, provider: CloudProvider) -> bool {
        self.endpoints(provider)
            .is_none_or(|endpoints| endpoints.client.is_some())
    }
}

/// Holds each provider's long-lived secret: an OAuth refresh token, or the WebDAV (app) password.
pub trait SecretStore: Send + Sync {
    fn load(&self, provider: CloudProvider) -> Result<Option<String>, String>;
    fn save(&self, provider: CloudProvider, secret: &str) -> Result<(), String>;
    fn delete(&self, provider: CloudProvider) -> Result<(), String>;
}

/// Windows Credential Manager, macOS Keychain, or the Secret Service (GNOME Keyring, KWallet) on Linux.
pub struct OsKeychain;

impl SecretStore for OsKeychain {
    fn load(&self, provider: CloudProvider) -> Result<Option<String>, String> {
        match keychain_entry(provider)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn save(&self, provider: CloudProvider, secret: &str) -> Result<(), String> {
        keychain_entry(provider)?
            .set_password(secret)
            .map_err(|error| error.to_string())
    }

    fn delete(&self, provider: CloudProvider) -> Result<(), String> {
        match keychain_entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn keychain_entry(provider: CloudProvider) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, provider.keychain_key()).map_err(|error| error.to_string())
}

/// One file of one session on one provider. A plan never holds an upload and a deletion for the same key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JobKey {
    pub provider: CloudProvider,
    pub session_id: SessionId,
    /// Name inside the session folder; also the name it gets in the cloud.
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudJob {
    Upload {
        key: JobKey,
        /// Cloud folder the session's files go into, under the app's own folder.
        remote_folder: String,
        local_path: PathBuf,
    },
    /// The favourite is gone; its cloud copy goes to the provider's trash (where it has one).
    Delete { key: JobKey, remote_id: String },
}

impl CloudJob {
    pub fn key(&self) -> &JobKey {
        match self {
            Self::Upload { key, .. } | Self::Delete { key, .. } => key,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloudPlan {
    /// Connected and enabled accounts only.
    pub accounts: Vec<CloudAccount>,
    pub jobs: Vec<CloudJob>,
    /// Sequence number of the last report the core had applied when it built `jobs`.
    pub reports_applied: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudProblem {
    /// Offline, rate-limited or the server is down; retrying with backoff. Nothing is lost.
    Retrying(String),
    /// Access was revoked or expired (password changed, app removed): the user has to connect again.
    NeedsReconnect,
    /// The provider refused a file for good: storage full, file too large.
    Rejected(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudReport {
    Uploaded {
        key: JobKey,
        remote_id: String,
    },
    Deleted {
        key: JobKey,
    },
    Connected(CloudAccount),
    ConnectFailed {
        provider: CloudProvider,
        error: String,
    },
    /// `None` once the provider works again.
    Problem {
        provider: CloudProvider,
        problem: Option<CloudProblem>,
    },
}

#[derive(Debug)]
pub enum WorkerRequest {
    Plan(CloudPlan),
    /// Runs the provider's browser sign-in.
    Connect(CloudProvider),
    ConnectWebDav {
        server: WebDavServer,
        password: String,
    },
    /// Forgets the provider's secret. Files already uploaded stay in the cloud.
    Disconnect(CloudProvider),
}

pub struct CloudWorker {
    pub requests: mpsc::UnboundedSender<WorkerRequest>,
    /// `(sequence number, report)`, numbered from 1. Ends when the worker thread does.
    pub reports: crossbeam_channel::Receiver<(u64, CloudReport)>,
}

/// The worker exits when `CloudWorker::requests` is dropped; transfers in progress are abandoned and simply
/// run again next launch, because the state is the queue.
pub fn spawn_cloud_worker(config: CloudConfig) -> std::io::Result<CloudWorker> {
    let (request_tx, request_rx) = mpsc::unbounded_channel();
    let (report_tx, report_rx) = crossbeam_channel::unbounded();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    std::thread::Builder::new().name("cloud".into()).spawn(move || {
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        let worker = Worker {
            config: Arc::new(config),
            http: reqwest::Client::new(),
            plan: CloudPlan::default(),
            clients: HashMap::new(),
            in_flight: HashSet::new(),
            finished: HashMap::new(),
            retries: HashMap::new(),
            needs_reconnect: HashSet::new(),
            reported_problems: HashMap::new(),
            reports: report_tx,
            last_report: 0,
            done_tx,
        };
        runtime.block_on(worker.run(request_rx, done_rx));
    })?;
    Ok(CloudWorker {
        requests: request_tx,
        reports: report_rx,
    })
}

enum TaskDone {
    Job {
        key: JobKey,
        outcome: Result<Option<String>, CloudError>,
    },
    Connect(Result<CloudAccount, (CloudProvider, String)>),
}

struct Worker {
    config: Arc<CloudConfig>,
    http: reqwest::Client,
    plan: CloudPlan,
    clients: HashMap<CloudProvider, Arc<ProviderClient>>,
    in_flight: HashSet<JobKey>,
    /// Done, but maybe not yet in the core's latest plan: value = the report's sequence number.
    /// Without this, a plan built before the core saw a report would upload the file twice.
    finished: HashMap<JobKey, u64>,
    /// Consecutive failures and when to try again.
    retries: HashMap<JobKey, (u32, Instant)>,
    needs_reconnect: HashSet<CloudProvider>,
    reported_problems: HashMap<CloudProvider, CloudProblem>,
    reports: crossbeam_channel::Sender<(u64, CloudReport)>,
    last_report: u64,
    done_tx: mpsc::UnboundedSender<TaskDone>,
}

impl Worker {
    async fn run(
        mut self,
        mut requests: mpsc::UnboundedReceiver<WorkerRequest>,
        mut done: mpsc::UnboundedReceiver<TaskDone>,
    ) {
        loop {
            self.start_due_jobs();
            let next_retry = self.next_retry();
            tokio::select! {
                request = requests.recv() => match request {
                    Some(request) => self.handle_request(request),
                    None => return,
                },
                Some(task) = done.recv() => self.finish(task),
                () = sleep_until(next_retry) => {}
            }
        }
    }

    fn handle_request(&mut self, request: WorkerRequest) {
        match request {
            WorkerRequest::Plan(plan) => {
                self.finished.retain(|_, sequence| *sequence > plan.reports_applied);
                self.clients.retain(|provider, client| {
                    plan.accounts
                        .iter()
                        .any(|account| account.provider == *provider && *account == client.account)
                });
                for account in &plan.accounts {
                    self.clients.entry(account.provider).or_insert_with(|| {
                        Arc::new(ProviderClient {
                            account: account.clone(),
                            http: self.http.clone(),
                            endpoints: self.config.endpoints(account.provider).cloned(),
                            secrets: self.config.secrets.clone(),
                            access_token: Mutex::new(None),
                            known_folders: Mutex::new(HashMap::new()),
                        })
                    });
                }
                self.plan = plan;
            }
            WorkerRequest::Connect(provider) => {
                let (config, http) = (self.config.clone(), self.http.clone());
                self.spawn_connect(async move {
                    connect_with_browser(&config, &http, provider)
                        .await
                        .map_err(|error| (provider, error))
                });
            }
            WorkerRequest::ConnectWebDav { server, password } => {
                let (secrets, http) = (self.config.secrets.clone(), self.http.clone());
                self.spawn_connect(async move {
                    connect_webdav(&http, secrets, server, password)
                        .await
                        .map_err(|error| (CloudProvider::WebDav, error))
                });
            }
            WorkerRequest::Disconnect(provider) => {
                self.clients.remove(&provider);
                self.needs_reconnect.remove(&provider);
                self.reported_problems.remove(&provider);
                let secrets = self.config.secrets.clone();
                tokio::task::spawn_blocking(move || {
                    if let Err(error) = secrets.delete(provider) {
                        log::warn!(
                            "couldn't remove the {} secret from the keychain: {error}",
                            provider.label()
                        );
                    }
                });
            }
        }
    }

    fn spawn_connect(
        &self,
        connect: impl Future<Output = Result<CloudAccount, (CloudProvider, String)>> + Send + 'static,
    ) {
        let done_tx = self.done_tx.clone();
        tokio::spawn(async move {
            let _ = done_tx.send(TaskDone::Connect(connect.await));
        });
    }

    fn start_due_jobs(&mut self) {
        let now = Instant::now();
        for job in &self.plan.jobs {
            let key = job.key();
            let busy = self.in_flight.contains(key)
                || self.finished.contains_key(key)
                || self.needs_reconnect.contains(&key.provider)
                || self.retries.get(key).is_some_and(|(_, retry_at)| *retry_at > now);
            let Some(client) = self.clients.get(&key.provider).filter(|_| !busy) else {
                continue;
            };
            let transfers = self.in_flight.iter().filter(|k| k.provider == key.provider).count();
            if transfers >= MAX_TRANSFERS_PER_PROVIDER {
                continue;
            }
            self.in_flight.insert(key.clone());
            let (client, job, done_tx) = (client.clone(), job.clone(), self.done_tx.clone());
            tokio::spawn(async move {
                let key = job.key().clone();
                let outcome = client.run(job).await;
                let _ = done_tx.send(TaskDone::Job { key, outcome });
            });
        }
    }

    fn next_retry(&self) -> Option<Instant> {
        let now = Instant::now();
        self.retries
            .values()
            .map(|(_, retry_at)| *retry_at)
            .filter(|retry_at| *retry_at > now)
            .min()
    }

    fn finish(&mut self, task: TaskDone) {
        match task {
            TaskDone::Job { key, outcome } => {
                self.in_flight.remove(&key);
                let provider = key.provider;
                match outcome {
                    Ok(remote_id) => {
                        self.retries.remove(&key);
                        let report = match remote_id {
                            Some(remote_id) => CloudReport::Uploaded {
                                key: key.clone(),
                                remote_id,
                            },
                            None => CloudReport::Deleted { key: key.clone() },
                        };
                        let sequence = self.report(report);
                        self.finished.insert(key, sequence);
                        self.set_problem(provider, None);
                    }
                    Err(CloudError::Unauthorized | CloudError::AuthExpired) => {
                        self.needs_reconnect.insert(provider);
                        self.set_problem(provider, Some(CloudProblem::NeedsReconnect));
                    }
                    Err(CloudError::Retryable(message)) => {
                        let failures = self.retries.get(&key).map_or(1, |(failures, _)| failures + 1);
                        let delay = self
                            .config
                            .first_retry_delay
                            .saturating_mul(1 << (failures - 1).min(16))
                            .min(MAX_RETRY_DELAY);
                        self.retries.insert(key, (failures, Instant::now() + delay));
                        self.set_problem(provider, Some(CloudProblem::Retrying(message)));
                    }
                    Err(CloudError::Rejected(message)) => {
                        self.retries.insert(key, (1, Instant::now() + REJECTED_RETRY_DELAY));
                        self.set_problem(provider, Some(CloudProblem::Rejected(message)));
                    }
                }
            }
            TaskDone::Connect(Ok(account)) => {
                // Rebuilt from the core's next plan, with the new secret.
                self.clients.remove(&account.provider);
                self.needs_reconnect.remove(&account.provider);
                let provider = account.provider;
                self.report(CloudReport::Connected(account));
                self.set_problem(provider, None);
            }
            TaskDone::Connect(Err((provider, error))) => {
                self.report(CloudReport::ConnectFailed { provider, error });
            }
        }
    }

    fn set_problem(&mut self, provider: CloudProvider, problem: Option<CloudProblem>) {
        let previous = match &problem {
            Some(problem) => self.reported_problems.insert(provider, problem.clone()),
            None => self.reported_problems.remove(&provider),
        };
        if previous != problem {
            self.report(CloudReport::Problem { provider, problem });
        }
    }

    fn report(&mut self, report: CloudReport) -> u64 {
        self.last_report += 1;
        // The core hung up: the app is closing.
        let _ = self.reports.send((self.last_report, report));
        self.last_report
    }
}

async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

async fn connect_with_browser(
    config: &CloudConfig,
    http: &reqwest::Client,
    provider: CloudProvider,
) -> Result<CloudAccount, String> {
    let endpoints = config
        .endpoints(provider)
        .ok_or_else(|| format!("{} doesn't use a browser sign-in", provider.label()))?;
    let tokens = oauth::sign_in_with_browser(http, provider, endpoints, config.open_browser).await?;
    let refresh_token = tokens
        .refresh_token
        .clone()
        .ok_or_else(|| format!("{} didn't grant lasting access; please try again", provider.label()))?;
    let account_label = match provider {
        CloudProvider::Dropbox => dropbox::account_email(http, endpoints, &tokens.access_token)
            .await
            .map_err(|error| error.to_string())?,
        _ => tokens.account_label().unwrap_or_else(|| provider.label().to_owned()),
    };
    save_secret(config.secrets.clone(), provider, refresh_token).await?;
    Ok(CloudAccount {
        provider,
        account_label,
        enabled: true,
        webdav: None,
    })
}

async fn connect_webdav(
    http: &reqwest::Client,
    secrets: Arc<dyn SecretStore>,
    server: WebDavServer,
    password: String,
) -> Result<CloudAccount, String> {
    webdav::check_login(http, &server, &password).await?;
    save_secret(secrets, CloudProvider::WebDav, password).await?;
    let host = reqwest::Url::parse(&server.url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default();
    Ok(CloudAccount {
        provider: CloudProvider::WebDav,
        account_label: format!("{} @ {host}", server.username),
        enabled: true,
        webdav: Some(server),
    })
}

async fn save_secret(secrets: Arc<dyn SecretStore>, provider: CloudProvider, secret: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || secrets.save(provider, &secret))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| format!("Couldn't save the sign-in to the system keychain: {error}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CloudError {
    /// Network trouble, rate limits, server errors.
    Retryable(String),
    /// The API refused the access token; refreshing it may help once.
    Unauthorized,
    /// The refresh token or password no longer works.
    AuthExpired,
    Rejected(String),
}

impl std::fmt::Display for CloudError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retryable(message) | Self::Rejected(message) => f.write_str(message),
            Self::Unauthorized | Self::AuthExpired => f.write_str("Sign-in expired"),
        }
    }
}

impl From<reqwest::Error> for CloudError {
    fn from(error: reqwest::Error) -> Self {
        Self::Retryable(error.to_string())
    }
}

/// Reading the session copy failed: the user deleted or moved it; it's dropped from the favourites on next look.
impl From<std::io::Error> for CloudError {
    fn from(error: std::io::Error) -> Self {
        Self::Rejected(format!("Couldn't read the file to upload: {error}"))
    }
}

/// Success passes through; everything else becomes the matching `CloudError`.
async fn check(response: Response) -> Result<Response, CloudError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(classify_failure(status, &body))
}

fn classify_failure(status: StatusCode, body: &str) -> CloudError {
    const QUOTA_MARKERS: [&str; 4] = [
        "storageQuotaExceeded",
        "quotaLimitReached",
        "insufficient_space",
        "insufficient_quota",
    ];
    const RATE_LIMIT_MARKERS: [&str; 2] = ["rateLimitExceeded", "userRateLimitExceeded"];
    if status == StatusCode::INSUFFICIENT_STORAGE || QUOTA_MARKERS.iter().any(|marker| body.contains(marker)) {
        return CloudError::Rejected("Storage is full".into());
    }
    match status.as_u16() {
        401 => CloudError::Unauthorized,
        408 | 423 | 425 | 429 | 500..=599 => CloudError::Retryable(format!("Server answered {status}")),
        403 if RATE_LIMIT_MARKERS.iter().any(|marker| body.contains(marker)) => {
            CloudError::Retryable(format!("Server answered {status}"))
        }
        _ => CloudError::Rejected(format!(
            "Server answered {status}: {}",
            body.chars().take(200).collect::<String>()
        )),
    }
}

/// Streams the file instead of loading it: RAWs run to 100 MB and three upload at once per provider.
async fn file_body(path: &Path) -> Result<(reqwest::Body, u64), CloudError> {
    let file = tokio::fs::File::open(path).await?;
    let length = file.metadata().await?.len();
    Ok((
        reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file)),
        length,
    ))
}

struct AccessToken {
    token: String,
    expires_at: Instant,
}

/// One connected account, shared by that provider's concurrent transfers.
struct ProviderClient {
    account: CloudAccount,
    http: reqwest::Client,
    /// `None` for WebDAV, which signs in with a password.
    endpoints: Option<ProviderEndpoints>,
    secrets: Arc<dyn SecretStore>,
    /// Behind an async lock so concurrent transfers refresh an expired token once, not three times.
    access_token: Mutex<Option<AccessToken>>,
    /// Remote folder path → the provider's id for it ("" where folders are addressed by path).
    /// Locked across lookup and creation, so parallel uploads never create the same folder twice.
    known_folders: Mutex<HashMap<String, String>>,
}

impl ProviderClient {
    /// Upload → `Some(remote id)`; delete → `None`.
    async fn run(&self, job: CloudJob) -> Result<Option<String>, CloudError> {
        match job {
            CloudJob::Upload {
                key,
                remote_folder,
                local_path,
            } => {
                let (folder, local, name) = (remote_folder.as_str(), local_path.as_path(), key.file_name.as_str());
                self.authorized(|credential| async move {
                    match self.account.provider {
                        CloudProvider::GoogleDrive => {
                            google_drive::upload(self, &credential, folder, local, name).await
                        }
                        CloudProvider::OneDrive => onedrive::upload(self, &credential, folder, local, name).await,
                        CloudProvider::Dropbox => dropbox::upload(self, &credential, folder, local, name).await,
                        CloudProvider::WebDav => webdav::upload(self, &credential, folder, local, name).await,
                    }
                })
                .await
                .map(Some)
            }
            CloudJob::Delete { remote_id, .. } => {
                let remote_id = remote_id.as_str();
                self.authorized(|credential| async move {
                    match self.account.provider {
                        CloudProvider::GoogleDrive => google_drive::delete(self, &credential, remote_id).await,
                        CloudProvider::OneDrive => onedrive::delete(self, &credential, remote_id).await,
                        CloudProvider::Dropbox => dropbox::delete(self, &credential, remote_id).await,
                        CloudProvider::WebDav => webdav::delete(self, &credential, remote_id).await,
                    }
                })
                .await
                .map(|()| None)
            }
        }
    }

    /// Runs `call` with a working credential; an access token the API rejects is refreshed and tried once more.
    async fn authorized<T, Call, Attempt>(&self, call: Call) -> Result<T, CloudError>
    where
        Call: Fn(String) -> Attempt,
        Attempt: Future<Output = Result<T, CloudError>>,
    {
        let refreshable = self.endpoints.is_some();
        match call(self.credential(false).await?).await {
            Err(CloudError::Unauthorized) if refreshable => match call(self.credential(true).await?).await {
                Err(CloudError::Unauthorized) => Err(CloudError::AuthExpired),
                other => other,
            },
            Err(CloudError::Unauthorized) => Err(CloudError::AuthExpired),
            other => other,
        }
    }

    /// The OAuth access token (refreshed as needed), or the WebDAV password.
    async fn credential(&self, force_refresh: bool) -> Result<String, CloudError> {
        let Some(endpoints) = &self.endpoints else {
            return self.load_secret().await;
        };
        let mut access_token = self.access_token.lock().await;
        if let Some(cached) = access_token.as_ref()
            && !force_refresh
            && cached.expires_at > Instant::now() + TOKEN_EXPIRY_MARGIN
        {
            return Ok(cached.token.clone());
        }
        let refresh_token = self.load_secret().await?;
        let tokens = oauth::refresh(&self.http, self.account.provider, endpoints, &refresh_token).await?;
        // Microsoft rotates refresh tokens on every use.
        if let Some(rotated) = tokens.refresh_token.filter(|rotated| *rotated != refresh_token)
            && let Err(error) = save_secret(self.secrets.clone(), self.account.provider, rotated).await
        {
            log::warn!("{error}");
        }
        *access_token = Some(AccessToken {
            token: tokens.access_token.clone(),
            expires_at: Instant::now() + Duration::from_secs(tokens.expires_in),
        });
        Ok(tokens.access_token)
    }

    async fn load_secret(&self) -> Result<String, CloudError> {
        let (secrets, provider) = (self.secrets.clone(), self.account.provider);
        match tokio::task::spawn_blocking(move || secrets.load(provider)).await {
            Ok(Ok(Some(secret))) => Ok(secret),
            Ok(Ok(None)) => Err(CloudError::AuthExpired),
            // A locked keychain unlocks once the user logs in again.
            Ok(Err(error)) => Err(CloudError::Retryable(format!("Keychain unavailable: {error}"))),
            Err(error) => Err(CloudError::Retryable(error.to_string())),
        }
    }

    fn endpoints(&self) -> &ProviderEndpoints {
        self.endpoints
            .as_ref()
            .expect("only OAuth providers' clients call this, and they always have endpoints")
    }
}
