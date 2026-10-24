# 03 — Review

## Plan critique

**What's right**
- **Copy, not move.** The source (usually the SD card dump) stays untouched, so culling is non-destructive and can be redone.
- **Session folder = deliverable.** It contains only favourited photos and no app metadata, so it can be uploaded, zipped or shared as is.
- **The state records what we created**, so un-favourite removes exactly our file even after a collision rename, and never a user file with the same name.
- **The actor serializes toggles for free.** The Tauri version needed a mutex held across an async copy; here it's structural.
- **Optimistic UI with an explicit `saving` set**: the ★ responds within the frame, and failures revert visibly.
- **Trash, not delete**, and never a hard-delete fallback.
- **The kittest flow test covers the real user path** (key press → dialog → typing → file on disk) with no window. That's exactly the holistic test style we want.

**Risks / open questions**
| Risk | Mitigation / decision |
|---|---|
| Source keyed by absolute path, so moving the source folder makes the ★ show "not favourited" | Accepted. Re-favouriting creates `name (2).jpg`. Upgrade path: key by `(file_name, size, mtime)`. |
| A long copy (60 MB RAW from an SD card) blocks the core queue for ~1 s, e.g. `SetActiveSession` waits | The UI stays responsive because the change is optimistic. A ponytail note in code gives the upgrade path (a dedicated copy worker). |
| `trash` crate on Linux without a desktop trash spec | Keep the file, drop the entry, warn. Never hard-delete. |
| macOS TCC prompt on first write to `~/Pictures` | Expected, one time. Denial → clear error toast. |
| Pruning stale entries on every `FavouritesOfActive` costs one `exists()` per favourite | ~1 ms per 1,000 favourites on SSD. Fine. |

**Deliberately skipped**
- Session rename, merging sessions, notes per session, an in-app favourites grid (the OS file manager is the grid), preserving mtime on copy (EXIF capture time is what Drive and Photos sort by).

## Code review checklist

- [ ] The copy goes through `.partial` + rename, and `.partial` is removed on failure.
- [ ] Un-favourite only touches `session.dir/favourite.file_name` via the trash function. No `remove_file` anywhere in this path.
- [ ] State is saved only after the filesystem op succeeded.
- [ ] The sanitizer covers Windows reserved names and trailing dots and spaces. It's a pure function.
- [ ] `ForgetSession` has no filesystem side effects.
- [ ] Stale entries are pruned before sending `FavouritesOfActive`.
- [ ] The UI does an optimistic flip with a `saving` set, reverts on `FavouriteFailed`, and shows a toast.
- [ ] Modals suspend viewer shortcuts. `NewSession` uses `Modifiers::COMMAND`.
- [ ] The ★ has an accessibility label reflecting its state.
