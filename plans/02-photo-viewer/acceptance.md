# 02 — Acceptance Criteria

- [ ] **AC0 — Gates passed**: Three viewer mocks were published and Andrei picked one. The decoder spike numbers and decision are recorded.
- [ ] **AC1 — Open folder**: Given the app, when I click "Open folder", press Ctrl/Cmd+O or drop a folder (Win/mac/X11), then the first photo in natural filename order fills the window, aspect-fit, with the filename and `1 / N` shown.
- [ ] **AC2 — Navigate**: → / ← show the next/previous photo, Home/End jump to first/last, and navigation stops (no wrap) at the ends.
- [ ] **AC3 — Instant**: Given sequential browsing on an SSD, the next photo appears in the next frame. Never a blank frame, including while holding an arrow key.
- [ ] **AC4 — Cold jumps**: Given a jump to a photo that isn't prefetched, the EXIF-thumbnail glimpse appears within a frame and the sharp version replaces it within ~130 ms (24 MP out-of-camera) / ~300 ms (45–60 MP).
- [ ] **AC5 — Decode speed**: The benchmark meets the targets in qa.md on native Windows, macOS and Linux.
- [ ] **AC6 — Correct pixels**: EXIF-rotated photos display upright. Photos are sharp on HiDPI screens. Images are never upscaled beyond 100 %.
- [ ] **AC7 — Formats**: JPEG, PNG, WebP, GIF (first frame), BMP and TIFF display. Extensions are matched case-insensitively and content is detected by magic bytes.
- [ ] **AC8 — Robust**: Corrupt or unreadable files show a "can't display" message, and navigation keeps working. No crash on any input.
- [ ] **AC9 — Bounded**: Memory plateaus (≤ 250 MB at 1440p) no matter how many photos I browse. CPU is ~0 % when idle.
- [ ] **AC10 — Remembers**: The last folder reopens on launch. If it's missing, the start screen shows without an error.
