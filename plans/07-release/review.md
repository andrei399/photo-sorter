# 07 — Review

## Plan critique

**What's right**
- **cargo-packager** produces every installer format from a plain Rust crate, with signing, notarization and a matching signed updater. No hand-rolled packaging scripts per OS.
- **Native means trivial runtime dependencies**: no WebView2 bootstrapper and no webkitgtk version matrix. The AppImage is close to self-contained.
- **Draft releases plus a human smoke test** before publishing. Auto-update spreads a broken build to everyone, so a 10-minute gate is cheap.
- **Building on ubuntu-22.04** gives the oldest glibc, so the binaries run on more distros.
- **Never auto-restart**: respects a photographer mid-cull and pending uploads.

**Risks**
| Risk | Mitigation |
|---|---|
| cargo-packager is smaller and less battle-tested than tauri-bundler (from which it was derived) | Pin the version. If a format breaks, fall back per format: `cargo-bundle` for .app/.deb, `cargo-wix` for MSI, `create-dmg` script. |
| Config key and env var names drift between cargo-packager versions | Step 1 says to verify against the docs. CI catches breakage on every tag. |
| Updater private key lost | Offline backup (password manager + encrypted USB). Owner: Andrei. |
| Notarization delays or failures | Runs in CI. Re-run the job on failure. Allow ~15 min. |
| SmartScreen warnings persist for new publishers even when signed | Reputation builds over downloads. Trusted Signing helps. |
| AppImage needs FUSE 2 on Ubuntu 24.04+ | README: `sudo apt install libfuse2t64`, or use the `.deb`. |
| GPU driver issues on user machines (old Intel, VMs) | The glow fallback from phase 01. `PHOTO_SORTER_RENDERER=glow` documented in the README's troubleshooting section. |
| No rpm | AppImage covers Fedora. Add `cargo-generate-rpm` on demand. |

**Deliberately skipped**
- Microsoft Store, Mac App Store (sandboxing fights arbitrary folder access), Flatpak/Snap (Flatpak if Linux users ask; portal file access needs testing), Homebrew/winget (cheap to add later), universal macOS binary (two builds are simpler), telemetry and crash reporting (contradicts the privacy stance).

## Code review checklist

- [ ] No secrets in the repo. Updater pubkey committed, private key not.
- [ ] The release workflow runs fmt, clippy and tests before packaging.
- [ ] Releases are drafts by default. `latest.json` is assembled only after all matrix jobs succeed.
- [ ] Tag/version match check.
- [ ] macOS usage-description strings present.
- [ ] The update check is rate-limited (≤ 1/day), runs in the background, and never auto-restarts.
