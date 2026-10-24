# 07 — QA

Run on every draft release, before publishing. Use **clean VMs or machines** that never had a dev build.

## Install matrix

| # | Platform | Artifact | Check |
|---|---|---|---|
| 1 | Windows 11 | `.exe` (NSIS) | Installs per-user. Signed publisher shown. Start menu entry. |
| 2 | Windows 10 VM (no GPU drivers) | `.exe` | Runs via the glow fallback. |
| 3 | Windows 11 | `.msi` | Installs, uninstalls cleanly. |
| 4 | macOS 14/15 Apple Silicon | `.dmg` (aarch64) | No Gatekeeper warning. `spctl -a -vv "/Applications/Photo Sorter.app"` → accepted, Notarized Developer ID. |
| 5 | macOS 13 Intel | `.dmg` (x64) | Same as 4. |
| 6 | Ubuntu 24.04 (GNOME Wayland) | `.deb` | `sudo apt install ./photo-sorter_*.deb`, launches from the menu. |
| 7 | Ubuntu 22.04 (X11) | `.AppImage` | `chmod +x`, runs. |
| 8 | Fedora 40 (KDE Wayland) | `.AppImage` | Runs. |
| 9 | Arch | pacman package | Installs, runs (best effort). |

## Smoke test per install (≈ 10 min)

1. Cold start < 300 ms to window. Open `qa-photos` → hold → for 5 s → smooth, never blank (incl. RAW if phase 06 shipped).
2. Enter → New Session dialog → create → favourite 3 → Open folder shows 3 files.
3. Ctrl/Cmd+N → second session → favourite 1 → switch back: the ★ states are correct.
4. Settings → rebind favourite to F → works → restart → still F.
5. Drive (Path B build with credentials): connect → favourite → the file appears in Drive.
6. macOS: open a folder on an SD card → the TCC prompt shows our usage text.
7. `decode_bench` on the release binary's machine matches phase 02's targets (SIMD is build-enforced by `require-simd`).
8. Uninstall → the app is removed. `state.json` remains in the data dir (documented).

## Updater test

1. Install the previous published version (vN-1).
2. Publish the draft vN.
3. Launch vN-1 → the notice appears → Restart to update → relaunches as vN (About box) with sessions intact.
4. Tamper test (once): point at a `latest.json` with a wrong signature → update refused.
5. Pending Drive uploads → Restart warns first. No forced restart.

## Release sign-off

- [ ] Install matrix rows 1–8 pass
- [ ] Smoke test passes on Win 11, macOS arm64 and Ubuntu 24.04 at minimum
- [ ] Updater test passes on at least one OS (AppImage, NSIS or .app)
- [ ] Artifact sizes recorded (target < 25 MB each)
- [ ] Checksums attached
