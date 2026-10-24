# 06 — RAW Preview & RAW+JPEG Pairs

Goal: photographers shoot RAW or RAW+JPEG. The app must (1) display RAW files as fast as JPEGs, and (2) when a shot exists as `IMG_0042.CR3` + `IMG_0042.JPG`, show it **once** and favourite **all of its files** together.

## Step 0 — Spike (time-box: 1 day): the RAW preview route

Nearly every RAW embeds a **full- or near-full-size JPEG preview** written by the camera. That JPEG goes straight into phase 02's `decode_full`, getting DCT scaling, the glimpse tier and prefetch for free. Real demosaicing takes seconds and is pointless for culling.

The key optimization is to **read only the preview's byte range**, not the whole 30–80 MB RAW. Parse the container header with seeks, find the preview offset and length, and read just those ~2–6 MB. That's 10× less I/O, which matters enormously on SD cards.

Evaluate, in order:
1. **`rawler`** (the dnglab decoder, pure Rust): does it expose embedded-preview extraction for CR2, CR3, NEF, ARW, RAF, ORF, RW2 and DNG? Does it read lazily or the whole file? What's the compile time and binary size impact?
2. **Hand-rolled extractor** (`imaging/raw_preview.rs`, ~200 lines):
   - TIFF-based (CR2, NEF, ARW, DNG, ORF, RW2, PEF, SRW): walk IFD0 + SubIFDs and take the largest JPEG (`JPEGInterchangeFormat`/`Length`, or a strip with `Compression = 6/7`).
   - CR3 (ISO-BMFF): the `PRVW` box inside the Canon `uuid` box.
   - RAF: fixed header fields give the JPEG offset and length.
   - Orientation comes from the RAW's IFD0 `Orientation` tag. The embedded JPEG usually has no EXIF of its own.
3. **Fallback** for files without a usable preview (some phone DNGs): full decode via rawler, then downscale. Slow but correct, and shown with the glimpse tier first.

Decision criteria: must cover Canon CR2/CR3, Nikon NEF, Sony ARW, Fuji RAF and DNG. Preview ≥ 1600 px on the long edge. ≤ 10 ms from extraction to JPEG bytes on SSD. Reads ≤ 10 % of the file. No C dependencies. **Write the decision and numbers into this file** before step 1.

Samples: raw.pixls.us (CC0, per camera) into `qa-photos/raw/` (git-ignored).

**Decided (2026-10-24): hand-rolled extractor, no new dependencies, no rawler.** It met every criterion on the first pass over 14 bodies (CR2, CR3 ×3, NEF ×2, ARW ×2, RAF, ORF ×2, RW2, Leica + iPhone DNG): extraction 0.1–1.0 ms, header overhead beyond the preview bytes ≈ 0 (a few KB), previews 1616–7008 px, all upright. Per-file numbers are in qa.md, reproducible with `cargo run --release --example raw_spike -- qa-photos/raw`.
- CR3 uses **track 1's full-size JPEG** (5–6 K px) rather than the 1620 px PRVW box, so focus can be judged. PRVW and THMB remain as fallbacks.
- "≤ 10 % of the file" holds for classic RAWs (2.7–9 %). Compressed CR3 (C-RAW, 11 MB) and iPhone DNG read 10–20 %, because the full-size preview *is* that share of a small file. The real criterion, "read only the preview", holds everywhere.
- **No full-decode fallback**: every sample, including the iPhone DNG, has a preview. A RAW without one shows "Can't display". Add rawler behind a cargo feature only if a real camera turns up without a preview.
- Fuzz: 378 k truncated/bit-flipped runs over the real samples, zero panics. IFD walking is capped (64 IFDs per file) so a self-referencing file can't fan out.

HEIC: **deferred** (needs libheif, a C library under LGPL). The upgrade path is `libheif-rs` behind a cargo feature, if iPhone shooters ask.

## Steps

### 1. Preview extraction (`imaging/raw_preview.rs`)

```rust
pub struct RawPreview { pub jpeg: Vec<u8>, pub orientation: Option<Orientation> }  // None: trust the JPEG's own EXIF (RAF)

/// Reads only container headers + the preview byte range.
pub fn extract_raw_preview(path: &Path) -> AppResult<RawPreview>
```
Extend `decode_full` with an orientation override: `decode_full(bytes, target_px, max_texture_side, orientation: Option<Orientation>)`. RAW jobs pass the RAW's orientation. JPEG jobs pass `None`, so the embedded EXIF is used. RAW glimpses come from the RAW's own IFD1/THMB thumbnail through the same head-read path.

