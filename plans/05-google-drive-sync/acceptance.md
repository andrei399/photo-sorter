# 05 — Acceptance Criteria

## Path A (must ship)

- [ ] **A-AC1 — Detect**: On Windows and macOS with Drive for desktop installed, Settings offers the Drive folder as the favourites root in one click.
- [ ] **A-AC2 — Works**: With the favourites root inside the Drive folder, new sessions and favourites appear in Google Drive in a folder named after the session, with no further action.
- [ ] **A-AC3 — Clean**: No temporary or partial files end up in Drive.
- [ ] **A-AC4 — Documented**: The README explains Path A setup, including the Linux rclone alternative.

## Path B (native sync)

- [ ] **B-AC0 — Design picked**: Sync indicator and connect flow mocks were picked before implementation.
- [ ] **B-AC1 — Connect**: "Connect Google Drive" signs me in via my system browser and shows my email. Only the `drive.file` permission (plus email) is requested.
- [ ] **B-AC2 — Auto-upload per session**: With "Sync to Drive" on, favourites upload to `My Drive/Photo Sorter/<session name>/` without ever slowing the viewer.
- [ ] **B-AC3 — Un-favourite mirrors**: Un-favouriting an uploaded photo moves it to the Drive trash.
- [ ] **B-AC4 — Resilient**: Uploads survive offline periods, restarts and transient Google errors, and finish automatically. No duplicates under normal operation.
- [ ] **B-AC5 — Visible**: Per-session progress plus clear offline / reconnect / Drive-full states.
- [ ] **B-AC6 — Long-lived login**: Still syncs after 8+ days without re-login.
- [ ] **B-AC7 — Private**: Tokens live only in the OS keychain. Only Google's servers are contacted. Disconnect revokes the token.
- [ ] **B-AC8 — No double sync**: API sync can't be enabled for a session inside a Drive for desktop folder.
- [ ] **B-AC9 — Tested**: All wiremock flow tests pass in CI on all 3 OSes.
