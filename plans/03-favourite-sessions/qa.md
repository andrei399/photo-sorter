# 03 — QA

## Automated (all 3 OSes in CI)

| Test | Asserts |
|---|---|
| `favourite_flow` | Copy has identical bytes, no `.partial`, state persisted on disk. |
| `unfavourite_flow` | Copy moved via the trash function, entry removed. |
| `collision` | `IMG_0001 (2).JPG`. Un-favouriting removes only that file. |
| `multi_session` | Independent per-session state, both dirs get copies. |
| `no_active_session` | `FavouriteFailed(Invalid)`, no side effects. |
| `forget_keeps_files` | Dir and files intact. |
| `name_validation` | Reserved names rejected, chars replaced, duplicates rejected. |
| `missing_session_dir_recreated` | Dir recreated on favourite. |
| `stale_favourite_pruned` | Externally deleted copy → not in `FavouritesOfActive`, and the entry is gone from disk state. |
| `copy_failure_leaves_no_trace` | No `.partial`, no entry. |
| `first_enter_creates_session_and_favourites` (kittest) | Full UI flow → file on disk, ★ label "Favourited". |
| `ctrl_n_opens_dialog` (kittest) | Dialog opens, and typing doesn't trigger shortcuts. |

## Manual (each OS)

| # | Step | Expected |
|---|---|---|
| 1 | Fresh install, open a folder, press **Enter** | The New Session dialog opens with the name field focused. Type "Test Event", Enter → session created, photo favourited, ★ filled. |
| 2 | Check `<Pictures>/Photo Sorter/Test Event/` | Contains the photo, same size. |
| 3 | → then Enter ×5 quickly | ★ fills instantly each time. 5 more copies. |
| 4 | Back to a favourited photo, Enter | ★ empties. The file is in the Recycle Bin / Trash / `~/.local/share/Trash/files`. |
| 5 | Click the on-screen ★ | Same as Enter. |
| 6 | **Ctrl+N** (Win/Linux) / **Cmd+N** (mac) | The New Session dialog opens. |
| 7 | Name `CON` → inline error. Name `My/Event:2` | Preview shows `My_Event_2`. |
| 8 | Create "Second", favourite 2, switch back | The ★ states are correct per session. Counts are correct in the switcher. |
| 9 | Click "Open folder" | The OS file manager opens on the session dir. |
| 10 | Hammer Enter 10× on one photo | Ends consistent (even presses = not favourited). The dir matches the ★. No `.partial`. |
| 11 | Favourite a 60 MB file from an SD card, then immediately → → → | Navigation stays instant. The ★ shows saving, then saved. |
| 12 | Delete a copy in Explorer, then switch sessions back and forth | That photo's ★ is off. |
| 13 | Unplug the drive holding the session dir, favourite | Error toast. ★ reverts. |
| 14 | Forget session | The confirm text says files are kept. The folder still exists. |
| 15 | Restart | Sessions, active session and ★ states restored. |
| 16 | Two folders with `IMG_0001.JPG` (different photos), favourite both | `IMG_0001.JPG` and `IMG_0001 (2).JPG`. |
| 17 | New session in an existing non-empty folder | Warning with the file count. Existing files untouched. |
| 18 | macOS first favourite into `~/Pictures` | TCC prompt. Denial → clear error. |
| 19 | In the name field, press ← → and type "n" | Cursor moves, text typed. No navigation, no dialogs. |
| 20 | Screen reader on the ★ | Announces "Favourite" / "Favourited". |
