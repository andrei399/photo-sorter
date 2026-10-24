# 05 — Google Drive Sync

Goal: favourites end up in Google Drive automatically, in a Drive folder that matches their session (`Photo Sorter/Wedding Ana & Mihai/…`).

There are two paths. **Ship Path A first**: it's documentation plus a settings suggestion, done in an hour. Build Path B for Linux users and people who don't want the Drive desktop client.

---

## Path A — Zero-code: Google Drive for desktop

Google Drive for desktop (Windows, macOS) exposes Drive as a local folder, for example `G:\My Drive` on Windows or `~/Library/CloudStorage/GoogleDrive-<email>/My Drive` on macOS. Anything written there uploads automatically.

### Steps

1. Install Google Drive for desktop and sign in.
2. Photo Sorter → Settings → Storage → set the **favourites root** to `<My Drive>/Photo Sorter`.
3. New sessions are created as `My Drive/Photo Sorter/<session>/`, and every favourite uploads. Un-favourite moves the file to the trash, and Drive mirrors the deletion.

### Small code addition (`processor/settings.rs` + settings dialog)

`pub fn detect_google_drive_folder() -> Option<PathBuf>` uses path probing only, with no API:
- Windows: drive letters whose root contains `My Drive` (default `G:`). Legacy `%USERPROFILE%\Google Drive`.
- macOS: glob `~/Library/CloudStorage/GoogleDrive-*/My Drive`.
- Linux: `None`.

If it finds one, the Storage section offers: "Google Drive detected. Save favourites there?" (one click). On Linux it shows "Use Drive sync (beta)" instead, which is Path B. Phase 03's `.partial` + rename already keeps half-written files from uploading under their final names.

### Linux alternative (documented, not built)

`rclone mount gdrive: ~/GoogleDrive --vfs-cache-mode writes`, then point the favourites root inside it.

---

## Path B — Native Drive API sync

### Design summary

- OAuth 2.0 **installed-app flow**: loopback redirect plus PKCE in the system browser.
- Scope **`https://www.googleapis.com/auth/drive.file`**: the app sees only files **it created**. Google classifies this scope as **non-sensitive** (basic verification only, no security assessment).
- Refresh token in the **OS keychain** (`keyring`: Windows Credential Manager, macOS Keychain, Linux Secret Service). Access token in memory only.
- **The state is the queue**: a favourite with `drive_file_id == None` in a sync-enabled session *is* a pending upload. Nothing separate to persist, and nothing to drift.
- The **core actor stays the only writer of state.** The Drive worker runs on its own thread with a tokio runtime (reqwest needs async for streaming uploads and timers). It receives work plans from the core and reports results back as `Command`s.

```
core (owns Store) ──DrivePlan──▶ drive worker (tokio thread) ──HTTP──▶ Google
      ▲                                   │
      └──── Command::DriveUploaded{…} ────┘  (core re-checks + saves + emits Event::DriveStatus)
```

### Step 1 — Google Cloud setup (one-time, by Andrei)

1. console.cloud.google.com → new project "Photo Sorter" → enable the **Google Drive API**.
2. OAuth consent screen: External. App name, support email, privacy policy URL (phase 07). Scopes: `.../auth/drive.file`, `openid`, `email`.
3. **Publish to "In production".** In "Testing" status, refresh tokens **expire after 7 days**. With only non-sensitive scopes, production needs basic brand verification only. Expect the "unverified app" screen until that's done.
4. Credentials → OAuth client ID → **Desktop app**. Google treats desktop client secrets as non-confidential, so embedding them is expected.
5. Provide them at build time as `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` (CI secrets), read via `option_env!`. If they're missing, the Drive panel says "Drive sync not configured in this build". Never commit them.

### Step 2 — Model changes (`models/`)

```rust
// Favourite
#[serde(default)] pub drive_file_id: Option<String>,
// Session
#[serde(default)] pub drive_sync_enabled: bool,
#[serde(default)] pub drive_folder_id: Option<String>,
/// Drive files to trash after un-favourite; survives restarts/offline.
#[serde(default)] pub pending_drive_trash: Vec<String>,
// PersistedState
#[serde(default)] pub drive_root_folder_id: Option<String>,    // "Photo Sorter"
#[serde(default)] pub drive_account_email: Option<String>,     // display only
```
In phase 03's un-favourite: if the entry had a `drive_file_id`, push it onto `pending_drive_trash` before removing the entry.

### Step 3 — OAuth (`integrations/google_auth.rs`)

