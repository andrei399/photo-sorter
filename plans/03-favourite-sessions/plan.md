# 03 — Favourite Sessions

Goal: favourite the current photo with **Enter** or the on-screen **★**. The photo is copied into the **active session's folder**. **Ctrl+N** (Cmd+N on macOS) creates a new session, which is its own directory. A session switcher plus an "Open folder" button make all favourites one click away for uploading to Drive.

## Concepts

- **Session**: a name plus a directory ("Wedding Ana & Mihai" → `~/Pictures/Photo Sorter/Wedding Ana & Mihai`). One session is *active* at a time. Favouriting always targets the active session.
- **Favourites root**: the default parent dir for new sessions. Default `<Pictures>/Photo Sorter`. Changeable in phase 04, and the trick for zero-code Drive sync (phase 05 Path A).
- **Favourite**: a copy of a source photo inside a session dir. State records `source path → copied file name` per session, so the app knows which photos are starred and exactly which file to remove on un-favourite.
- A photo can be favourited in several sessions. The ★ shows its state **for the active session**.

## Step 0 — Design gate

Extend the picked viewer mock with 2–3 variants of: the session switcher (top-bar dropdown vs. side drawer vs. command-palette style), the ★ states (off / on / saving / error) and the New Session dialog. Publish, report the URL, **stop until Andrei picks**.

**Picked (2026-10-24):** the switcher is D1's dropdown anchored to the session button, filled with D2's session cards (name, ★ count, folder path, amber border on the active one) and D2's footer (+ New session, Forget). D3's **Ctrl/Cmd+K quick switcher** is added on top: type to filter with amber match highlighting, ↑↓ to choose, Enter switches, Ctrl+Enter creates a session named after the query. The dialog is **E2** (editable folder that follows the name until edited, a non-empty folder warning, a Google Drive hint, and "Create & favourite" when opened by Enter). The ★ states are **F**.

## Steps

### 1. Persisted model (`models/session.rs`, extend `models/state.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub dir: PathBuf,
    pub created_at: String,                                   // RFC 3339, display only
    /// Key = absolute source path. Value = what we created in `dir`.
    #[serde(default)] pub favourites: BTreeMap<PathBuf, Favourite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Favourite { pub file_name: String }               // struct, not String: phase 05 adds drive_file_id
```
`PersistedState` gains `sessions: Vec<Session>`, `active_session_id: Option<SessionId>` and `settings: Settings`, all `#[serde(default)]`.

`Settings { favourites_root: PathBuf }` defaults to `UserDirs::picture_dir()/"Photo Sorter"`, falling back to `home_dir()/Pictures/Photo Sorter`. Bump `schema_version` to 1.

### 2. Session name → folder name (`processor/sessions.rs`)

