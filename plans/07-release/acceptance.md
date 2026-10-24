# 07 — Acceptance Criteria

- [ ] **AC1 — One-tag release**: Pushing a `v*` tag produces a draft GitHub Release with Windows (.exe, .msi), macOS (arm64 + x64 .dmg) and Linux (.AppImage, .deb, pacman) artifacts, plus `latest.json`, signatures and checksums.
- [ ] **AC2 — Trusted on macOS**: Signed and notarized. Opens on a clean Mac with no Gatekeeper warning.
- [ ] **AC3 — Trusted on Windows**: The installer is code-signed and shows the publisher name.
- [ ] **AC4 — Runs on clean machines**: Works on clean Windows 10/11 (including without GPU drivers), macOS 13+, Ubuntu 22.04/24.04 (X11 and Wayland) and Fedora 40.
- [ ] **AC5 — Still fast in release**: Packaged builds meet phase 02's performance targets, with SIMD enforced at build time.
- [ ] **AC6 — Auto-update**: A previous version detects the release, updates after the user clicks Restart, and keeps all sessions and settings. It never restarts on its own.
- [ ] **AC7 — Update integrity**: Updates with invalid signatures are rejected.
- [ ] **AC8 — Gate**: Updaters don't see a release until a human publishes the draft after the smoke test.
- [ ] **AC9 — Privacy**: A public privacy policy is linked from the OAuth consent screen and the app. No telemetry.
- [ ] **AC10 — No secrets in repo**: Signing keys and OAuth credentials live only in CI secrets. The updater private key is backed up offline.
