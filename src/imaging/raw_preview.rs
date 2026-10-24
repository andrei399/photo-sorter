//! Every camera RAW embeds a JPEG preview rendered by the camera. Culling only needs that JPEG, so instead of
//! demosaicing (seconds) we parse the container with seeks and read just the preview's byte range (~2–6 MB of
//! a 25–80 MB file). Every offset and length comes from untrusted input and is bounds-checked.

use crate::error::{AppError, AppResult};
use crate::imaging::decode::{Orientation, decode_full, decode_glimpse};
use egui::ColorImage;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub const RAW_EXTENSIONS: &[&str] = &[
    "cr2", "cr3", "nef", "nrw", "arw", "raf", "orf", "rw2", "pef", "dng", "srw",
];

const MAX_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;
const MAX_IFD_ENTRIES: u16 = 1024;
/// Real RAWs have under 20 IFDs; the cap stops self-referencing SubIFDs in a damaged file from fanning out.
const MAX_IFDS_PER_FILE: u16 = 64;
const MAX_TIFF_DEPTH: u8 = 4;
const MAX_BOXES_PER_LEVEL: usize = 256;
const MAX_JPEG_MARKERS_BEFORE_FRAME: usize = 64;
/// Bigger previews are first searched for their own EXIF thumbnail before being decoded for a glimpse.
const GLIMPSE_MAX_PREVIEW_BYTES: u64 = 512 * 1024;
const GLIMPSE_HEAD_BYTES: u64 = 64 * 1024;
const GLIMPSE_TARGET_PX: [u32; 2] = [320, 320];

const CANON_METADATA_UUID: [u8; 16] = *b"\x85\xc0\xb6\x87\x82\x0f\x11\xe0\x81\x11\xf4\xce\x46\x2b\x6a\x48";
const CANON_PREVIEW_UUID: [u8; 16] = *b"\xea\xf4\x2b\x5e\x1c\x98\x4b\x88\xb9\xfb\xb7\xdc\x40\x6e\x4d\x16";

pub fn is_raw_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| RAW_EXTENSIONS.iter().any(|raw| raw.eq_ignore_ascii_case(ext)))
}

pub struct RawPreview {
    pub jpeg: Vec<u8>,
    /// From the RAW's own metadata, which the embedded JPEG usually lacks. `None`: trust the JPEG's EXIF.
    pub orientation: Option<Orientation>,
}

/// The largest embedded JPEG. Reads only container headers and that byte range.
pub fn extract_raw_preview(path: &Path) -> AppResult<RawPreview> {
    extract_raw_preview_from(BufReader::new(File::open(path)?))
}

pub fn extract_raw_preview_from(reader: impl Read + Seek) -> AppResult<RawPreview> {
    let mut raw = RawFile::new(reader)?;
    let embedded = raw.find_embedded_jpegs()?;
    let largest = embedded.largest_displayable(&mut raw)?;
    Ok(RawPreview {
        jpeg: raw.read_at(largest.offset, largest.length)?,
        orientation: embedded.orientation,
    })
}

