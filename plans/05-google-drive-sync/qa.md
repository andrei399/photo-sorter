# 05 — QA

## Automated (wiremock, no real network)

| Test | Asserts |
|---|---|
| `upload_flow` | Folder bodies, resumable metadata, byte-exact uploads, ids on disk, final status. |
| `unfavourite_trashes_in_drive` | PATCH trashed=true, empty `pending_drive_trash`. |
| `retry_on_503_then_success` | Eventually uploaded, with backoff observed under paused time. |
| `offline_persists_queue` | Queue survives a restart and drains once the server is back. |
| `deleted_drive_folder_recreated` | New folder id, everything re-uploaded. |
| `auth_expired` | `NeedsReconnect`, zero uploads. |
| `unfavourite_during_upload` | The late id is trashed. No orphan. |
| `sync_disabled_session_ignored` | Zero Drive calls. |
| `quota_exceeded_not_retried` | `Error("Google Drive is full")`, no retry storm. |
| `double_sync_refused` | Error, sync stays off. |

## Manual — Path A (Windows, macOS)

| # | Step | Expected |
|---|---|---|
| A1 | Install Drive for desktop, open Settings → Storage | "Google Drive detected" with the correct path. |
| A2 | Accept, create "DriveTest", favourite 5 | Files appear in drive.google.com under `Photo Sorter/DriveTest` within ~1 min. |
| A3 | Un-favourite 1 | Removed from Drive (in Drive's trash). |
| A4 | Favourite a 60 MB file, watch Drive | No `.partial` remains in Drive. |
| A5 | Linux | No suggestion. "Use Drive sync (beta)" shown. |

## Manual — Path B (each OS, real account, production OAuth client)

| # | Step | Expected |
|---|---|---|
| B1 | Settings → Connect Google Drive | The system browser shows consent listing only "files you use with this app" (+ email). |
| B2 | Approve | Browser says "You can close this tab". App shows "Connected as …". |
| B3 | Deny | "Connection cancelled". No crash. |
| B4 | Close the tab without acting, wait 5 min | Times out cleanly. Button usable again. |
| B5 | Sync-on session, favourite 20 fast | Indicator counts 20 → 0. Drive has 20 files of identical size. Navigation never stutters. |
| B6 | Enable sync on a session with 50 favourites | All 50 upload. |
| B7 | Wi-Fi off, favourite 5, Wi-Fi on | Offline indicator, then it completes unattended. |
| B8 | Quit with uploads pending, reopen | Quit prompt. After reopening, uploads resume. |
| B9 | Delete the session folder in drive.google.com, favourite 1 | Folder recreated, all favourites re-uploaded. |
| B10 | Un-favourite an uploaded photo | It goes to Drive trash within one pass. |
| B11 | Revoke at myaccount.google.com/permissions | "Reconnect needed". Reconnecting resumes. |
| B12 | Disconnect | Keychain entry removed (Credential Manager / Keychain Access / seahorse). Drive files remain. |
| B13 | Session inside the Drive for desktop folder → enable API sync | Refused with an explanation. |
| B14 | **Token longevity**: connect, leave closed 8+ days, reopen | Syncs without re-login. |
| B15 | Linux without Secret Service | Clear keychain error. No token on disk. |
| B16 | Grep `state.json` and logs for tokens | None. |
| B17 | "Open in Drive" | Opens the right folder. |
