# 05 — Review

## Plan critique

**What's right**
- **Path A first.** An hour of work covers most Windows and Mac users with Google's own sync engine (resume, offline, throttling).
- **The `drive.file` scope**: least privilege, non-sensitive classification, and an easy sell to privacy-minded users.
- **The state is the queue, and the core is the only writer.** The worker never touches `Store`. It proposes results, and the core re-checks against current truth (the un-favourite-mid-upload race is handled in one place). No locks anywhere.
- **Tokio is confined to the integration.** The rest of the app stays simple synchronous threads.
- **Trash, not delete**, in Drive too.

**Risks**
| Risk | Likelihood | Mitigation |
|---|---|---|
| App left in "Testing" → refresh tokens die after 7 days | High if forgotten | Step 1.3, plus an acceptance criterion that a token works after 8+ days. |
| "Unverified app" warning until brand verification | Medium | Verify early with the Nodus Veritatis domain, homepage and privacy policy. |
| Crash between upload success and saving the id → duplicate on next run | Low (ms window) | Accepted. Upgrade: set `appProperties.sourceKey` on upload and query it before re-uploading. |
| Plans resent on every mutation get large (thousands of favourites) | Low | Plans include only pending work (ids are `None`), so they're tiny in steady state. |
| User deletes the Drive folder | Medium | `FolderMissing` → recreate and re-upload everything. |
| Path A + Path B on the same session → double uploads | Medium | Refused in `SetSessionDriveSync`. |
| Linux without Secret Service → `keyring` fails | Medium | A clear message: "install gnome-keyring or KWallet". Never fall back to a plaintext token file. |
| Uploads restart from zero on failure | Low (5–60 MB files) | ponytail note gives the Content-Range resume as the upgrade path. |
| Client secret extractable from the binary | Expected | Google's model for desktop apps. PKCE protects the flow. |

**Deliberately skipped**
- Two-way sync, choosing an existing Drive folder (needs the web-only Google Picker under `drive.file`), multiple accounts, bandwidth limits and pause.

## Code review checklist

- [ ] Only the `drive.file`, `openid` and `email` scopes are requested.
- [ ] PKCE S256 and `state` verification. The loopback binds `127.0.0.1`, has a timeout, and accepts one request.
- [ ] The refresh token exists only in the keychain: not in `state.json`, logs or error strings.
- [ ] Credentials come from `option_env!` and aren't in the repo.
- [ ] The worker never touches `Store`. Every state change goes through `apply_drive_result` on the core.
- [ ] `Uploaded` re-checks the favourite before saving the id. Orphans go to `pending_drive_trash`.
- [ ] Backoff is capped and jittered. Non-retryable 4xx errors aren't retried.
- [ ] Uploads stream from disk with no whole-file reads.
- [ ] Base URLs are injectable. CI makes no real network calls.
- [ ] The double-sync guard is present. The close-with-pending prompt works.