/// Low-res stand-in while the full preview decodes, from the smallest embedded JPEG: decoded whole when it's a
/// thumbnail, else via its own EXIF thumbnail (Fuji, Panasonic), else at 1/8 DCT scale (Nikon, Olympus, iPhone).
pub fn decode_raw_glimpse(path: &Path) -> Option<ColorImage> {
    let mut raw = RawFile::new(BufReader::new(File::open(path).ok()?)).ok()?;
    let embedded = raw.find_embedded_jpegs().ok()?;
    let smallest = embedded.smallest_displayable(&mut raw).ok()?;
    if smallest.length > GLIMPSE_MAX_PREVIEW_BYTES
        && let Some(glimpse) = decode_glimpse(&raw.read_at(smallest.offset, GLIMPSE_HEAD_BYTES).ok()?)
    {
        return Some(glimpse);
    }
    let jpeg = raw.read_at(smallest.offset, smallest.length).ok()?;
    decode_full(&jpeg, GLIMPSE_TARGET_PX, usize::MAX, embedded.orientation).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ByteRange {
    offset: u64,
    length: u64,
}

#[derive(Default)]
struct EmbeddedJpegs {
    candidates: Vec<ByteRange>,
    orientation: Option<Orientation>,
}

impl EmbeddedJpegs {
    /// Containers also hold lossless-JPEG raw data (CR2, DNG) that looks like a JPEG; only frames
    /// libjpeg-turbo can show at 8 bits qualify.
    fn largest_displayable<R: Read + Seek>(&self, raw: &mut RawFile<R>) -> AppResult<ByteRange> {
        self.first_displayable(raw, |a, b| b.length.cmp(&a.length))
    }

    fn smallest_displayable<R: Read + Seek>(&self, raw: &mut RawFile<R>) -> AppResult<ByteRange> {
        self.first_displayable(raw, |a, b| a.length.cmp(&b.length))
    }

    fn first_displayable<R: Read + Seek>(
        &self,
        raw: &mut RawFile<R>,
        order: impl Fn(&ByteRange, &ByteRange) -> std::cmp::Ordering,
    ) -> AppResult<ByteRange> {
        let mut candidates = self.candidates.clone();
        candidates.sort_by(order);
        candidates
            .into_iter()
            .find(|candidate| raw.is_displayable_jpeg(*candidate))
            .ok_or_else(|| AppError::Invalid("this RAW has no embedded preview".into()))
    }
}

struct RawFile<R> {
    reader: R,
    len: u64,
    ifds_left: u16,
}

impl<R: Read + Seek> RawFile<R> {
    fn new(mut reader: R) -> AppResult<Self> {
        let len = reader.seek(SeekFrom::End(0))?;
        Ok(Self {
            reader,
            len,
            ifds_left: MAX_IFDS_PER_FILE,
        })
    }

    fn read_at(&mut self, offset: u64, length: u64) -> AppResult<Vec<u8>> {
        let in_bounds = offset.checked_add(length).is_some_and(|end| end <= self.len);
        if !in_bounds || length > MAX_PREVIEW_BYTES {
            return Err(corrupt());
        }
        self.reader.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0; length as usize];
        self.reader.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn find_embedded_jpegs(&mut self) -> AppResult<EmbeddedJpegs> {
        let magic = self.read_at(0, 16.min(self.len))?;
        let mut found = EmbeddedJpegs::default();
        if magic.starts_with(b"FUJIFILMCCD-RAW ") {
            self.collect_raf(&mut found)?;
        } else if magic.get(4..12) == Some(b"ftypcrx ") {
            self.collect_cr3(&mut found)?;
        } else {
            let tiff = Tiff::at(self, 0)?;
            let first_ifd = tiff.first_ifd_offset(self)?;
            found.orientation = tiff.orientation(self, first_ifd);
            tiff.collect(self, first_ifd, 0, &mut found);
        }
        Ok(found)
    }

    /// Fixed big-endian header fields: JPEG offset at 84, length at 88. The JPEG carries its own EXIF.
    fn collect_raf(&mut self, found: &mut EmbeddedJpegs) -> AppResult<()> {
        let header = self.read_at(84, 8)?;
        found.candidates.push(ByteRange {
            offset: u32::from_be_bytes(header[0..4].try_into().expect("4 bytes")).into(),
            length: u32::from_be_bytes(header[4..8].try_into().expect("4 bytes")).into(),
        });
        Ok(())
    }

    /// ISO-BMFF. Track 1 is a full-size JPEG; the PRVW box (1620 px) and THMB (160 px) are smaller fallbacks.
    /// Orientation lives in CMT1, a TIFF IFD0 inside the Canon metadata uuid box.
    fn collect_cr3(&mut self, found: &mut EmbeddedJpegs) -> AppResult<()> {
        let top_level = self.boxes(0, self.len)?;
        if let Some(moov) = top_level.iter().find(|b| b.kind == *b"moov") {
            let moov_children = self.boxes(moov.payload, moov.end)?;
            if let Some(metadata) = moov_children.iter().find(|b| b.uuid == Some(CANON_METADATA_UUID)) {
                for child in self.boxes(metadata.payload, metadata.end)? {
                    match &child.kind {
                        b"CMT1" => {
                            if let Ok(tiff) = Tiff::at(self, child.payload)
                                && let Ok(ifd0) = tiff.first_ifd_offset(self)
                            {
                                found.orientation = tiff.orientation(self, ifd0);
                            }
                        }
                        b"THMB" => found
                            .candidates
                            .extend(self.sized_jpeg_at(child.payload + 8, child.payload + 16)),
                        _ => {}
                    }
                }
            }
            if let Some(first_track) = moov_children.iter().find(|b| b.kind == *b"trak") {
                found.candidates.extend(self.first_sample_of_track(first_track)?);
            }
        }
        if let Some(preview) = top_level.iter().find(|b| b.uuid == Some(CANON_PREVIEW_UUID)) {
            for child in self.boxes(preview.payload + 8, preview.end)? {
                if child.kind == *b"PRVW" {
                    found
                        .candidates
                        .extend(self.sized_jpeg_at(child.payload + 12, child.payload + 16));
                }
            }
        }
        Ok(())
    }

    /// A big-endian u32 length at `length_at`, JPEG bytes from `jpeg_at` (Canon's THMB/PRVW layout).
    fn sized_jpeg_at(&mut self, length_at: u64, jpeg_at: u64) -> Option<ByteRange> {
        let length = self.read_at(length_at, 4).ok()?;
        Some(ByteRange {
            offset: jpeg_at,
            length: u32::from_be_bytes(length.try_into().ok()?).into(),
        })
    }

    fn first_sample_of_track(&mut self, track: &Mp4Box) -> AppResult<Option<ByteRange>> {
        let Some(sample_table) = self.descend(track, &[*b"mdia", *b"minf", *b"stbl"])? else {
            return Ok(None);
        };
        let mut sample_size = None;
        let mut chunk_offset = None;
        for child in self.boxes(sample_table.payload, sample_table.end)? {
            match &child.kind {
                b"stsz" => {
                    let fields = self.read_at(child.payload + 4, 12)?;
                    let fixed = be_u32(&fields[0..4]);
                    sample_size = Some(if fixed != 0 { fixed } else { be_u32(&fields[8..12]) });
                }
                b"co64" => {
                    chunk_offset = Some(u64::from_be_bytes(
                        self.read_at(child.payload + 8, 8)?.try_into().expect("8 bytes"),
                    ))
                }
                b"stco" => chunk_offset = Some(be_u32(&self.read_at(child.payload + 8, 4)?).into()),
                _ => {}
            }
        }
        Ok(sample_size.zip(chunk_offset).map(|(length, offset)| ByteRange {
            offset,
            length: length.into(),
        }))
    }

    fn descend(&mut self, from: &Mp4Box, path: &[[u8; 4]]) -> AppResult<Option<Mp4Box>> {
        let mut current = from.clone();
        for kind in path {
            match self
                .boxes(current.payload, current.end)?
                .into_iter()
                .find(|b| b.kind == *kind)
            {
                Some(child) => current = child,
                None => return Ok(None),
            }
        }
        Ok(Some(current))
    }

    fn boxes(&mut self, start: u64, end: u64) -> AppResult<Vec<Mp4Box>> {
        let end = end.min(self.len);
        let mut boxes = Vec::new();
        let mut position = start;
        while position + 8 <= end && boxes.len() < MAX_BOXES_PER_LEVEL {
            let header = self.read_at(position, 8)?;
            let kind: [u8; 4] = header[4..8].try_into().expect("4 bytes");
            let (size, mut payload) = match be_u32(&header[0..4]) {
                0 => (end - position, position + 8),
                1 => (
                    u64::from_be_bytes(self.read_at(position + 8, 8)?.try_into().expect("8 bytes")),
                    position + 16,
                ),
                size => (size.into(), position + 8),
            };
            let box_end = position
                .checked_add(size)
                .filter(|&e| e <= end && size >= payload - position)
                .ok_or_else(corrupt)?;
            let uuid = if kind == *b"uuid" {
                let uuid = self.read_at(payload, 16)?.try_into().expect("16 bytes");
                payload += 16;
                Some(uuid)
            } else {
                None
            };
            boxes.push(Mp4Box {
                kind,
                uuid,
                payload,
                end: box_end,
            });
            position = box_end;
        }
        Ok(boxes)
    }

    /// Walks the JPEG markers up to the frame header with tiny seeks: baseline/extended/progressive only.
    fn is_displayable_jpeg(&mut self, candidate: ByteRange) -> bool {
        let Some(end) = candidate
            .offset
            .checked_add(candidate.length)
            .filter(|&end| end <= self.len)
        else {
            return false;
        };
        if candidate.length < 4 || self.read_at(candidate.offset, 2).ok().as_deref() != Some(&[0xFF, 0xD8]) {
            return false;
        }
        let mut position = candidate.offset + 2;
        for _ in 0..MAX_JPEG_MARKERS_BEFORE_FRAME {
            let Some(marker) = self.read_at(position, 4).ok().filter(|_| position + 4 <= end) else {
                return false;
            };
            if marker[0] != 0xFF {
                return false;
            }
            match marker[1] {
                0xC0..=0xC2 => return true,
                0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF | 0xD9 | 0xDA => return false,
                _ => position += 2 + u64::from(u16::from_be_bytes([marker[2], marker[3]])),
            }
        }
        false
    }
}

#[derive(Debug, Clone)]
struct Mp4Box {
    kind: [u8; 4],
    uuid: Option<[u8; 16]>,
    payload: u64,
    end: u64,
}

/// A TIFF structure whose offsets are relative to `base` (0 for the file itself, the box/MakerNote start otherwise).
#[derive(Clone, Copy)]
struct Tiff {
    base: u64,
    big_endian: bool,
}

struct IfdEntry {
    tag: u16,
    count: u32,
    /// The 4-byte value/offset field, already decoded as SHORT or LONG per the entry type.
    value: u32,
    is_short: bool,
}

mod tag {
    pub const COMPRESSION: u16 = 0x0103;
    pub const PHOTOMETRIC: u16 = 0x0106;
    pub const STRIP_OFFSETS: u16 = 0x0111;
    pub const ORIENTATION: u16 = 0x0112;
    pub const STRIP_BYTE_COUNTS: u16 = 0x0117;
    pub const SUB_IFDS: u16 = 0x014A;
    pub const JPEG_OFFSET: u16 = 0x0201;
    pub const JPEG_LENGTH: u16 = 0x0202;
    /// Panasonic RW2: a full-size JPEG stored as one UNDEFINED value.
    pub const PANASONIC_JPEG_FROM_RAW: u16 = 0x002E;
    pub const EXIF_IFD: u16 = 0x8769;
    pub const MAKER_NOTE: u16 = 0x927C;
    pub const OLYMPUS_CAMERA_SETTINGS: u16 = 0x2020;
    pub const OLYMPUS_PREVIEW_START: u16 = 0x0101;
    pub const OLYMPUS_PREVIEW_LENGTH: u16 = 0x0102;
}

/// Compression 6 = old-style JPEG (CR2, ARW), 7 = JPEG (DNG previews).
const JPEG_COMPRESSIONS: [u32; 2] = [6, 7];
/// DNG raw data is JPEG-compressed too, but with these photometrics it's sensor data, not a picture.
const RAW_PHOTOMETRICS: [u32; 2] = [32803, 34892];

impl Tiff {
    /// `II*\0`/`MM\0*`, plus the variants Olympus (`IIRO`, `IIRS`) and Panasonic (`IIU\0`) write.
    fn at<R: Read + Seek>(raw: &mut RawFile<R>, base: u64) -> AppResult<Self> {
        let header = raw.read_at(base, 4)?;
        let big_endian = match &header[0..2] {
            b"II" => false,
            b"MM" => true,
            _ => return Err(AppError::Invalid("not a supported RAW file".into())),
        };
        Ok(Self { base, big_endian })
    }

    fn u16(&self, bytes: &[u8]) -> u16 {
        let bytes = [bytes[0], bytes[1]];
        if self.big_endian {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        }
    }

    fn u32(&self, bytes: &[u8]) -> u32 {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if self.big_endian {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        }
    }

    fn first_ifd_offset<R: Read + Seek>(&self, raw: &mut RawFile<R>) -> AppResult<u64> {
        Ok(self.u32(&raw.read_at(self.base + 4, 4)?).into())
    }

    /// Entries plus the next-IFD offset (0 = none).
    fn ifd<R: Read + Seek>(&self, raw: &mut RawFile<R>, offset: u64) -> AppResult<(Vec<IfdEntry>, u64)> {
        raw.ifds_left = raw.ifds_left.checked_sub(1).ok_or_else(corrupt)?;
        let position = self.base + offset;
        let count = self.u16(&raw.read_at(position, 2)?).min(MAX_IFD_ENTRIES);
        let table = raw.read_at(position + 2, u64::from(count) * 12 + 4)?;
        let entries = table
            .chunks_exact(12)
            .map(|entry| {
                let kind = self.u16(&entry[2..4]);
                let is_short = kind == 3;
                IfdEntry {
                    tag: self.u16(&entry[0..2]),
                    count: self.u32(&entry[4..8]),
                    value: if is_short {
                        self.u16(&entry[8..10]).into()
                    } else {
                        self.u32(&entry[8..12])
                    },
                    is_short,
                }
            })
            .collect();
        let next = self.u32(&table[table.len() - 4..]);
        Ok((entries, next.into()))
    }

    /// LONG/IFD arrays with more than one value live out of line.
    fn values<R: Read + Seek>(&self, raw: &mut RawFile<R>, entry: &IfdEntry) -> Vec<u32> {
        if entry.count <= 1 || entry.is_short {
            return vec![entry.value];
        }
        let count = entry.count.min(u32::from(MAX_IFD_ENTRIES));
        raw.read_at(self.base + u64::from(entry.value), u64::from(count) * 4)
            .map(|bytes| bytes.chunks_exact(4).map(|value| self.u32(value)).collect())
            .unwrap_or_default()
    }

    fn orientation<R: Read + Seek>(&self, raw: &mut RawFile<R>, ifd_offset: u64) -> Option<Orientation> {
        let (entries, _) = self.ifd(raw, ifd_offset).ok()?;
        let entry = entries.iter().find(|entry| entry.tag == tag::ORIENTATION)?;
        Some(Orientation::from_exif_value(entry.value))
    }

    /// Damaged IFDs are skipped, not fatal: another IFD may still hold a good preview.
    fn collect<R: Read + Seek>(&self, raw: &mut RawFile<R>, ifd_offset: u64, depth: u8, found: &mut EmbeddedJpegs) {
        let mut next = ifd_offset;
        let mut chained = 0;
        while next != 0 && depth <= MAX_TIFF_DEPTH && chained < 8 {
            let Ok((entries, following)) = self.ifd(raw, next) else {
                return;
            };
            self.collect_from_entries(raw, &entries, depth, found);
            next = following;
            chained += 1;
        }
    }

    fn collect_from_entries<R: Read + Seek>(
        &self,
        raw: &mut RawFile<R>,
        entries: &[IfdEntry],
        depth: u8,
        found: &mut EmbeddedJpegs,
    ) {
        let value_of = |wanted: u16| {
            entries
                .iter()
                .find(|entry| entry.tag == wanted)
                .map(|entry| entry.value)
        };
        let mut add = |offset: u32, length: u32| {
            found.candidates.push(ByteRange {
                offset: self.base + u64::from(offset),
                length: length.into(),
            })
        };
        if let (Some(offset), Some(length)) = (value_of(tag::JPEG_OFFSET), value_of(tag::JPEG_LENGTH)) {
            add(offset, length);
        }
        let is_jpeg_strip = value_of(tag::COMPRESSION).is_some_and(|c| JPEG_COMPRESSIONS.contains(&c))
            && !value_of(tag::PHOTOMETRIC).is_some_and(|p| RAW_PHOTOMETRICS.contains(&p));
        let single_strip = |wanted| entries.iter().find(|entry| entry.tag == wanted && entry.count == 1);
        if is_jpeg_strip
            && let (Some(offset), Some(length)) =
                (single_strip(tag::STRIP_OFFSETS), single_strip(tag::STRIP_BYTE_COUNTS))
        {
            add(offset.value, length.value);
        }
        if let Some(entry) = entries.iter().find(|entry| entry.tag == tag::PANASONIC_JPEG_FROM_RAW) {
            add(entry.value, entry.count);
        }
        for entry in entries {
            match entry.tag {
                tag::SUB_IFDS | tag::EXIF_IFD => {
                    for sub_ifd in self.values(raw, entry) {
                        self.collect(raw, sub_ifd.into(), depth + 1, found);
                    }
                }
                tag::MAKER_NOTE => self.collect_olympus_maker_note(raw, self.base + u64::from(entry.value), found),
                _ => {}
            }
        }
    }

    /// Olympus keeps its full preview in MakerNote → CameraSettings, with offsets relative to the MakerNote.
    fn collect_olympus_maker_note<R: Read + Seek>(&self, raw: &mut RawFile<R>, start: u64, found: &mut EmbeddedJpegs) {
        if raw.read_at(start, 8).ok().as_deref() != Some(b"OLYMPUS\0") {
            return;
        }
        let Ok(maker_note) = Tiff::at(raw, start + 8).map(|tiff| Tiff { base: start, ..tiff }) else {
            return;
        };
        let Ok((entries, _)) = maker_note.ifd(raw, 12) else {
            return;
        };
        let Some(settings) = entries.iter().find(|entry| entry.tag == tag::OLYMPUS_CAMERA_SETTINGS) else {
            return;
        };
        let Ok((settings, _)) = maker_note.ifd(raw, settings.value.into()) else {
            return;
        };
        let value_of = |wanted: u16| {
            settings
                .iter()
                .find(|entry| entry.tag == wanted)
                .map(|entry| entry.value)
        };
        if let (Some(offset), Some(length)) = (
            value_of(tag::OLYMPUS_PREVIEW_START),
            value_of(tag::OLYMPUS_PREVIEW_LENGTH),
        ) {
            found.candidates.push(ByteRange {
                offset: start + u64::from(offset),
                length: length.into(),
            });
        }
    }
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn corrupt() -> AppError {
    AppError::Invalid("this RAW file is damaged or truncated".into())
}
