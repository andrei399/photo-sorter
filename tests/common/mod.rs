#![allow(dead_code)]

pub mod raw_fixtures;

use photo_sorter::core::{Command, CoreHandle, Event, spawn_core_with_trash};
use photo_sorter::models::session::SessionId;
use photo_sorter::models::settings::Settings;
use photo_sorter::models::state::PersistedState;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

pub struct TestCore {
    pub handle: CoreHandle,
    pub dir: TempDir,
}

/// Stand-in for the OS recycle bin: renames `x.jpg` to `x.jpg.trashed` next to it.
pub fn rename_to_trashed(path: &Path) -> std::io::Result<()> {
    let mut trashed = path.as_os_str().to_owned();
    trashed.push(".trashed");
    std::fs::rename(path, trashed)
}

pub fn failing_trash(_: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("no trash on this system"))
}

/// Core with its favourites root inside the temp dir, so tests never write to the real ~/Pictures.
/// Startup events (snapshot, sessions, favourites) are consumed.
pub fn test_core() -> TestCore {
    test_core_with_trash(rename_to_trashed)
}

pub fn test_core_with_trash(move_to_trash: fn(&Path) -> std::io::Result<()>) -> TestCore {
    let dir = tempfile::tempdir().expect("tempdir");
    let state = PersistedState {
        settings: Settings {
            favourites_root: dir.path().join("favourites"),
            ..Settings::default()
        },
        ..Default::default()
    };
    std::fs::write(dir.path().join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
    let handle = spawn_core_with_trash(dir.path().join("state.json"), move_to_trash, || {}).expect("spawn core");
    let core = TestCore { handle, dir };
    core.drain_startup();
    core
}

/// A second core on the same state dir, i.e. "the user restarted the app". Startup events are NOT consumed.
pub fn restart(previous: TestCore) -> TestCore {
    let TestCore { handle, dir } = previous;
    drop(handle);
    let handle = spawn_core_with_trash(dir.path().join("state.json"), rename_to_trashed, || {}).expect("respawn core");
    TestCore { handle, dir }
}

impl TestCore {
    pub fn state_path(&self) -> PathBuf {
        self.dir.path().join("state.json")
    }

    pub fn favourites_root(&self) -> PathBuf {
        self.dir.path().join("favourites")
    }

    /// Snapshot, SessionsChanged, FavouritesOfActive.
    pub fn drain_startup(&self) {
        assert!(matches!(self.expect_event(), Event::Snapshot(_)));
        assert!(matches!(self.expect_event(), Event::SessionsChanged { .. }));
        assert!(matches!(self.expect_event(), Event::FavouritesOfActive { .. }));
    }

    pub fn expect_no_event(&self) {
        if let Ok(event) = self.handle.events.recv_timeout(Duration::from_millis(200)) {
            panic!("unexpected event {event:?}");
        }
    }

    pub fn send(&self, command: Command) {
        self.handle.commands.send(command).expect("core is alive");
    }

    pub fn expect_event(&self) -> Event {
        self.handle
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("core emitted no event within 5 s")
    }

    pub fn state_on_disk(&self) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(self.state_path()).expect("state.json exists")).expect("valid json")
    }
}

/// Baseline JPEG of `width`×`height` where `color_at(x, y)` paints each pixel.
pub fn jpeg_bytes(width: u32, height: u32, color_at: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let image = image::RgbImage::from_fn(width, height, |x, y| image::Rgb(color_at(x, y)));
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95)
        .encode_image(&image)
        .expect("encode jpeg");
    bytes
}

