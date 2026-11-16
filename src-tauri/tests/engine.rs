//! Integration tests: the core against a real Photos.sh engine.
//!
//! Skipped unless both are set:
//!
//!     PHOTOS_TEST_BASE_URL=http://127.0.0.1:8098/api
//!     PHOTOS_TEST_API_KEY=...
//!
//! They keep model calls few and pictures small (the engine may be shared).
//! `PHOTOS_TEST_KEYCHAIN=1` also tries the operating system's keychain, with
//! an entry of its own that is removed again.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use photos_desktop::batch::{BatchEvents, BatchItem, BatchSummary, ItemStatus};
use photos_desktop::core::{Core, EditorTool};
use photos_desktop::keychain::{KeyVault, MemoryKeychain, OsKeychain, SecretStore, Storage};
use photos_desktop::mask::Stroke;
use photos_desktop::ops::{Fit, Format, Operation, VectorMode};
use photos_desktop::settings::Settings;

fn engine() -> Option<(String, String)> {
    let base_url = std::env::var("PHOTOS_TEST_BASE_URL").ok()?;
    let key = std::env::var("PHOTOS_TEST_API_KEY").ok()?;
    if base_url.is_empty() || key.is_empty() {
        return None;
    }
    assert!(
        !base_url.contains("photos.sh"),
        "the tests are not to be run against production"
    );
    Some((base_url, key))
}

macro_rules! engine_or_skip {
    () => {
        match engine() {
            Some(engine) => engine,
            None => {
                eprintln!("skipped: set PHOTOS_TEST_BASE_URL and PHOTOS_TEST_API_KEY");
                return;
            }
        }
    };
}

fn core(dir: &Path, base_url: &str, key: &str) -> Core {
    let core = Core::new(
        dir.join("config/settings.json"),
        Box::new(MemoryKeychain::with_key(key)),
    )
    .unwrap();
    core.set_settings(Settings {
        base_url: base_url.to_string(),
        parallel: 2,
        default_format: None,
    })
    .unwrap();
    core
}

/// A product-like photo: a dark red disc on a light backdrop.
fn write_photo(path: &Path, width: u32, height: u32) -> PathBuf {
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let radius = width.min(height) as f32 * 0.3;
    let image = image::RgbImage::from_fn(width, height, |x, y| {
        let (dx, dy) = (x as f32 - cx, y as f32 - cy);
        if (dx * dx + dy * dy).sqrt() < radius {
            image::Rgb([170, 30, 40])
        } else {
            let shade = 225 + ((x + y) % 12) as u8;
            image::Rgb([shade, shade, shade])
        }
    });
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    image.save(path).unwrap();
    path.to_path_buf()
}

#[derive(Default)]
struct Recorder(Mutex<Vec<BatchItem>>, Mutex<Vec<BatchSummary>>);

impl BatchEvents for Recorder {
    fn item(&self, item: &BatchItem) {
        self.0.lock().unwrap().push(item.clone());
    }
    fn finished(&self, summary: &BatchSummary) {
        self.1.lock().unwrap().push(summary.clone());
    }
}

#[tokio::test]
async fn the_account_and_the_lists() {
    let (base_url, key) = engine_or_skip!();
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path(), &base_url, &key);

    assert_eq!(core.client().unwrap().health().await.unwrap(), "ok");
    let account = core.account().await.unwrap();
    assert!(account.unlimited || account.credits.is_some());
    let catalog = core.catalog().await.unwrap();
    assert!(catalog.presets.iter().any(|p| p.name == "web"));
    assert!(
        catalog
            .marketplaces
            .iter()
            .any(|m| m.name == "amazon" && !m.title.is_empty())
    );
}

#[tokio::test]
async fn a_refused_key_is_unauthorized_and_starts_no_batch() {
    let (base_url, _) = engine_or_skip!();
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path(), &base_url, "not-a-real-key-0000");
    let error = core.account().await.unwrap_err();
    assert_eq!(error.code, "unauthorized");
    assert_eq!(error.status, Some(401));

    core.batch_add(&[write_photo(&dir.path().join("a.png"), 64, 64)])
        .unwrap();
    let operation = Operation::Resize {
        width: Some(32),
        height: None,
        fit: None,
        enlarge: false,
        format: None,
        quality: None,
        max_bytes: None,
        background: None,
    };
    let refused = core.batch_prepare(operation, None).await.err().unwrap();
    assert_eq!(refused.code, "unauthorized");
    assert!(!dir.path().join("photos-sh").exists());
}

