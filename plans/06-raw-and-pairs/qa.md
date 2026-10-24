# 06 — QA

## Fixtures

`qa-photos/raw/` from raw.pixls.us. At least: Canon CR2 (5D III), CR3 (R5, R6, M50), Nikon NEF (D750, Z6), Sony ARW (A7 III, A7 IV), Fuji RAF (X-T3), Olympus ORF, Panasonic RW2, DNG (Leica + phone). Also a 200-pair RAW+JPEG folder from a real shoot, a truncated file per format, and the RAW set copied to an SD card.

## Automated

| Test | Asserts |
|---|---|
| `extracts_upright_full_size_preview_per_format` | JPEG bytes, full size, correct orientation. |
| `reads_only_the_preview_range` | ≤ 10 % of the file read, ≤ preview + 4 KB. |
| `corrupt_raws_error_and_never_panic` | `Err`, no panic. |
| `shot_files_are_grouped_into_one_photo_preferring_the_jpeg` | Grouping, display choice, companions including XMP. |
| `favouriting_copies_the_whole_shot_and_unfavouriting_trashes_it` / `name_collision_suffixes_every_file_of_the_shot_alike` / `a_failed_companion_copy_leaves_nothing_behind` / `companions_can_be_turned_off` | As in the plan. |
| `viewer_shows_the_pair_once_with_a_badge_and_enter_copies_both` | The real app: one entry, `JPG+RAW` badge, Enter copies both, a RAW-only shot decodes. |
| `glimpse_is_small_and_upright` / `raw_photos_decode_through_the_prefetch_pipeline` | RAW glimpse and full tier through the real prefetch pool. |
| fuzz (one-off, not CI) | Done 2026-10-24: 378 k truncated and bit-flipped runs over the 14 real samples, zero panics. CI runs a smaller version on the synthetic fixtures (`corrupt_raws_error_and_never_panic`). |

## Spike report (fill in during step 0)

Measured 2026-10-24 in WSL2 on an NVMe SSD, files in page cache (so ms are parse cost, not I/O). Bytes read include all header parsing; the overhead beyond the preview is < 10 KB everywhere. SD card column: still to measure on native hardware.

| Camera / format | Preview | Orientation | Bytes read / file | Extract ms | Glimpse |
|---|---|---|---|---|---|
| Canon 5D III CR2 | 5760×3840 | from IFD0 | 5.7 % (1.48 MB) | 0.2 | 160×120 IFD1, 0.1 ms |
| Canon M50 CR3 (C-RAW) | 6000×4000 | from CMT1 | 11.7 % (1.74 MB) | 0.3 | 160×120 THMB, 0.1 ms |
| Canon R5 CR3 (C-RAW, crop) | 5088×3392 | from CMT1 | 10.4 % (1.19 MB) | 0.2 | 160×120 THMB, 0.1 ms |
| Canon R6 CR3 (C-RAW) | 5472×3648 | from CMT1 | 20.5 % (2.29 MB) | 0.5 | 160×120 THMB, 0.2 ms |
| Nikon D750 NEF | 6016×4016 | from IFD0 | 5.5 % (0.98 MB) | 0.3 | 1/8 decode, 6.7 ms |
| Nikon Z6 NEF | 6048×4024 | from IFD0 | 2.7 % (1.29 MB) | 0.2 | 1/8 decode, 5.0 ms |
| Sony A7 III ARW | 1616×1080 | from IFD0 | 4.5 % (1.15 MB) | 0.2 | 160×120, 0.1 ms |
| Sony A7 IV ARW (uncompressed) | 7008×4672 | from IFD0 | 9.1 % (6.65 MB) | 1.0 | 160×120, 0.2 ms |
| Fuji X-T3 RAF | 4416×2944 | JPEG's own EXIF | 4.8 % (2.67 MB) | 0.4 | EXIF thumb, 0.1 ms |
| Olympus E-M1 II ORF | 3200×2400 | from IFD0 | 6.2 % (1.06 MB) | 0.2 | 1/8 decode, 9.3 ms |
| Olympus E-M10 III ORF | 3200×2400 | from IFD0 | 5.4 % (0.99 MB) | 0.3 | 1/8 decode, 8.8 ms |
| Panasonic G9 RW2 | 1920×1440 | from IFD0 | 0.5 % (0.61 MB) | 0.2 | EXIF thumb, 0.2 ms |
| Leica M10 DNG | 5952×3968 | from IFD0 | 7.3 % (2.51 MB) | 0.4 | 160×120, 0.2 ms |
| iPhone 12 Pro DNG | 4032×3024 (portrait) | from IFD0 | 18.4 % (5.37 MB) | 0.8 | 1/8 decode, 28 ms |

## Manual (each OS)

| # | Step | Expected |
|---|---|---|
| 1 | Open the RAW-only folder | Every supported format displays. Sequential browsing is as instant as JPEG. |
| 2 | Portrait RAWs | Upright. |
| 3 | Hold → through 100 RAWs | Smooth, never blank. |
| 4 | Same from the SD card | Still smooth after the first 2 (range reads + prefetch). |
| 5 | RAW+JPEG folder | Each shot appears once (count = number of pairs), with a `JPG+RAW` badge. |
| 6 | Favourite a pair | Session dir gets JPG + RAW (+ XMP) with matching names. |
| 7 | Un-favourite | All of the group's files go to trash. |
| 8 | Collision across two folders with pairs | `IMG_0001 (2).JPG` + `IMG_0001 (2).CR3`. |
| 9 | Settings: companions off, favourite a pair | Only the JPG is copied. |
| 10 | Drive (Path B), "Upload RAW" off, then on | Only JPGs upload, then the RAWs follow. |
| 11 | Import the session folder into Lightroom/darktable | Pairs and XMP recognized. |
| 12 | Truncated RAW | "Can't display" message. Navigation OK. |
| 13 | Phone DNG without a preview | Glimpse, then the full decode appears (slower, acceptable). |
