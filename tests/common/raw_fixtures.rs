//! Minimal but structurally faithful RAW containers, so the extractor is tested without committing 20–80 MB
//! camera files. Each is padded with fake sensor data so the preview is a small fraction of the file, like the real thing.

/// A frame header libjpeg-turbo can't show at 8 bits (lossless, SOF3), like CR2/DNG raw data.
fn lossless_jpeg_sensor_data(len: usize) -> Vec<u8> {
    let mut data = vec![
        0xFF, 0xD8, 0xFF, 0xC3, 0x00, 0x0B, 0x0E, 0x10, 0x00, 0x10, 0x00, 0x01, 0x01, 0x11, 0x00,
    ];
    data.resize(len, 0x5A);
    data
}

fn le_entry(tag: u16, kind: u16, count: u32, value: u32) -> Vec<u8> {
    let mut entry = Vec::with_capacity(12);
    entry.extend_from_slice(&tag.to_le_bytes());
    entry.extend_from_slice(&kind.to_le_bytes());
    entry.extend_from_slice(&count.to_le_bytes());
    if kind == 3 && count == 1 {
        entry.extend_from_slice(&(value as u16).to_le_bytes());
        entry.extend_from_slice(&[0, 0]);
    } else {
        entry.extend_from_slice(&value.to_le_bytes());
    }
    entry
}

fn le_ifd(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut ifd = (entries.len() as u16).to_le_bytes().to_vec();
    entries.iter().for_each(|entry| ifd.extend_from_slice(entry));
    ifd.extend_from_slice(&0u32.to_le_bytes());
    ifd
}

fn ifd_len(entry_count: usize) -> u32 {
    (2 + 12 * entry_count + 4) as u32
}

/// NEF/DNG layout: IFD0 holds the orientation; SubIFDs hold the preview (JPEGInterchangeFormat) and the raw
/// data (a single lossless-JPEG strip, bigger than the preview, that must be skipped).
pub fn tiff_raw(preview_jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let ifd0_at = 8;
    let sub_ifd_list_at = ifd0_at + ifd_len(3);
    let preview_ifd_at = sub_ifd_list_at + 8;
    let raw_ifd_at = preview_ifd_at + ifd_len(2);
    let preview_at = raw_ifd_at + ifd_len(4);
    let sensor_data = lossless_jpeg_sensor_data(preview_jpeg.len() * 12);
    let sensor_at = preview_at + preview_jpeg.len() as u32;

    let mut file = b"II*\0".to_vec();
    file.extend_from_slice(&ifd0_at.to_le_bytes());
    file.extend(le_ifd(&[
        le_entry(0x00FE, 4, 1, 1),
        le_entry(0x0112, 3, 1, orientation.into()),
        le_entry(0x014A, 4, 2, sub_ifd_list_at),
    ]));
    file.extend_from_slice(&preview_ifd_at.to_le_bytes());
    file.extend_from_slice(&raw_ifd_at.to_le_bytes());
    file.extend(le_ifd(&[
        le_entry(0x0201, 4, 1, preview_at),
        le_entry(0x0202, 4, 1, preview_jpeg.len() as u32),
    ]));
    file.extend(le_ifd(&[
        le_entry(0x0103, 3, 1, 7),
        le_entry(0x0106, 3, 1, 32803),
        le_entry(0x0111, 4, 1, sensor_at),
        le_entry(0x0117, 4, 1, sensor_data.len() as u32),
    ]));
    assert_eq!(file.len() as u32, preview_at);
    file.extend_from_slice(preview_jpeg);
    file.extend(sensor_data);
    file
}

fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut b = ((8 + payload.len()) as u32).to_be_bytes().to_vec();
    b.extend_from_slice(kind);
    b.extend_from_slice(payload);
    b
}

fn uuid_box(uuid: &[u8; 16], children: &[u8]) -> Vec<u8> {
    let mut payload = uuid.to_vec();
    payload.extend_from_slice(children);
    mp4_box(b"uuid", &payload)
}

/// Canon CR3 (ISO-BMFF): CMT1 (TIFF IFD0 with the orientation) and THMB inside the Canon metadata uuid box,
/// track 1's single sample is the full-size JPEG in mdat.
pub fn cr3_raw(preview_jpeg: &[u8], thumbnail_jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let ftyp = mp4_box(b"ftyp", b"crx \x00\x00\x00\x01crx isom");
    let mut cmt1 = b"II*\0".to_vec();
    cmt1.extend_from_slice(&8u32.to_le_bytes());
    cmt1.extend(le_ifd(&[le_entry(0x0112, 3, 1, orientation.into())]));
    let mut thmb = vec![0, 0, 0, 0, 0, 160, 0, 120];
    thmb.extend_from_slice(&(thumbnail_jpeg.len() as u32).to_be_bytes());
    thmb.extend_from_slice(&[0, 1, 0, 0]);
    thmb.extend_from_slice(thumbnail_jpeg);
    let mut canon_metadata = mp4_box(b"CMT1", &cmt1);
    canon_metadata.extend(mp4_box(b"THMB", &thmb));

    let moov_for = |preview_at: u64| {
        let mut stsz = vec![0; 4];
        stsz.extend_from_slice(&(preview_jpeg.len() as u32).to_be_bytes());
        stsz.extend_from_slice(&1u32.to_be_bytes());
        let mut co64 = vec![0, 0, 0, 0, 0, 0, 0, 1];
        co64.extend_from_slice(&preview_at.to_be_bytes());
        let mut stbl_children = mp4_box(b"stsz", &stsz);
        stbl_children.extend(mp4_box(b"co64", &co64));
        let trak = mp4_box(
            b"trak",
            &mp4_box(b"mdia", &mp4_box(b"minf", &mp4_box(b"stbl", &stbl_children))),
        );
        let mut moov_children = uuid_box(
            b"\x85\xc0\xb6\x87\x82\x0f\x11\xe0\x81\x11\xf4\xce\x46\x2b\x6a\x48",
            &canon_metadata,
        );
        moov_children.extend(trak);
        mp4_box(b"moov", &moov_children)
    };
    let preview_at = (ftyp.len() + moov_for(0).len() + 8) as u64;
    let mut mdat = preview_jpeg.to_vec();
    mdat.extend(lossless_jpeg_sensor_data(preview_jpeg.len() * 12));

    let mut file = ftyp;
    file.extend(moov_for(preview_at));
    file.extend(mp4_box(b"mdat", &mdat));
    file
}

/// Fuji RAF: big-endian JPEG offset/length at fixed header positions 84/88. The JPEG carries its own EXIF.
pub fn raf_raw(preview_jpeg_with_exif: &[u8]) -> Vec<u8> {
    let preview_at = 160u32;
    let mut file = b"FUJIFILMCCD-RAW 0201FF383501".to_vec();
    file.resize(84, 0);
    file.extend_from_slice(&preview_at.to_be_bytes());
    file.extend_from_slice(&(preview_jpeg_with_exif.len() as u32).to_be_bytes());
    file.resize(preview_at as usize, 0);
    file.extend_from_slice(preview_jpeg_with_exif);
    file.extend(vec![0x5A; preview_jpeg_with_exif.len() * 12]);
    file
}