#[tokio::test]
async fn a_batch_of_resizes() {
    let (base_url, key) = engine_or_skip!();
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path(), &base_url, &key);

    let sources = dir.path().join("shop");
    write_photo(&sources.join("a.png"), 200, 120);
    write_photo(&sources.join("b.jpg"), 160, 160);
    write_photo(&sources.join("more/a.png"), 120, 200);
    // Not a picture, whatever its name says: fails alone.
    std::fs::write(sources.join("broken.png"), b"this is not a png").unwrap();
    std::fs::write(sources.join("notes.txt"), b"not taken").unwrap();
    let snapshot = core.batch_add(std::slice::from_ref(&sources)).unwrap();
    assert_eq!(snapshot.items.len(), 4);

    let operation = Operation::Resize {
        width: Some(64),
        height: Some(64),
        fit: Some(Fit::Contain),
        enlarge: false,
        format: Some(Format::Webp),
        quality: Some(80),
        max_bytes: None,
        background: None,
    };
    let estimate = core.batch_estimate(&operation).await.unwrap();
    assert_eq!(estimate.files, 4);
    assert!(estimate.enough);

    let prepared = core.batch_prepare(operation, None).await.unwrap();
    let out = sources.join("photos-sh");
    assert_eq!(prepared.job.output_dir, out);
    let events = Recorder::default();
    let summary = core.batch_run(prepared, &events).await.unwrap();
    assert_eq!(
        (summary.total, summary.done, summary.failed, summary.skipped),
        (4, 3, 1, 0)
    );
    assert!(summary.stopped.is_none());

    let items = core.batch_snapshot().items;
    let broken = items.iter().find(|i| i.name == "broken.png").unwrap();
    assert_eq!(broken.status, ItemStatus::Failed);
    let code = &broken.error.as_ref().unwrap().code;
    assert!(
        ["bad_request", "unsupported_format", "unprocessable"].contains(&code.as_str()),
        "unexpected code {code}"
    );

    // Names from the sources, the extension from the result's type; the two
    // a.png sources do not share a result.
    let mut names: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["a-2.webp", "a.webp", "b.webp"]);
    let (width, height) = image::image_dimensions(out.join("a.webp")).unwrap();
    assert!(width <= 64 && height <= 64 && width.max(height) == 64);
    let done = items.iter().find(|i| i.name == "b.jpg").unwrap();
    assert_eq!((done.width, done.height), (Some(64), Some(64)));
    assert!(done.bytes.unwrap() > 0);

    // The thumbnails the list shows.
    let thumb = core.preview(&format!("batch/{}/source", done.id)).unwrap();
    assert_eq!(thumb.mime, "image/jpeg");
    let thumb = core.preview(&format!("batch/{}/result", done.id)).unwrap();
    assert!(!thumb.bytes.is_empty());
    assert!(
        core.preview(&format!("batch/{}/result", broken.id))
            .is_err()
    );

    // Retrying runs only the failed file, into the same folder.
    let prepared = core.batch_prepare_retry().await.unwrap();
    assert_eq!(prepared.estimate.files, 1);
    let again = core.batch_run(prepared, &events).await.unwrap();
    assert_eq!((again.total, again.failed), (1, 1));
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 3);
    // A second run leaves the first run's results alone.
    assert!(sources.join("a.png").exists());
}

