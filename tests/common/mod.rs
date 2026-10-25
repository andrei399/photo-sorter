#![allow(dead_code)]

pub mod raw_fixtures;

use photo_sorter::core::{Command, CoreHandle, Event, spawn_core_with};
use photo_sorter::integrations::cloud::{CloudConfig, OAuthClient, ProviderEndpoints, SecretStore};
use photo_sorter::models::cloud::CloudProvider;
use photo_sorter::models::session::SessionId;
use photo_sorter::models::settings::Settings;
use photo_sorter::models::state::PersistedState;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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
    test_core_with(move_to_trash, |_| {}, offline_cloud_config())
}

/// `seed` edits the initial state (favourites root already inside the temp dir). Startup events are consumed.
pub fn test_core_with(
    move_to_trash: fn(&Path) -> std::io::Result<()>,
    seed: impl FnOnce(&mut PersistedState),
    cloud_config: CloudConfig,
) -> TestCore {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut state = PersistedState {
        settings: Settings {
            favourites_root: dir.path().join("favourites"),
            ..Settings::default()
        },
        ..Default::default()
    };
    seed(&mut state);
    std::fs::write(dir.path().join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
    let handle =
        spawn_core_with(dir.path().join("state.json"), move_to_trash, cloud_config, || {}).expect("spawn core");
    let core = TestCore { handle, dir };
    core.drain_startup();
    core
}

/// Stands in for the OS keychain.
#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<CloudProvider, String>>);

impl MemorySecrets {
    pub fn with(secrets: &[(CloudProvider, &str)]) -> Arc<Self> {
        let store = Self::default();
        for (provider, secret) in secrets {
            store.save(*provider, secret).unwrap();
        }
        Arc::new(store)
    }

    pub fn get(&self, provider: CloudProvider) -> Option<String> {
        self.0.lock().unwrap().get(&provider).cloned()
    }
}

impl SecretStore for MemorySecrets {
    fn load(&self, provider: CloudProvider) -> Result<Option<String>, String> {
        Ok(self.get(provider))
    }

    fn save(&self, provider: CloudProvider, secret: &str) -> Result<(), String> {
        self.0.lock().unwrap().insert(provider, secret.to_owned());
        Ok(())
    }

    fn delete(&self, provider: CloudProvider) -> Result<(), String> {
        self.0.lock().unwrap().remove(&provider);
        Ok(())
    }
}

/// Every provider at `<base>/<google|onedrive|dropbox>/…`, with test client ids and instant-ish retries.
pub fn cloud_config_at(base: &str, secrets: Arc<MemorySecrets>) -> CloudConfig {
    let endpoints = |prefix: &str, secret: Option<&str>| {
        let root = format!("{base}/{prefix}");
        ProviderEndpoints {
            authorize: format!("{root}/authorize"),
            token: format!("{root}/token"),
            api: root.clone(),
            content: root.clone(),
            client: Some(OAuthClient {
                id: format!("{prefix}-client"),
                secret: secret.map(str::to_owned),
            }),
        }
    };
    CloudConfig {
        google_drive: endpoints("google", Some("google-secret")),
        onedrive: endpoints("onedrive", None),
        dropbox: endpoints("dropbox", None),
        secrets,
        open_browser: approve_in_browser,
        first_retry_delay: Duration::from_millis(20),
    }
}

/// Nothing listens on port 9; no test without accounts ever gets that far.
pub fn offline_cloud_config() -> CloudConfig {
    cloud_config_at("http://127.0.0.1:9", Arc::new(MemorySecrets::default()))
}

/// Plays the user clicking "Allow": calls the app's loopback redirect with a code and the request's `state`.
pub fn approve_in_browser(authorize_url: &str) -> std::io::Result<()> {
    let authorize_url = reqwest::Url::parse(authorize_url).unwrap();
    let params: HashMap<String, String> = authorize_url.query_pairs().into_owned().collect();
    assert_eq!(params["code_challenge_method"], "S256");
    assert_eq!(params["code_challenge"].len(), 43, "base64url SHA-256");
    let port = reqwest::Url::parse(&params["redirect_uri"]).unwrap().port().unwrap();
    let state = params["state"].clone();
    std::thread::spawn(move || {
        // A browser's favicon request may come first and must be ignored.
        let mut favicon = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(favicon, "GET /favicon.ico HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "GET /?code=test-code&state={state} HTTP/1.1\r\nHost: localhost\r\n\r\n"
        )
        .unwrap();
        let mut page = String::new();
        let _ = stream.read_to_string(&mut page);
        assert!(page.contains("Connected"), "{page}");
    });
    Ok(())
}

/// A second core on the same state dir, i.e. "the user restarted the app". Startup events are NOT consumed.
pub fn restart(previous: TestCore) -> TestCore {
    let TestCore { handle, dir } = previous;
    drop(handle);
    let handle = spawn_core_with(
        dir.path().join("state.json"),
        rename_to_trashed,
        offline_cloud_config(),
        || {},
    )
    .expect("respawn core");
    TestCore { handle, dir }
}

impl TestCore {
    pub fn state_path(&self) -> PathBuf {
        self.dir.path().join("state.json")
    }

    pub fn favourites_root(&self) -> PathBuf {
        self.dir.path().join("favourites")
    }

    /// Snapshot, SessionsChanged, FavouritesOfActive (and the initial cloud status).
    pub fn drain_startup(&self) {
        assert!(matches!(self.expect_event(), Event::Snapshot(_)));
        assert!(matches!(self.expect_event(), Event::SessionsChanged { .. }));
        assert!(matches!(self.expect_event(), Event::FavouritesOfActive { .. }));
    }

    /// Skips other events until one satisfies `matches`; uploads finish on their own schedule.
    pub fn wait_for_event(&self, what: &str, matches: impl Fn(&Event) -> bool) -> Event {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.handle.events.recv_timeout(left) {
                Ok(event) if matches(&event) => return event,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        panic!("never happened: {what}");
    }

    pub fn wait_for_state(&self, what: &str, holds: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let state = self.state_on_disk();
            if holds(&state) {
                return state;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("state never satisfied: {what}\n{:#}", self.state_on_disk());
    }

    pub fn expect_no_event(&self) {
        while let Ok(event) = self.handle.events.recv_timeout(Duration::from_millis(200)) {
            if !matches!(event, Event::CloudStatusChanged(_)) {
                panic!("unexpected event {event:?}");
            }
        }
    }

    pub fn send(&self, command: Command) {
        self.handle.commands.send(command).expect("core is alive");
    }

    /// Upload status arrives on the cloud worker's schedule, so it's skipped here; `wait_for_event` sees it.
    pub fn expect_event(&self) -> Event {
        loop {
            let event = self
                .handle
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("core emitted no event within 5 s");
            if !matches!(event, Event::CloudStatusChanged(_)) {
                return event;
            }
        }
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
    splice_exif(jpeg, &tiff)
}

/// A camera JPEG's EXIF reduced to its capture time: IFD0 → Exif IFD → DateTimeOriginal (`2024:06:15 14:32:07`).
pub fn with_capture_time(jpeg: &[u8], date_time_original: &str) -> Vec<u8> {
    let mut tiff: Vec<u8> = b"MM\x00\x2a\x00\x00\x00\x08".to_vec();
    // IFD0 at 8: ExifIFDPointer (LONG) → Exif IFD at 26.
    tiff.extend_from_slice(&1u16.to_be_bytes());
    tiff.extend_from_slice(&[0x87, 0x69, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01]);
    tiff.extend_from_slice(&26u32.to_be_bytes());
    tiff.extend_from_slice(&0u32.to_be_bytes());
    // Exif IFD at 26: DateTimeOriginal (ASCII, NUL-terminated) stored at 44.
    let ascii = [date_time_original.as_bytes(), b"\0"].concat();
    tiff.extend_from_slice(&1u16.to_be_bytes());
    tiff.extend_from_slice(&[0x90, 0x03, 0x00, 0x02]);
    tiff.extend_from_slice(&(ascii.len() as u32).to_be_bytes());
    tiff.extend_from_slice(&44u32.to_be_bytes());
    tiff.extend_from_slice(&0u32.to_be_bytes());
    tiff.extend_from_slice(&ascii);
    splice_exif(jpeg, &tiff)
}

fn splice_exif(jpeg: &[u8], tiff: &[u8]) -> Vec<u8> {
    let mut app1 = vec![0xFF, 0xE1];
    app1.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
    app1.extend_from_slice(b"Exif\x00\x00");
    app1.extend_from_slice(tiff);

    let mut spliced = jpeg[..2].to_vec();
    spliced.extend_from_slice(&app1);
    spliced.extend_from_slice(&jpeg[2..]);
    spliced
}

pub type AppHarness = egui_kittest::Harness<'static, photo_sorter::app::PhotoSorterApp>;

/// The real app, headless, with its favourites root inside `state_dir` (never the real ~/Pictures).
pub fn app_harness(state_dir: &Path) -> AppHarness {
    app_harness_with(state_dir, |_| {}, offline_cloud_config())
}

/// `seed` edits the initial state; `cloud_config` points the cloud providers at a fake server.
pub fn app_harness_with(
    state_dir: &Path,
    seed: impl FnOnce(&mut PersistedState),
    cloud_config: CloudConfig,
) -> AppHarness {
    let mut state = PersistedState {
        settings: Settings {
            favourites_root: state_dir.join("favourites"),
            ..Settings::default()
        },
        ..Default::default()
    };
    seed(&mut state);
    std::fs::write(state_dir.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
    let state_path = state_dir.join("state.json");
    egui_kittest::Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_eframe(move |cc| {
            photo_sorter::app::PhotoSorterApp::new_with_cloud(&cc.egui_ctx, state_path, cloud_config).unwrap()
        })
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
