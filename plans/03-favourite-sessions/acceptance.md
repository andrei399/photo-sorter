# 03 — Acceptance Criteria

- [ ] **AC0 — Design picked**: Session switcher, ★ states and New Session dialog mocks were published and picked before implementation.
- [ ] **AC1 — Favourite by key**: Given an active session and a photo on screen, when I press Enter, then the ★ fills in the same frame and the photo is copied into the session folder.
- [ ] **AC2 — Favourite by button**: Same as AC1 via clicking the on-screen ★.
- [ ] **AC3 — Toggle off**: Pressing Enter or ★ on a favourited photo moves the copy to the OS trash (never hard-deleted) and empties the ★.
- [ ] **AC4 — New session**: Ctrl+N (Cmd+N on macOS) opens a dialog. Confirming a name creates `<favourites root>/<name>` and makes it the active session.
- [ ] **AC5 — First-run flow**: With no session, pressing Enter opens the New Session dialog, and after confirming, that photo is favourited into the new session.
- [ ] **AC6 — Separate folders**: Favourites go only into the active session's folder, and the ★ reflects the active session.
- [ ] **AC7 — Easy access**: "Open folder" opens the session folder in the OS file manager. It contains only favourited photos and no app files.
- [ ] **AC8 — Collisions**: Two different photos with the same filename can both be favourited without overwriting (`name (2).ext`).
- [ ] **AC9 — Persistence**: Sessions, the active session and ★ states survive a restart. The ★ reflects the disk (externally deleted copies show as not favourited).
- [ ] **AC10 — Safety**: Forgetting a session never deletes files. The app only ever removes files it copied itself, and only via the trash. No half-written file ever appears under a final name.
- [ ] **AC11 — Never blocks**: Favouriting large files from slow media never delays navigation.
- [ ] **AC12 — Invalid names**: Names illegal on any of the 3 OSes are rejected or sanitized, with a live folder-name preview.
