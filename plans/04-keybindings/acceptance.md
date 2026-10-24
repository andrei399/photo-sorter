# 04 — Acceptance Criteria

- [ ] **AC0 — Design picked**: The Settings dialog mock was picked before implementation.
- [ ] **AC1 — Defaults**: On a fresh install, Enter/NumpadEnter = favourite, ←/→ = previous/next, Home/End = first/last, Ctrl+N / Cmd+N = new session, Ctrl+K / Cmd+K = quick session switcher, Ctrl+O / Cmd+O = open folder.
- [ ] **AC2 — Rebind**: In Settings I can add, remove or replace keys for any action. After saving, the new binding works immediately and persists across restarts.
- [ ] **AC3 — No conflicts**: A combo can't be assigned to two actions, and every action keeps at least one binding. Errors show before saving.
- [ ] **AC4 — Layout-proof**: Bindings follow the physical key and keep working after switching keyboard layout (where the OS reports physical keys).
- [ ] **AC5 — Cross-platform modifier**: Bindings using the platform modifier show ⌘ on macOS and Ctrl elsewhere, and work on each.
- [ ] **AC6 — Typing safe**: While a text field or dialog has focus, viewer shortcuts don't fire.
- [ ] **AC7 — Discoverable**: The ★, the session menu and buttons show their current bindings.
- [ ] **AC8 — Favourites root**: I can change where new session folders are created. Existing sessions are unaffected. Unwritable locations are rejected with a clear message.
- [ ] **AC9 — Recovery**: "Reset to defaults" restores defaults, and Ctrl/Cmd+, always opens Settings.
