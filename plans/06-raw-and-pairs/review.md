# 06 — Review

## Plan critique

**What's right**
- **The embedded preview through the existing JPEG pipeline**: RAW support is "find the bytes", not a second imaging stack. It inherits DCT scaling, glimpse and prefetch.
- **Range reads** are the native-only win: 2–6 MB read instead of 60 MB. On an SD card that's ~100 ms versus ~1.5 s per RAW.
- **Spike first with written criteria**, because RAW is a long tail of container quirks.
- **Grouping by stem** with a shared collision suffix keeps pairs and XMP edits intact for Lightroom.
- **The server re-derives companions**, so the UI can't trick the core into copying arbitrary files.

**Risks**
| Risk | Mitigation |
|---|---|
| Some RAWs embed small previews (older Nikons ~640 px) or none (some phone DNGs) | The full-decode fallback with glimpse first. The spike table lists the preview size per camera. |
| CR3 quirks across bodies (R5, R6, M50, R8) | 3+ Canon bodies in the samples. CR3 is the most common RAW at events today. |
| A hand-rolled parser panics on hostile or corrupt input (bad offsets) | Every offset and length is bounds-checked. A quick fuzz pass with truncated and bit-flipped files. Run `cargo fuzz` once on the extractor. |
| Same stem but different shots (two cameras in one folder) | Mis-grouped. Accepted, since cameras write separate folders. Upgrade: compare EXIF capture time. |
| Uploading RAWs to Drive eats quota | Off by default, per session. |
| rawler's binary or compile-time cost | Measured in the spike. The hand-rolled extractor avoids it entirely if it meets the criteria. |

**Deliberately skipped**
- HEIC (C dependency), video, RAW editing, reading star ratings or colour labels from XMP, an on-disk preview cache.

## Code review checklist

- [ ] The spike decision and numbers are in `plan.md`.
- [ ] The extractor seeks and range-reads, never `fs::read` of the whole RAW.
- [ ] Every offset and length from the file is bounds-checked. No panics on fuzzed input.
- [ ] RAW orientation comes from the RAW's IFD0 and is passed as an override.
- [ ] Companions are re-derived on the core. Collision suffixes are shared by the whole group.
- [ ] Group copy is all-or-nothing, with rollback through the trash function.
- [ ] `copy_companions` and the per-session "upload RAW" option default as specified.
