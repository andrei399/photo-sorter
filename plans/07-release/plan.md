# 07 — Release & Distribution

Goal: tagging `v*` produces signed installers for Windows, macOS (Apple Silicon + Intel) and Linux on a GitHub Release, and installed apps update themselves.

Tooling: **cargo-packager** (CrabNebula; formats: NSIS `.exe`, WiX `.msi`, `.app`, `.dmg`, `.deb`, `.AppImage`, pacman) plus the **cargo-packager-updater** crate. Exact env var and config key names below follow cargo-packager's docs. **Verify each against the current docs** during step 1, because they evolve.

## Steps

### 1. Packaging config (`Cargo.toml` → `[package.metadata.packager]`)

```toml
[package.metadata.packager]
product-name = "Photo Sorter"
identifier = "com.nodusveritatis.photosorter"
publisher = "Nodus Veritatis"
icons = ["assets/icon.png"]                 # 1024×1024 source; packager derives .ico/.icns
before-packaging-command = "cargo build --release"
formats = ["nsis", "wix", "app", "dmg", "deb", "appimage"]

[package.metadata.packager.macos]
minimum-system-version = "13.0"
# Info.plist usage strings: the text shown in the TCC prompt when opening a folder on Desktop/Documents/SD card
# NSDesktopFolderUsageDescription / NSDocumentsFolderUsageDescription / NSRemovableVolumesUsageDescription =
#   "Photo Sorter reads the photos you choose to browse."

[package.metadata.packager.deb]
depends = ["libxkbcommon0", "libwayland-client0"]   # GPU drivers come from the system (Mesa/vendor)
```
- Version: `Cargo.toml` `version` is the single source of truth. The app reads it via `env!("CARGO_PKG_VERSION")` for the About box.
- No WebView2 or webkitgtk, so the Linux and Windows dependency stories are trivial compared with the Tauri plan.
- Fedora/RHEL: ship the AppImage (cargo-packager has no rpm). Add an rpm via `cargo-generate-rpm` only if Fedora users ask.

### 2. Code signing secrets (GitHub → Settings → Secrets)

**macOS** (required, since unsigned apps are blocked by Gatekeeper):
- Apple Developer Program ($99/yr) → "Developer ID Application" certificate → export `.p12`.
- Secrets: `APPLE_CERTIFICATE` (base64 p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, plus notarization: `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID`.
- Hardened runtime is on by default. The app needs no extra entitlements (no JIT, no camera). Network is only used by Drive sync, and outgoing connections need no entitlement outside the sandbox.

**Windows** (strongly recommended, otherwise SmartScreen shows "unknown publisher"):
- **Azure Trusted Signing** (~$10/month, no hardware token). Configure the packager's Windows `sign-command` to call `trusted-signing-cli -e <endpoint> -a <account> -c <profile> %1`.
- Secrets: `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`.

**Linux**: no signing. Publish SHA256 checksums.

**Google OAuth** (phase 05): `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` set as env vars for `cargo build`.

### 3. Auto-update (`cargo-packager-updater`)

1. `cargo packager signer generate`. Store the private key and its password as secrets (`CARGO_PACKAGER_SIGN_PRIVATE_KEY`, `CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD`), and **back the key up offline. Losing it means existing installs can never update again.**
2. Embed the public key in the app (`const UPDATER_PUBKEY: &str = include_str!("../assets/updater.pub")`).
3. CI produces signed update artifacts. A small CI step assembles `latest.json` (version, notes, pub_date, and per-platform `url` + `signature`, in the format the updater crate expects) and attaches it to the release.
4. In-app: on startup, at most once per 24 h, a background thread calls `cargo_packager_updater::check_update(current_version, config)` against `https://github.com/<org>/photo-sorter/releases/latest/download/latest.json`. If there's an update, show a non-blocking top-bar notice: "Update to vX available — Restart to update". The download runs in the background. **Never auto-restart**, and warn if Drive uploads are pending.
5. Linux: only the AppImage self-updates. `.deb` users get the notice with a link to the release page.

### 4. Release workflow (`.github/workflows/release.yml`)

Trigger: `push: tags: ['v*']`. Matrix:
| runner | target |
|---|---|
| `macos-latest` | `aarch64-apple-darwin` |
| `macos-latest` | `x86_64-apple-darwin` |
| `ubuntu-22.04` | `x86_64-unknown-linux-gnu` (oldest supported glibc → widest compatibility) |
| `windows-latest` | `x86_64-pc-windows-msvc` |

Steps:
1. checkout → rust toolchain + target → rust-cache → cmake + nasm (+ Linux deps)
2. `cargo fmt --check && cargo clippy -- -D warnings && cargo test` (don't ship red)
3. `cargo install cargo-packager --locked` → `cargo packager --release --target <target>` with all secrets in env
4. Upload the artifacts plus `.sig` files with `softprops/action-gh-release` to a **draft** release named after the tag.
5. A final `needs:`-all job assembles `latest.json` and checksums and attaches them to the draft.

A human runs qa.md on the draft artifacts, then publishes. Publishing is what exposes `latest.json` to updaters.

### 5. Versioning and changelog

- Conventional commits (as for MR titles). Bump the `Cargo.toml` version in the release PR, then tag `v<version>`. CI fails if the tag doesn't match `CARGO_PKG_VERSION`.
- Release notes: GitHub's auto-generated notes from merged PRs.

### 6. Privacy policy (required for Google OAuth production status, phase 05)

One static page on the Nodus Veritatis domain: no servers, no data collection, photos go only to the user's own Google Drive when sync is enabled, and tokens live in the OS keychain. Link it from the OAuth consent screen and the app's About box.

### 7. Website / download page

Skipped. The GitHub Releases page is enough until there's demand.
