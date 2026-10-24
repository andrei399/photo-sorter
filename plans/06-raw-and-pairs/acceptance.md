# 06 — Acceptance Criteria

- [ ] **AC0 — Spike decided**: The RAW extraction approach was chosen against the written criteria, and the spike table is filled in.
- [ ] **AC1 — RAW display**: CR2, CR3, NEF, ARW, RAF, ORF, RW2 and DNG display upright on all three OSes at ≥ 1600 px on the long edge.
- [ ] **AC2 — As fast as JPEG**: Sequential RAW browsing is as instant as JPEG browsing (next frame when prefetched). Extraction reads ≤ 10 % of each file.
- [ ] **AC3 — One shot, one entry**: `X.JPG` + `X.CR3` (+ `.xmp`) appear once, with a badge listing the file types.
- [ ] **AC4 — Pairs favourite together**: Favouriting copies all of a shot's files with matching names. Un-favouriting trashes all of them.
- [ ] **AC5 — Never half a pair**: If any file of a group fails to copy, none remain in the session folder.
- [ ] **AC6 — User choice**: I can turn off copying RAW/sidecar files, and choose per session whether RAWs upload to Drive. Defaults: copy on, upload off.
- [ ] **AC7 — Robust**: Corrupt RAWs show a message and never crash the app, including under fuzzing.