Deps: `tokio` (rt, net, time, fs, macros), `oauth2 = "5"`, `reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }`, `tokio-util` (`ReaderStream`), `keyring = "3"`, `futures`, `mime_guess`.

`pub async fn connect_google_account() -> AppResult<(String /*email*/, RefreshToken)>`
1. Bind `tokio::net::TcpListener` on `127.0.0.1:0` (loopback only).
2. Build the auth URL with `oauth2::BasicClient`: `https://accounts.google.com/o/oauth2/v2/auth`, token `https://oauth2.googleapis.com/token`, redirect `http://127.0.0.1:<port>`, PKCE S256, CSRF `state`, scopes `drive.file openid email`, extras `access_type=offline`, `prompt=consent`.
3. `opener::open_browser(auth_url)`.
4. Accept **one** connection with a 5-minute timeout. Parse `GET /?code=…&state=…`, verify `state`, reply "You can close this tab and return to Photo Sorter."
5. Exchange code + verifier for tokens. Store the refresh token with `keyring::Entry::new("photo-sorter", "google-refresh-token")`.
6. Email: decode the id_token payload (it came over TLS straight from Google's token endpoint, so no signature check is needed for a display-only value).

`DriveAuth` (owned by the worker): `access_token()` refreshes when < 60 s remain, and `invalid_grant` → `AppError::DriveAuthExpired`. `disconnect()` posts to `https://oauth2.googleapis.com/revoke` and deletes the keychain entry.

### Step 4 — Drive REST client (`integrations/google_drive.rs`)

Hand-rolled with reqwest. Four endpoints don't justify the generated `google-drive3` crate. Base URLs are constructor params so tests can point at wiremock.

```rust
impl DriveClient {
    pub async fn create_folder(&self, name: &str, parent_id: Option<&str>) -> AppResult<String>
    // POST {api}/drive/v3/files?fields=id  {"name", "mimeType":"application/vnd.google-apps.folder", "parents":[…]}
    pub async fn folder_exists(&self, id: &str) -> AppResult<bool>
    // GET {api}/drive/v3/files/{id}?fields=id,trashed → 404 or trashed ⇒ false
    pub async fn upload_file(&self, local: &Path, name: &str, parent_id: &str) -> AppResult<String>
    // POST {upload}/upload/drive/v3/files?uploadType=resumable&fields=id  (X-Upload-Content-Type/-Length, JSON metadata)
    //   → Location = session URI → PUT session URI, body = Body::wrap_stream(ReaderStream::new(File)) → {"id"}
    pub async fn trash_file(&self, id: &str) -> AppResult<()>
    // PATCH {api}/drive/v3/files/{id} {"trashed":true}; 404 ⇒ Ok
}
```
Status mapping: 401 → refresh once and retry. 429, 403 `rateLimitExceeded`/`userRateLimitExceeded`, 5xx and network errors → `Retryable`. 403 `storageQuotaExceeded` → `Drive("Google Drive is full")`, not retried. 404 on the parent → recreate the folder.

<!-- ponytail: failed uploads restart from byte 0; add Content-Range resume only if users push >200 MB files over flaky links -->

### Step 5 — The plan/result protocol (`processor/drive_sync.rs`)

Core side (sync, owns the store):
```rust
pub struct DrivePlan {
    pub root_folder_id: Option<String>,
    pub sessions: Vec<SessionPlan>,      // only drive_sync_enabled sessions
}
pub struct SessionPlan { pub session_id: SessionId, pub name: String, pub folder_id: Option<String>,
                         pub uploads: Vec<UploadJob>, pub trash: Vec<String> }
pub struct UploadJob { pub source: PathBuf, pub local_copy: PathBuf, pub file_name: String }

pub fn build_drive_plan(state: &PersistedState) -> DrivePlan
pub fn apply_drive_result(store: &mut Store, result: DriveResult) -> AppResult<()>
```
The core sends a fresh `DrivePlan` to the worker after every state mutation (when connected) and on startup.

`apply_drive_result` handles the results the worker sends as `Command::DriveResult(..)`:
- `RootFolderCreated(id)`, `SessionFolderCreated { session_id, id }` → save the id.
- `Uploaded { session_id, source, file_id }` → **re-check the favourite still exists and still has no id** → set `drive_file_id` and save. If it was un-favourited mid-upload, push `file_id` onto `pending_drive_trash` instead, so nothing is orphaned in Drive.
- `Trashed { session_id, file_id }` → remove it from `pending_drive_trash`.
- `FolderMissing { session_id }` → clear `drive_folder_id` and all of that session's `drive_file_id`s. Everything re-uploads into a recreated folder.
- `Connected { email }`, `Disconnected` → save or clear the account and all `drive_*` ids.

Then the core emits `Event::DriveStatus` and sends a fresh plan.

Worker side (`integrations::drive_worker`, a tokio current-thread runtime on thread `"drive"`):
1. Hold the latest `DrivePlan`. Each new plan replaces the old one.
2. Keep an in-flight set keyed by job (`source` / `file_id`) and a backoff map `key → (attempts, next_try_at)`. Exponential 1 s → 5 min, ± 20 % jitter. Wake on a new plan or the earliest backoff expiry.
3. Ensure the root folder, then each session folder (`folder_exists` check once per session per run). Then run uploads and trashes with **at most 3 concurrent** (`tokio::sync::Semaphore`).
4. Send every outcome to the core as a `Command::DriveResult`. `DriveAuthExpired` → pause and report `NeedsReconnect`. Network down → report `Offline` and keep backing off.
5. Report live progress (`Syncing { pending, failed }` per session) through `Command::DriveResult(Progress(..))`, so the UI has a single event stream.

```rust
pub enum DriveSyncState { NotConfigured, NotConnected, Idle, Syncing, Offline, NeedsReconnect, Error(String) }
Event::DriveStatus { state: DriveSyncState, sessions: Vec<SessionSyncProgress> }  // uploaded/pending/failed per session
```

### Step 6 — Commands (UI → core)

| Command | Behaviour |
|---|---|
| `DriveConnect` | Forwarded to the worker, which runs OAuth and replies with `Connected { email }`. |
| `DriveDisconnect` | Worker revokes the token and clears the keychain. Core clears the `drive_*` ids. Files stay in Drive. |
| `SetSessionDriveSync { session_id, enabled }` | Enabling queues all existing favourites (no ids yet). Disabling stops future uploads and leaves Drive untouched. **Refuse** if `session.dir` is inside a detected Drive for desktop folder: "Already synced by Google Drive for desktop." |
| `OpenSessionInDrive(session_id)` | `opener::open_browser("https://drive.google.com/drive/folders/<id>")`. |

### Step 7 — UI (`ui/drive_panel.rs`)

Design gate: mock 2 variants of the sync indicator (a cloud badge in the top bar vs. a status line under the session name) plus the connect flow. Publish and wait for a pick.

- Settings → Google Drive: Connect → "Connected as x@gmail.com" + Disconnect.
- Session switcher: a per-session "Sync to Drive" toggle (default **on** for new sessions while connected) and "Open in Drive".
- Indicator: ☁✓ synced · ☁↑ 12 left · ☁⚠ offline, retrying · ☁✕ reconnect (click → `DriveConnect`) · ☁✕ Drive full.
- Quit with pending uploads: intercept the close via `ctx.input(|i| i.viewport().close_requested())` + `ViewportCommand::CancelClose`, then ask "12 photos still uploading. Quit anyway? They'll upload next time." Nothing is lost either way, because the state is the queue.

### Step 8 — Tests (`tests/drive_sync.rs`)

Use **`wiremock`** to fake the token and Drive endpoints. The test core is built with `DriveClient` base URLs pointing at the mock server and a fixed access token. No keychain, browser or real network.
1. **upload_flow**: enable sync → favourite 3 → root and session folders created (assert POST bodies) → 3 resumable uploads (assert metadata name/parent, byte-exact PUT bodies) → `state_on_disk()` has 3 `drive_file_id`s → final `DriveStatus` is `Idle` with `uploaded: 3`.
2. **unfavourite_trashes_in_drive**: PATCH `{"trashed":true}` → `pending_drive_trash` empty on disk.
3. **retry_on_503_then_success**: 503 ×2 then 200 → uploaded (`tokio::time::pause` inside the worker runtime under test, so backoff takes no wall time).
4. **offline_persists_queue**: server down → `Offline`, ids still `None` on disk → new core from the same dir → server up → uploads complete.
5. **deleted_drive_folder_recreated**, **auth_expired → NeedsReconnect with zero upload calls**, **unfavourite_during_upload → late id trashed**, **sync_disabled_session_ignored (`expect(0)`)**, **quota_exceeded_not_retried**, **double_sync_refused** (session dir inside a fake "My Drive" folder).