/// The tools that need no model: enhance, a text watermark, expand onto a
/// colour, and vectorize, whose result is an SVG file.
#[tokio::test]
async fn the_tools_without_a_model() {
    let (base_url, key) = engine_or_skip!();
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path(), &base_url, &key);
    let original = write_photo(&dir.path().join("disc.png"), 256, 192);
    let session = core.editor_open(&original).unwrap().session;
    let apply = async |json: &str| {
        let operation: Operation = serde_json::from_str(json).unwrap();
        core.editor_apply(EditorTool::Operation { operation }).await
    };

    let enhanced = apply(r#"{"kind":"enhance","brightness":10,"sharpen":20}"#)
        .await
        .unwrap();
    assert_eq!(enhanced.current.label, "Enhance");
    assert_eq!(
        (enhanced.current.width, enhanced.current.height),
        (Some(256), Some(192))
    );

    let marked = apply(r#"{"kind":"watermark","text":"photos.sh","position":"center","scale":60}"#)
        .await
        .unwrap();
    assert_eq!(
        (marked.cursor, marked.current.width, marked.current.height),
        (2, Some(256), Some(192))
    );

    // To a square on white: the height grows, half above and half below.
    let square = apply(r#"{"kind":"expand","aspect":"1:1","fill":"white"}"#)
        .await
        .unwrap();
    assert_eq!(square.current.label, "Expand to 1:1");
    assert_eq!(
        (square.current.width, square.current.height),
        (Some(256), Some(256))
    );
    let picture = core.preview(&format!("editor/{session}/3")).unwrap();
    let decoded = image::load_from_memory(&picture.bytes).unwrap().to_rgb8();
    assert_eq!(decoded.get_pixel(128, 2).0, [255, 255, 255]);

    // Bad options come back as the API's own error, and add no step.
    let refused = apply(r#"{"kind":"expand","aspect":"1:30"}"#)
        .await
        .unwrap_err();
    assert_eq!(refused.code, "bad_request");
    assert_eq!(core.editor_snapshot().unwrap().cursor, 3);

    // An SVG is not a step; in a batch it is a file of its own type.
    let refused = apply(r#"{"kind":"vectorize"}"#).await.unwrap_err();
    assert_eq!(refused.code, "invalid");
    core.editor_close();

    core.batch_add(std::slice::from_ref(&original)).unwrap();
    let vectorize = Operation::Vectorize {
        mode: Some(VectorMode::Mono),
        curves: None,
    };
    let prepared = core.batch_prepare(vectorize, None).await.unwrap();
    let events = Recorder::default();
    let summary = core.batch_run(prepared, &events).await.unwrap();
    assert_eq!((summary.done, summary.failed), (1, 0));
    let svg = std::fs::read_to_string(dir.path().join("photos-sh/disc.svg")).unwrap();
    assert!(svg.contains("<svg"), "the result is an SVG file");
    let item = &core.batch_snapshot().items[0];
    assert_eq!(item.output_name.as_deref(), Some("disc.svg"));
    assert!(item.note.as_deref().is_some_and(|n| n.contains("path")));
    let thumb = core.preview(&format!("batch/{}/result", item.id)).unwrap();
    assert_eq!(thumb.mime, "image/svg+xml");
}

#[tokio::test]
async fn the_editor_from_open_to_save() {
    let (base_url, key) = engine_or_skip!();
    let dir = tempfile::tempdir().unwrap();
    let core = core(dir.path(), &base_url, &key);
    let original = write_photo(&dir.path().join("disc.png"), 256, 192);
    let original_bytes = std::fs::read(&original).unwrap();

    let opened = core.editor_open(&original).unwrap();
    assert_eq!(
        (opened.original.width, opened.original.height),
        (Some(256), Some(192))
    );
    let session = opened.session;

    // One model call: the background goes, transparency comes.
    let cutout = core
        .editor_apply(EditorTool::Operation {
            operation: Operation::RemoveBackground {
                background: None,
                crop: false,
                padding: None,
                shadow: None,
                studio: false,
                format: None,
                quality: None,
            },
        })
        .await
        .unwrap();
    assert_eq!(cutout.cursor, 1);
    assert_eq!(cutout.current.content_type, "image/png");
    assert_eq!(
        (cutout.current.width, cutout.current.height),
        (Some(256), Some(192))
    );
    let picture = core.preview(&format!("editor/{session}/1")).unwrap();
    let decoded = image::load_from_memory(&picture.bytes).unwrap().to_rgba8();
    assert_eq!(decoded.get_pixel(2, 2).0[3], 0, "the corner is transparent");
    assert!(decoded.get_pixel(128, 96).0[3] > 200, "the disc is kept");

    // Back to the original, and erase a corner of it with the brush (one
    // more model call). The earlier step is dropped.
    core.editor_undo().unwrap();
    let erased = core
        .editor_apply(EditorTool::Erase {
            strokes: vec![Stroke {
                radius: 10.0,
                points: vec![[20.0, 20.0], [50.0, 24.0]],
            }],
            format: None,
            quality: None,
        })
        .await
        .unwrap();
    assert_eq!((erased.cursor, erased.steps.len()), (1, 1));
    assert_eq!(erased.current.label, "Erase");
    assert_eq!(
        (erased.current.width, erased.current.height),
        (Some(256), Some(192))
    );

    // No model: the next tool works on the erased picture.
    let small = core
        .editor_apply(EditorTool::Operation {
            operation: Operation::Resize {
                width: Some(128),
                height: None,
                fit: None,
                enlarge: false,
                format: Some(Format::Jpg),
                quality: Some(85),
                max_bytes: None,
                background: None,
            },
        })
        .await
        .unwrap();
    assert_eq!(small.cursor, 2);
    assert_eq!(small.current.content_type, "image/jpeg");
    assert_eq!(
        (small.current.width, small.current.height),
        (Some(128), Some(96))
    );
    assert_eq!(small.suggested_name, "disc-edited.jpg");

    // Check without models: metrics and findings for the current step.
    let report = core.editor_check(None, false, false).await.unwrap();
    assert_eq!(report["metrics"]["width"], 128);
    assert!(report["findings"].is_array());
    // And against a marketplace, on the original (a model call for the
    // product mask and one for text; the rules need them).
    core.editor_go_to(0).unwrap();
    let verdict = core
        .editor_check(Some("amazon"), false, false)
        .await
        .unwrap();
    assert_eq!(verdict["marketplace"]["marketplace"], "amazon");
    assert!(verdict["marketplace"]["passed"].is_boolean());
    core.editor_go_to(2).unwrap();

    // Save as: the picture's own type decides the extension.
    let saved = core
        .editor_save(&dir.path().join("final.png"), false)
        .unwrap();
    assert_eq!(saved, dir.path().join("final.png.jpg"));
    assert_eq!(image::image_dimensions(&saved).unwrap(), (128, 96));
    // The erased step is a PNG like the original: saving it there is refused
    // until the user has confirmed.
    core.editor_go_to(1).unwrap();
    let refused = core.editor_save(&original, false).unwrap_err();
    assert_eq!(refused.code, "confirm_replace_original");

    core.editor_close();
    assert_eq!(std::fs::read(&original).unwrap(), original_bytes);
    assert!(saved.exists());
}

/// The real keychain of this machine, with a throwaway entry.
#[test]
fn the_operating_systems_keychain() {
    if std::env::var("PHOTOS_TEST_KEYCHAIN").as_deref() != Ok("1") {
        eprintln!("skipped: set PHOTOS_TEST_KEYCHAIN=1");
        return;
    }
    let service = "sh.photos.desktop.test";
    let account = format!("test-{}", std::process::id());
    let store = OsKeychain::new(service, &account);
    assert_eq!(store.get().unwrap(), None);

    let vault = KeyVault::new(Box::new(OsKeychain::new(service, &account)));
    let status = vault.set("throwaway-0123456789abcdef").unwrap();
    assert_eq!(status.storage, Storage::Keychain);
    assert!(status.keychain_available);
    assert_eq!(status.last4.as_deref(), Some("cdef"));
    // Another reader (the next start of the app) finds it.
    assert_eq!(
        store.get().unwrap().as_deref(),
        Some("throwaway-0123456789abcdef")
    );
    let again = KeyVault::new(Box::new(OsKeychain::new(service, &account)));
    assert_eq!(again.status().storage, Storage::Keychain);

    assert!(!vault.remove().unwrap().set);
    assert_eq!(store.get().unwrap(), None);
}
