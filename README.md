# Photo Sorter

Fast photo culling for event photographers. Open a folder, browse full-screen, press **Enter** to favourite. Favourites are copied into the folder of the active **session** (one per event) and, if you like, uploaded to every cloud service you switch on.

Native Rust (egui + wgpu), no web runtime. Windows 10/11, macOS 13+, Linux (X11 and Wayland).

## Features

- **Instant browsing.** JPEGs are decoded at screen size with libjpeg-turbo, neighbours are prefetched in parallel, and the embedded EXIF thumbnail shows at once on cold jumps, so you never see a blank screen.
- **Sessions.** One folder per event. Favouriting copies the photo there; un-favouriting moves the copy to the OS trash. Originals are never modified.
- **RAW support.** CR2, CR3, NEF, NRW, ARW, RAF, ORF, RW2, PEF, DNG, SRW are previewed from their embedded JPEG. RAW+JPEG pairs (and `.xmp` sidecars) are favourited together; turn that off in Settings if you only hand out JPEGs.
- **Rebindable shortcuts**, a scrub bar, and go-to-photo by number.
- **Cloud uploads** to Google Drive, OneDrive, Dropbox and Nextcloud/WebDAV, all at once. Switch each one on or off in Settings › Cloud uploads or from the upload chip in the bottom bar. Offline at the venue? Uploads wait and resume by themselves, even after a restart. Sign-ins live in the OS keychain, and files go straight from your computer to each service.
- **Auto-update** from GitHub Releases, at most one check a day; installing waits for you to click Restart.

## Shortcuts

`Cmd` on macOS, `Ctrl` elsewhere. All of them can be changed in Settings.

| Action | Key |
|---|---|
| Toggle favourite | `Enter` |
| Next / previous photo | `→` / `←` |
| First / last photo | `Home` / `End` |
| Go to photo number | `G` |
| New session | `Cmd+N` |
| Switch session | `Cmd+K` |
| Open folder | `Cmd+O` |

## Install

Download the installer for your OS from the [latest release](https://github.com/andrei399/photo-sorter/releases/latest): `.dmg` (macOS), `.exe`/`.msi` (Windows), `.AppImage`/`.deb`/pacman package (Linux).

## Build from source

Needs stable Rust, plus `cmake` and `nasm` for the vendored libjpeg-turbo.

```sh
# Debian/Ubuntu
sudo apt install cmake nasm pkg-config libxkbcommon-dev libwayland-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
# macOS
brew install cmake nasm
# Windows
choco install cmake nasm

cargo run --release
```

From WSL, `./build-windows.sh` builds a native Windows `.exe` through the host's MSVC toolchain.

Checks run in CI on all three OSes:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Releasing

Bump `version` in `Cargo.toml`, then push a matching `vX.Y.Z` tag. [`release.yml`](.github/workflows/release.yml) runs CI, builds signed installers for every OS and opens a **draft** release. Run [`plans/07-release/qa.md`](plans/07-release/qa.md) on the draft, then publish it; only published releases reach auto-update.

## Layout

```
src/
  main.rs, app.rs   eframe bootstrap and the app shell
  ui/               viewer, sessions, settings dialog, keybindings
  core/             Command/Event actor that owns the store
  processor/        folder scanning, sessions, favourites, settings
  imaging/          decoding, RAW previews, parallel prefetch
  models/           persisted state (one state.json in the OS data dir)
  integrations/     auto-updater, cloud upload worker (Drive, OneDrive, Dropbox, WebDAV)
tests/              whole-flow tests, including egui_kittest UI tests
plans/              design docs and per-phase plans
```

## Privacy

No servers, accounts or telemetry. See [PRIVACY.md](PRIVACY.md).

## License

Copyright © 2026 Tudor Andrei-Cristian.

Licensed under the [GNU General Public License v3.0 or later](LICENSE). The bundled Inter font is under the [SIL Open Font License](assets/fonts/Inter-LICENSE.txt).