/// Splices an EXIF APP1 segment (orientation tag + optional IFD1 thumbnail) right after the JPEG SOI marker,
/// the way cameras write it.
pub fn with_exif(jpeg: &[u8], orientation: u16, thumbnail: Option<&[u8]>) -> Vec<u8> {
    let mut tiff: Vec<u8> = b"MM\x00\x2a\x00\x00\x00\x08".to_vec();
    let ifd1_offset: u32 = if thumbnail.is_some() { 26 } else { 0 };
    // IFD0: one entry (Orientation, SHORT).
    tiff.extend_from_slice(&1u16.to_be_bytes());
    tiff.extend_from_slice(&[0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01]);
    tiff.extend_from_slice(&orientation.to_be_bytes());
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&ifd1_offset.to_be_bytes());
    if let Some(thumbnail) = thumbnail {
        // IFD1 at 26: JPEGInterchangeFormat + Length (LONG), thumbnail bytes right after at 56.
        tiff.extend_from_slice(&2u16.to_be_bytes());
        tiff.extend_from_slice(&[0x02, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01]);
        tiff.extend_from_slice(&56u32.to_be_bytes());
        tiff.extend_from_slice(&[0x02, 0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01]);
        tiff.extend_from_slice(&(thumbnail.len() as u32).to_be_bytes());
        tiff.extend_from_slice(&0u32.to_be_bytes());
        tiff.extend_from_slice(thumbnail);
    }
    let mut app1 = vec![0xFF, 0xE1];
    app1.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
    app1.extend_from_slice(b"Exif\x00\x00");
    app1.extend_from_slice(&tiff);

    let mut spliced = jpeg[..2].to_vec();
    spliced.extend_from_slice(&app1);
    spliced.extend_from_slice(&jpeg[2..]);
    spliced
}

pub type AppHarness = egui_kittest::Harness<'static, photo_sorter::app::PhotoSorterApp>;

/// The real app, headless, with its favourites root inside `state_dir` (never the real ~/Pictures).
pub fn app_harness(state_dir: &Path) -> AppHarness {
    let state = PersistedState {
        settings: Settings {
            favourites_root: state_dir.join("favourites"),
            ..Settings::default()
        },
        ..Default::default()
    };
    std::fs::write(state_dir.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
    let state_path = state_dir.join("state.json");
    egui_kittest::Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_eframe(move |cc| photo_sorter::app::PhotoSorterApp::new(&cc.egui_ctx, state_path).unwrap())
}

/// Steps the app until `done` holds; the core and decoders answer from other threads.
pub fn run_until(harness: &mut AppHarness, what: &str, done: impl Fn(&AppHarness) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        harness.step();
        if done(harness) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("never happened: {what}");
}

pub fn run_until_label(harness: &mut AppHarness, label: &str) {
    use egui_kittest::kittest::Queryable;
    run_until(harness, &format!("label {label:?} visible"), |h| {
        h.query_by_label_contains(label).is_some()
    });
}

/// A folder of `count` small JPEGs named IMG_1.jpg … IMG_<count>.jpg.
pub fn jpeg_folder(count: usize) -> TempDir {
    let folder = tempfile::tempdir().unwrap();
    for i in 1..=count {
        std::fs::write(
            folder.path().join(format!("IMG_{i}.jpg")),
            jpeg_bytes(300, 200, |x, y| [x as u8, y as u8, 60]),
        )
        .unwrap();
    }
    folder
}

pub fn photo_folder_with(names_and_bytes: &[(&str, &[u8])]) -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    for (name, bytes) in names_and_bytes {
        fs::write(folder.path().join(name), bytes).unwrap();
    }
    folder
}

/// Returns the new session's id after consuming SessionsChanged + FavouritesOfActive.
pub fn create_session(core: &TestCore, name: &str) -> SessionId {
    core.send(Command::CreateSession {
        name: name.into(),
        dir: None,
    });
    let Event::SessionsChanged { active: Some(id), .. } = core.expect_event() else {
        panic!("expected SessionsChanged")
    };
    assert!(matches!(core.expect_event(), Event::FavouritesOfActive { session_id: Some(s), .. } if s == id));
    id
}

pub fn toggle(core: &TestCore, source: &Path) -> Event {
    core.send(Command::ToggleFavourite(source.to_path_buf()));
    let event = core.expect_event();
    if !matches!(event, Event::FavouriteFailed { .. }) {
        assert!(matches!(core.expect_event(), Event::SessionsChanged { .. }));
    }
    event
}

pub fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}