```rust
/// Pure (no I/O), so the New Session dialog can call it for a live preview.
pub fn session_name_to_folder_name(name: &str) -> AppResult<String>
```
- Trim, then replace `<>:"/\|?*` and control chars with `_`, and strip trailing dots and spaces (Windows).
- Reject empty results and Windows reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`, case-insensitive, with or without an extension) → `Invalid`.
- Cap at 100 chars. Names must be unique, case-insensitively, and two sessions may never share a `dir`.

The rule stays "UI doesn't touch the filesystem". Calling pure processor functions is allowed.

### 3. Commands and events (`core/`)

| Command | Processor behaviour | Event(s) |
|---|---|---|
| `CreateSession { name, dir: Option<PathBuf> }` | Validate. `dir` defaults to `favourites_root/<folder name>`. `create_dir_all`. Set it active. Save. | `SessionsChanged`, `FavouritesOfActive` |
| `InspectSessionDir(PathBuf)` | `{ exists, file_count }`, which lets the dialog warn "folder already has 23 files" | `SessionDirInspected` |
| `SetActiveSession(SessionId)` | Save. | `SessionsChanged`, `FavouritesOfActive` |
| `ForgetSession(SessionId)` | Remove it from the list only. **The folder and its files are never touched.** If it was active, clear the active session. | `SessionsChanged`, `FavouritesOfActive` |
| `ToggleFavourite(PathBuf)` | See step 4. | `FavouriteToggled` or `FavouriteFailed` |

`CreateSession` failures come back as `SessionCreateFailed(AppError)` instead of `Error`, so the dialog stays open and shows the problem inline.

```rust
pub struct SessionSummary { pub id: SessionId, pub name: String, pub dir: PathBuf, pub favourite_count: usize }
Event::SessionsChanged { sessions: Vec<SessionSummary>, active: Option<SessionId> }
Event::FavouritesOfActive { session_id: Option<SessionId>, sources: HashSet<PathBuf> }
Event::FavouriteToggled { source: PathBuf, favourited: bool }
Event::FavouriteFailed { source: PathBuf, error: AppError }
```
Before building `FavouritesOfActive`, **prune entries whose copied file no longer exists** (the user deleted or renamed it in Explorer), then save. The ★ always mirrors the disk.

Rename is skipped: forget plus create pointing at the same dir covers it.

### 4. Favourite toggle (`processor/favourites.rs`)

```rust
pub fn toggle_favourite(store: &mut Store, source: &Path) -> AppResult<bool /*now favourited*/>
```
The core actor processes commands one at a time, so rapid Enter presses on the same photo are serialized for free: no copy vs. trash race, and no locks.

**Favourite** (source not in `session.favourites`):
1. No active session → `Invalid("no active session")`. The UI prevents this case (step 6).
2. Session dir missing (deleted, or an unplugged drive) → `create_dir_all`. If that fails, return `Io`.
3. Destination name: `file_name`. If it's taken, try `stem (2).ext`, `stem (3).ext`… This handles two cameras both writing `IMG_0001.JPG`.
4. `fs::copy(source, dir/"<name>.partial")`, then `fs::rename` to `dir/<name>`. Drive for desktop and users never see a half-written file under its final name.
5. Insert the entry, then `store.save()`.
6. If the copy fails: remove the `.partial`, leave state unchanged, return the error.

**Un-favourite**:
1. `trash::delete(dir/file_name)`, which uses the OS recycle bin, never a hard delete. If the file is already gone, carry on. If trashing fails (e.g. Linux without a trash implementation), **keep the file**, remove the entry, and return a warning event "couldn't move X to trash; file kept".
2. Remove the entry, then `store.save()`.

<!-- ponytail: copies run on the core thread, so a 60 MB copy from an SD card delays the next command by ~1 s. Move copies to a dedicated worker only if users notice. -->

Deps: `trash = "5"`, `time` (RFC 3339 formatting).

### 5. UI state (`app.rs`)

```rust
pub struct SessionsView {
    pub sessions: Vec<SessionSummary>,
    pub active: Option<SessionId>,
    pub favourited: HashSet<PathBuf>,          // active session
    pub saving: HashSet<PathBuf>,              // toggles in flight
    pub favourite_after_create: Option<PathBuf>,
}
```
- **Optimistic toggle**: flip `favourited` immediately, add the path to `saving`, then send the command. `FavouriteToggled` → remove from `saving`. `FavouriteFailed` → revert, remove from `saving`, show a toast with the error.
- `FavouritesOfActive` replaces `favourited` wholesale.

### 6. UI pieces (per the picked mock)

- **★ button** on the viewer: filled when `favourited` contains the current photo, with a subtle spinner ring while it's in `saving`. Click → toggle. Hover tooltip shows the shortcut.
- **Session switcher**: the active session's name plus favourite count, the list of sessions, "New session… (Ctrl+N)" and "Forget session…". The forget confirm text says: "Files in <dir> are kept."
- **Open folder** button next to the session name → `opener::open(&session.dir)`, which opens Explorer/Finder/Files. This is the "access all favourites easily" requirement: one click, then drag to drive.google.com.
- **New Session dialog** (`egui::Modal`): the name `TextEdit` gets focus on the first frame. A live preview line `~/Pictures/Photo Sorter/<sanitized>` shows inline validation errors. "Change location…" opens `rfd` pick_folder. If `SessionDirInspected` reports existing files, show a warning. Enter creates, Esc cancels.
- **No active session plus favourite pressed** → set `favourite_after_create = Some(current)` and open the dialog. When the next `SessionsChanged` arrives with a new active session, send `ToggleFavourite` for it. A first-time user just presses Enter and names the event.

### 7. Keyboard

Add to `ui/keybindings.rs`: `ToggleFavourite` = `Enter` / `NumpadEnter`, and `NewSession` = `Modifiers::COMMAND + N` (egui's `COMMAND` is Cmd on macOS and Ctrl elsewhere). While a modal is open, viewer actions are suspended, and the modal owns Enter and Esc.

### 8. Tests

`tests/favourites.rs` (core flows, real filesystem in a tempdir, favourites root inside it):
1. **favourite_flow**: `CreateSession("Wedding")` → `OpenFolder(src)` → `ToggleFavourite(src/IMG_1.jpg)` → `FavouriteToggled{favourited:true}` → `root/Wedding/IMG_1.jpg` has identical bytes (hash), no `.partial` remains, and `state_on_disk()` lists the favourite.
2. **unfavourite_flow**: toggle again → the file is gone from the session dir and the entry is removed. Swap the trash for a test double: `processor::favourites` takes a `fn(&Path) -> io::Result<()>` trash function, and tests pass one that moves the file into `tempdir/trash`.
3. **collision**: two source dirs each with a different `IMG_0001.JPG` → both favourited → `IMG_0001.JPG` and `IMG_0001 (2).JPG`. Un-favouriting the second removes only `(2)`.
4. **multi_session**: favourite in A, switch to B → `FavouritesOfActive` is empty → favourite in B → both dirs have the file.
5. **no_active_session** → `FavouriteFailed(Invalid)`, nothing written.
6. **forget_keeps_files**, **name_validation** (`CON`, `"   "`, `a/b` → `a_b`, duplicates), **missing_session_dir_recreated**, **stale_favourite_pruned**, **copy_failure_leaves_no_trace** (unreadable source).

`tests/favourites_ui.rs` (`egui_kittest`, the whole user flow headless):
7. **first_enter_creates_session_and_favourites**: open a fixture folder → press Enter → the "New session" dialog is visible → type "Party" → press Enter → wait for events → `root/Party/<first photo>` exists, and the ★ button's accessibility label says "Favourited".
8. **ctrl_n_opens_dialog**: press `COMMAND+N` → the dialog is visible. Typing "n" inside it doesn't trigger shortcuts.