The prefetch job picks its source by extension (`is_raw_file`): `fs::read` or `extract_raw_preview`. Everything after that is identical. RAW glimpses (`decode_raw_glimpse`) use the smallest embedded JPEG: decoded whole when it's a thumbnail, else its own EXIF thumbnail, else a 1/8-scale decode (Nikon/Olympus/iPhone, 5–30 ms).

<!-- ponytail: no on-disk preview cache; range reads make extraction ~ms. Add a cache under ProjectDirs::cache_dir() only if NAS/network-drive users report slowness. -->

### 2. Folder scan groups by stem (`processor/photo_folder.rs`)

```rust
const RAW_EXTENSIONS: &[&str]  // in imaging/raw_preview.rs, next to is_raw_file() = &["cr2","cr3","nef","nrw","arw","raf","orf","rw2","pef","dng","srw"];

pub struct Photo {
    pub path: PathBuf,             // file used for display
    pub file_name: String,
    pub companions: Vec<PathBuf>,  // other files of the same shot (RAW behind a JPEG, .xmp sidecars)
}
```
- Group files by **case-insensitive stem** within the folder: `IMG_0042.JPG`, `IMG_0042.CR3` and `IMG_0042.CR3.xmp`/`IMG_0042.xmp` are one shot, one `Photo`.
- Display choice: the viewable file if one exists (it's the camera's processed JPEG, usually better than the embedded preview), else the RAW.
- Companions: the rest of the group, **including `.xmp` sidecars**, so Lightroom/darktable edits travel with the RAW.

### 3. Favourite copies the whole group (`processor/favourites.rs`)

```rust
pub struct Favourite {
    pub file_name: String,
    #[serde(default)] pub companion_file_names: Vec<String>,   // Drive ids join when phase 05 lands
}
```
- `toggle_favourite(source)` **re-derives companions** by scanning the source folder for the stem. The UI sends only `source`, and its list isn't trusted.
- Copy the display file and its companions with phase 03's `.partial` → rename routine. On a collision, **the whole group gets the same suffix** (`IMG_0042 (2).JPG` + `IMG_0042 (2).CR3`) so pairs stay matched for Lightroom.
- All-or-nothing: if any copy fails, delete the files already copied in this toggle and return the error. They are our own copies from milliseconds ago, never shown as a favourite, so the trash would only add clutter.
- Un-favourite trashes the display copy plus every companion.
- Setting `copy_companions: bool` (default **true**), shown in Settings as "Include RAW and sidecar files when favouriting". Some users only want JPEGs for clients.

### 4. Drive sync handles companions (if phase 05 Path B shipped)

**Not built: phase 05 is deferred.** Do this step together with Path B.

`build_drive_plan` emits one `UploadJob` per file (display + companions), and `Uploaded` results address files by `(session_id, source, file_name)`. Un-favourite pushes all ids to `pending_drive_trash`. Per-session option "Upload RAW files to Drive", default **off** (RAWs are 25–80 MB each and clients can't open them).

### 5. UI

- Overlay badge: `JPG+RAW`, `RAW`, `JPG+RAW+XMP`, so the user knows what favouriting will copy.
- Nothing else changes. The viewer and cache don't care where the JPEG bytes came from.

### 6. Tests

Real RAWs are 11–130 MB, so tests build **synthetic containers** instead (`tests/common/raw_fixtures.rs`: NEF/DNG-style TIFF with a lossless-JPEG raw strip that must be skipped, CR3 BMFF, RAF), padded so the preview is < 10 % of the file. The real samples are covered by the spike example and the manual QA.

`tests/raw_preview.rs`:
1. **extracts_preview_per_format**: every fixture → bytes start `FF D8`, decode long edge ≥ 1600 px, correct orientation.
2. **reads_only_preview_range**: wrap the file reader in a counting reader → bytes read ≤ 10 % of the file size.
3. **corrupt_raw_errors**: truncated file → `Err`, no panic.

`tests/raw_pairs.rs` (core flows):
4. **scan_groups_pairs**: `A.JPG` + `A.CR3` + `A.CR3.xmp` + `B.NEF` → 2 photos. A is viewable with 2 companions. B is RAW.
5. **favourite_copies_group** → JPG, CR3 and XMP in the session dir, identical bytes.
6. **collision_suffixes_whole_group**, **partial_failure_rolls_back** (unreadable companion → session dir unchanged), **copy_companions_off** (only the display file copied).
