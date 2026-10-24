# 02 — Review

## Plan critique

**What's right**
- **DCT-scaled decoding straight into the texture buffer** gives about 4× less memory and upload work, and 1.5–2.3× faster decodes than full decode + resize. The spike showed Huffman decoding is the floor, so **prefetch + glimpse** carry perceived speed, not the decoder alone.
- **Workers create the textures themselves.** The UI thread never touches pixel buffers, so frame time stays under 2 ms regardless of photo size.
- **Never-blank rule**: keep the previous photo until something new is ready, and show the EXIF-thumbnail glimpse (0.1 ms, 64 KB read) on a cold miss. Perceived speed matters more than measured speed.
- **A spike with written criteria** before committing to a C dependency (libjpeg-turbo needs cmake + nasm).
- **Memory is bounded by the window size**, not by the photo's resolution or the folder's size.

**Risks**
| Risk | Likelihood | Mitigation |
|---|---|---|
| libjpeg-turbo build friction (nasm missing → non-SIMD build, 3× slower) | Medium | The `require-simd` feature makes a build without nasm **fail** instead of silently degrading. `pkg-config` is off, so the vendored lib is always used. CI installs nasm. |
| Held arrow at 30 Hz key-repeat outruns full decoding (100–260 ms per big file; the spike proved entropy decoding is the floor) | High for big files | Glimpses are prefetched 12 ahead, so a held arrow shows a soft stream that sharpens when you stop (Lightroom-style). Stale-skip keeps the pool on what's near the cursor. Upgrade paths: a priority heap, or parallel Huffman decoding over restart markers for files that have them. |
| FIFO pool queueing: after a big jump, the new current job waits behind stale ones | Low | Stale jobs exit in microseconds (the index check runs before any I/O). |
| Slow storage (SD card reader at 40 MB/s, NAS) → reading a 15 MB file takes 400 ms | Medium | Prefetch hides it for sequential browsing. Glimpse doesn't help, because the read dominates. Accepted. Document "copy to SSD first" as a tip. |
| EXIF orientation edge cases (orientation tag in XMP only, or MakerNotes) | Low | Use the standard IFD0 tag only. All 8 orientations are covered by an in-test EXIF test. |
| Wayland has no drag-and-drop (winit limitation) | Certain on Wayland | Button plus `Mod+O`. Revisit when winit adds it. |
| egui `TextureOptions::LINEAR` downsampling (≤ 1.25×) aliasing on fine patterns | Low | Barely visible at ≤ 1.25×. If needed, enable mipmaps in the texture options or add a resize step. |
| `rfd` blocking dialog freezes rendering behind it | Expected | It's modal, so there's nothing to render. On Linux, rfd uses the XDG portal (needs `xdg-desktop-portal` running, which is standard on GNOME/KDE). Document it for bare WMs. |

**Deliberately skipped**
- Zoom and pan (the focus-check bold idea in the README), slideshow, delete, rotate, recursive scan, EXIF-date sort, file watching, animated GIF (first frame only), AVIF/HEIC.

## Code review checklist

- [ ] The spike numbers and decision are recorded in `plan.md`.
- [ ] Format sniffing is by magic bytes. Extension matching is case-insensitive. Hidden and `._*` files are skipped.
- [ ] JPEG decodes directly to RGBA at a DCT scale ≥ fit. No full decode followed by a resize for JPEG.
- [ ] Never upscale. Clamped to `max_texture_side`.
- [ ] `load_texture` runs on workers. The UI thread does no pixel work.
- [ ] The stale-skip check happens before `fs::read`.
- [ ] Eviction drops `TextureHandle`s outside the window, so the cache is bounded.
- [ ] Glimpse and full are separate slots. A full decode for an outdated viewport size never replaces a newer one.
- [ ] No blank frames: the previous photo stays until the new one is ready.
- [ ] Shortcuts go through `ui/keybindings.rs`, and are skipped while a text field has focus.
- [ ] Decoding errors become a `Failed` entry. Nothing panics on corrupt files.
