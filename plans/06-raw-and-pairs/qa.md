# 06 — QA

## Fixtures

`qa-photos/raw/` from raw.pixls.us. At least: Canon CR2 (5D III), CR3 (R5, R6, M50), Nikon NEF (D750, Z6), Sony ARW (A7 III, A7 IV), Fuji RAF (X-T3), Olympus ORF, Panasonic RW2, DNG (Leica + phone). Also a 200-pair RAW+JPEG folder from a real shoot, a truncated file per format, and the RAW set copied to an SD card.

## Automated

| Test | Asserts |
|---|---|
| `extracts_preview_per_format` | JPEG bytes, ≥ 1600 px, correct orientation. |
| `reads_only_preview_range` | ≤ 10 % of the file read. |
| `corrupt_raw_errors` | `Err`, no panic. |
| `scan_groups_pairs` | Grouping, display choice, companions including XMP. |
| `favourite_copies_group` / `collision_suffixes_whole_group` / `partial_failure_rolls_back` / `copy_companions_off` | As in the plan. |
| fuzz (one-off, not CI) | 10 min of `cargo fuzz` on `extract_raw_preview`, zero crashes. |

## Spike report (fill in during step 0)

| Camera / format | Preview found | Preview size | Bytes read / file size | Extract ms (SSD) | Extract ms (SD) |
|---|---|---|---|---|---|
| Canon R5 CR3 | | | | | |
| … | | | | | |

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
