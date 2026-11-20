//! The batch runner against the in-process mock server.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::ops::Format;
use crate::testing::{MockServer, Reply, write_fake};

#[derive(Default)]
struct Recorder {
    items: Mutex<Vec<BatchItem>>,
    summaries: Mutex<Vec<BatchSummary>>,
}

impl BatchEvents for Recorder {
    fn item(&self, item: &BatchItem) {
        self.items.lock().unwrap().push(item.clone());
    }
    fn finished(&self, summary: &BatchSummary) {
        self.summaries.lock().unwrap().push(summary.clone());
    }
}

impl Recorder {
    /// The statuses one file went through, in order.
    fn history(&self, name: &str) -> Vec<ItemStatus> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .filter(|i| i.name == name)
            .map(|i| i.status)
            .collect()
    }
}

fn remove_background() -> Operation {
    Operation::RemoveBackground {
        background: None,
        crop: false,
        padding: None,
        shadow: None,
        model: None,
        studio: false,
        format: None,
        quality: None,
    }
}

fn job(output_dir: &Path) -> Job {
    Job {
        operation: remove_background(),
        output_dir: output_dir.to_path_buf(),
    }
}

fn store_with(dir: &Path, names: &[&str]) -> (BatchStore, Vec<PathBuf>) {
    let paths: Vec<PathBuf> = names.iter().map(|n| write_fake(dir, n)).collect();
    let store = BatchStore::default();
    assert_eq!(store.add_paths(&paths).unwrap(), names.len());
    (store, paths)
}

fn status_of(store: &BatchStore, name: &str) -> ItemStatus {
    store
        .snapshot()
        .items
        .iter()
        .find(|i| i.name == name)
        .unwrap()
        .status
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn processes_every_file_and_names_results_by_source_and_type() {
    let server = MockServer::start(|r, _| match r.image_name().as_str() {
        "b.png" => Reply::image("image/webp", b"B")
            .header("x-text-removed", "2")
            .header("x-text-kept", "0"),
        name => Reply::image("image/png", name.as_bytes()),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["a.jpg", "b.png", "c.heic"]);
    let out = dir.path().join("out");
    let events = Recorder::default();
    let operation = Operation::RemoveBackground {
        background: Some("white".into()),
        crop: true,
        padding: Some(10),
        shadow: None,
        model: None,
        studio: false,
        format: Some(Format::Png),
        quality: None,
    };

    let summary = run(
        &store,
        &server.client(),
        Job {
            operation,
            output_dir: out.clone(),
        },
        Selection::Unfinished,
        3,
        &events,
    )
    .await
    .unwrap();

    assert_eq!(
        (summary.total, summary.done, summary.failed, summary.skipped),
        (3, 3, 0, 0)
    );
    assert!(!summary.cancelled && summary.stopped.is_none());
    assert_eq!(summary.output_dir, out.display().to_string());
    assert_eq!(*events.summaries.lock().unwrap(), [summary]);
    assert_eq!(file_names(&out), ["a.png", "b.webp", "c.png"]);
    assert_eq!(std::fs::read(out.join("a.png")).unwrap(), b"a.jpg");
    assert_eq!(
        events.history("a.jpg"),
        [ItemStatus::Running, ItemStatus::Done]
    );

    let snapshot = store.snapshot();
    assert!(!snapshot.running);
    let b = &snapshot.items[1];
    assert_eq!(b.output_name.as_deref(), Some("b.webp"));
    assert_eq!(
        b.output.as_deref(),
        Some(out.join("b.webp").to_str().unwrap())
    );
    assert_eq!((b.width, b.height, b.bytes), (Some(8), Some(6), Some(1)));
    assert_eq!(b.note.as_deref(), Some("2 text lines removed"));
    assert_eq!(store.output_path(b.id), Some(out.join("b.webp")));

    // Every request was the operation's, with its options.
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for r in &requests {
        assert_eq!(r.path, "/api/v1/remove-background");
        let query: Vec<(&str, &str)> = r
            .query
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            query,
            [
                ("background", "white"),
                ("crop", "true"),
                ("padding", "10"),
                ("format", "png")
            ]
        );
    }
}

#[tokio::test]
async fn vectorize_writes_svg_files_and_says_how_many_paths() {
    let server = MockServer::start(|r, _| match r.image_name().as_str() {
        "logo.png" => Reply::image("image/svg+xml", b"<svg>logo</svg>").header("x-paths", "23"),
        _ => Reply::image("image/svg+xml", b"<svg>icon</svg>").header("x-paths", "1"),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["logo.png", "icon.webp"]);
    let out = dir.path().join("out");
    let events = Recorder::default();
    let operation = Operation::Vectorize {
        mode: Some(crate::ops::VectorMode::Mono),
        curves: Some(crate::ops::Curves::Pixel),
    };

    let summary = run(
        &store,
        &server.client(),
        Job {
            operation,
            output_dir: out.clone(),
        },
        Selection::Unfinished,
        2,
        &events,
    )
    .await
    .unwrap();

    assert_eq!((summary.done, summary.failed), (2, 0));
    assert_eq!(file_names(&out), ["icon.svg", "logo.svg"]);
    assert_eq!(
        std::fs::read(out.join("logo.svg")).unwrap(),
        b"<svg>logo</svg>"
    );
    let items = store.snapshot().items;
    assert_eq!(items[0].output_name.as_deref(), Some("logo.svg"));
    assert_eq!(items[0].note.as_deref(), Some("23 paths"));
    assert_eq!(items[1].note.as_deref(), Some("1 path"));
    // The size the picture was traced at.
    assert_eq!((items[0].width, items[0].height), (Some(8), Some(6)));
    for r in &server.requests() {
        assert_eq!(r.path, "/api/v1/vectorize");
        assert_eq!(
            r.query,
            [
                ("mode".to_string(), "mono".to_string()),
                ("curves".to_string(), "pixel".to_string())
            ]
        );
    }
}

#[tokio::test]
async fn each_file_says_how_many_faces_were_hidden_or_whether_a_subject_was_found() {
    let server = MockServer::start(|r, _| match (r.path.as_str(), r.image_name().as_str()) {
        ("/api/v1/blur-faces", "group.jpg") => Reply::png().header("x-faces", "5"),
        ("/api/v1/blur-faces", _) => Reply::png().header("x-faces", "0"),
        ("/api/v1/crop", "group.jpg") => Reply::png().header("x-subject-found", "true"),
        _ => Reply::png().header("x-subject-found", "false"),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["group.jpg", "landscape.jpg"]);
    let events = Recorder::default();
    let client = server.client();
    let note_of = |name: &str| {
        store
            .snapshot()
            .items
            .iter()
            .find(|i| i.name == name)
            .unwrap()
            .note
            .clone()
    };

    let hide = Operation::BlurFaces {
        mode: Some(crate::ops::FaceMode::Fill),
        strength: Some(70.0),
        format: None,
        quality: None,
    };
    run(
        &store,
        &client,
        Job {
            operation: hide,
            output_dir: dir.path().join("hidden"),
        },
        Selection::Unfinished,
        1,
        &events,
    )
    .await
    .unwrap();
    assert_eq!(note_of("group.jpg").as_deref(), Some("5 faces hidden"));
    // A photo that came back as it was is still "done": the note says so.
    assert_eq!(status_of(&store, "landscape.jpg"), ItemStatus::Done);
    assert_eq!(
        note_of("landscape.jpg").as_deref(),
        Some("No faces found: nothing was hidden")
    );
    assert_eq!(
        server.requests()[0].query,
        [("mode".to_string(), "fill".to_string())]
    );

    store.reset().unwrap();
    let crop = Operation::Crop {
        aspect: "4:5".into(),
        focus: None,
        zoom: true,
        padding: None,
        format: None,
        quality: None,
    };
    run(
        &store,
        &client,
        Job {
            operation: crop,
            output_dir: dir.path().join("cropped"),
        },
        Selection::Unfinished,
        1,
        &events,
    )
    .await
    .unwrap();
    assert_eq!(note_of("group.jpg"), None);
    assert_eq!(
        note_of("landscape.jpg").as_deref(),
        Some("No subject or face found: the crop is centred")
    );
    assert_eq!(
        server.requests()[2].query,
        [
            ("aspect".to_string(), "4:5".to_string()),
            ("zoom".to_string(), "true".to_string())
        ]
    );

    // Remove background returns a photo without a subject as it was.
    store.reset().unwrap();
    run(
        &store,
        &client,
        Job {
            operation: remove_background(),
            output_dir: dir.path().join("cutouts"),
        },
        Selection::Unfinished,
        1,
        &events,
    )
    .await
    .unwrap();
    assert_eq!(status_of(&store, "landscape.jpg"), ItemStatus::Done);
    assert_eq!(
        note_of("landscape.jpg").as_deref(),
        Some("No subject found: nothing was cut out")
    );
}

#[tokio::test]
async fn name_collisions_get_numbers_and_nothing_is_overwritten() {
    let server = MockServer::start(|r, _| {
        let body = r.part("image").unwrap().bytes.clone();
        Reply::image("image/jpeg", &body)
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    // Two sources with one name, and results going to the first one's folder.
    let (store, sources) = store_with(dir.path(), &["one/shoe.jpg", "two/shoe.jpg"]);
    let out = dir.path().join("one");
    let events = Recorder::default();

    let summary = run(
        &store,
        &server.client(),
        job(&out),
        Selection::Unfinished,
        1,
        &events,
    )
    .await
    .unwrap();
    assert_eq!(summary.done, 2);
    assert_eq!(
        file_names(&out),
        ["shoe-2.jpg", "shoe-3.jpg", "shoe.jpg"],
        "the source keeps its name; results take the next free ones"
    );
    // The sources are untouched.
    assert_eq!(
        std::fs::read(&sources[0]).unwrap(),
        b"fake image one/shoe.jpg"
    );
    assert_eq!(
        std::fs::read(&sources[1]).unwrap(),
        b"fake image two/shoe.jpg"
    );
    assert_eq!(
        std::fs::read(out.join("shoe-2.jpg")).unwrap(),
        b"fake image one/shoe.jpg"
    );
    assert_eq!(
        std::fs::read(out.join("shoe-3.jpg")).unwrap(),
        b"fake image two/shoe.jpg"
    );

    // Running the same files again adds new results beside the old ones.
    store.reset().unwrap();
    run(
        &store,
        &server.client(),
        job(&out),
        Selection::Unfinished,
        2,
        &events,
    )
    .await
    .unwrap();
    assert_eq!(file_names(&out).len(), 5);
    assert_eq!(
        std::fs::read(out.join("shoe.jpg")).unwrap(),
        b"fake image one/shoe.jpg"
    );
}

#[tokio::test]
async fn a_failed_file_is_marked_and_the_batch_goes_on() {
    let server = MockServer::start(|r, _| match r.image_name().as_str() {
        "b.heic" => Reply::error(415, "unsupported_format", "HEIC is not supported here"),
        "c.png" => Reply::error(500, "internal", "Something broke"),
        _ => Reply::png(),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["a.jpg", "b.heic", "c.png", "d.jpg"]);
    // A source that disappears before its turn fails alone, too.
    std::fs::remove_file(dir.path().join("d.jpg")).unwrap();
    let out = dir.path().join("out");
    let events = Recorder::default();

    let summary = run(
        &store,
        &server.client(),
        job(&out),
        Selection::Unfinished,
        2,
        &events,
    )
    .await
    .unwrap();

    assert_eq!(
        (summary.total, summary.done, summary.failed, summary.skipped),
        (4, 1, 3, 0)
    );
    assert!(summary.stopped.is_none() && !summary.cancelled);
    let items = store.snapshot().items;
    assert_eq!(items[0].status, ItemStatus::Done);
    assert_eq!(
        items[1].error,
        Some(ItemError {
            code: "unsupported_format".into(),
            message: "HEIC is not supported here".into()
        })
    );
    assert_eq!(items[2].error.as_ref().unwrap().code, "internal");
    assert_eq!(items[3].error.as_ref().unwrap().code, "io");
    assert_eq!(file_names(&out), ["a.png"]);
}

#[tokio::test]
async fn out_of_credits_stops_the_batch() {
    for (status, code) in [
        (402, "payment_required"),
        (401, "unauthorized"),
        (403, "account_suspended"),
    ] {
        let server = MockServer::start(move |r, _| match r.image_name().as_str() {
            "a.jpg" => Reply::png(),
            _ => Reply::error(status, code, "No more"),
        })
        .await;
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store_with(dir.path(), &["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
        let events = Recorder::default();

        let summary = run(
            &store,
            &server.client(),
            job(&dir.path().join("out")),
            Selection::Unfinished,
            1,
            &events,
        )
        .await
        .unwrap();

        assert_eq!(
            summary.stopped,
            Some(ItemError {
                code: code.into(),
                message: "No more".into()
            })
        );
        assert!(!summary.cancelled);
        assert_eq!(
            (summary.done, summary.failed, summary.skipped),
            (1, 1, 2),
            "{code}"
        );
        assert_eq!(status_of(&store, "a.jpg"), ItemStatus::Done);
        assert_eq!(status_of(&store, "b.jpg"), ItemStatus::Failed);
        assert_eq!(status_of(&store, "c.jpg"), ItemStatus::Skipped);
        assert_eq!(status_of(&store, "d.jpg"), ItemStatus::Skipped);
        assert_eq!(events.history("d.jpg"), [ItemStatus::Skipped]);
        assert_eq!(
            server.requests().len(),
            2,
            "nothing is sent after the stop ({code})"
        );
    }
}

/// Cancels the batch once `running` files are in flight.
fn cancel_when_running(store: &Arc<BatchStore>, running: usize) -> tokio::task::JoinHandle<()> {
    let store = store.clone();
    tokio::spawn(async move {
        loop {
            let now = store
                .snapshot()
                .items
                .iter()
                .filter(|i| i.status == ItemStatus::Running)
                .count();
            if now == running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        // Let the requests reach the server.
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(store.is_running());
        // The list cannot change under a running batch.
        assert_eq!(store.clear(), Err(BatchError::Running));
        assert_eq!(store.reset(), Err(BatchError::Running));
        assert_eq!(store.add_paths(&[]), Err(BatchError::Running));
        assert!(store.cancel());
    })
}

#[tokio::test]
async fn cancelling_drops_the_files_in_flight_and_writes_nothing_for_them() {
    let slow = Arc::new(AtomicBool::new(true));
    let server = {
        let slow = slow.clone();
        MockServer::start(move |_, _| {
            if slow.load(Ordering::SeqCst) {
                // Holds the response open far longer than the test may take.
                Reply::png().after(Duration::from_secs(20))
            } else {
                Reply::png()
            }
        })
        .await
    };
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["a.jpg", "b.jpg", "c.jpg", "d.jpg", "e.jpg"]);
    let store = Arc::new(store);
    let out = dir.path().join("out");
    let events = Recorder::default();
    let client = server.client();
    assert!(!store.cancel(), "nothing to cancel before the run");

    let canceller = cancel_when_running(&store, 2);
    let started = Instant::now();
    let summary = run(
        &store,
        &client,
        job(&out),
        Selection::Unfinished,
        2,
        &events,
    )
    .await
    .unwrap();
    canceller.await.unwrap();

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "cancel does not wait for the server"
    );
    assert!(summary.cancelled && summary.stopped.is_none());
    assert_eq!(
        (
            summary.total,
            summary.done,
            summary.failed,
            summary.aborted,
            summary.skipped
        ),
        (5, 0, 0, 2, 3)
    );
    assert_eq!(status_of(&store, "a.jpg"), ItemStatus::Cancelled);
    assert_eq!(status_of(&store, "b.jpg"), ItemStatus::Cancelled);
    assert_eq!(status_of(&store, "c.jpg"), ItemStatus::Skipped);
    assert_eq!(
        events.history("a.jpg"),
        [ItemStatus::Running, ItemStatus::Cancelled]
    );
    assert_eq!(events.history("e.jpg"), [ItemStatus::Skipped]);
    let a = &store.snapshot().items[0];
    assert_eq!((a.output.as_ref(), a.error.as_ref()), (None, None));
    assert!(!store.is_running());
    // Nothing was written, not even a partial file.
    assert!(file_names(&out).is_empty());
    assert_eq!(server.requests().len(), 2);
    // The server saw its clients go and dropped the two requests.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(server.answered(), 0);

    // "Retry failed" takes the cancelled and the skipped files alike.
    assert_eq!(store.count(Selection::Failed), 5);
    slow.store(false, Ordering::SeqCst);
    let again = run(
        &store,
        &client,
        store.last_job().unwrap(),
        Selection::Failed,
        2,
        &events,
    )
    .await
    .unwrap();
    assert!(!again.cancelled);
    assert_eq!((again.total, again.done, again.aborted), (5, 5, 0));
    assert_eq!(file_names(&out).len(), 5);
    assert_eq!(
        events.history("a.jpg"),
        [
            ItemStatus::Running,
            ItemStatus::Cancelled,
            ItemStatus::Queued,
            ItemStatus::Running,
            ItemStatus::Done
        ]
    );
}

#[tokio::test]
async fn files_finished_before_the_cancel_stay_done() {
    let server = MockServer::start(|r, _| match r.image_name().as_str() {
        "a.jpg" => Reply::png(),
        _ => Reply::png().after(Duration::from_secs(20)),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["a.jpg", "b.jpg", "c.jpg"]);
    let store = Arc::new(store);
    let out = dir.path().join("out");
    let events = Recorder::default();

    // One at a time: a.jpg finishes, b.jpg is under way when cancel comes.
    let canceller = {
        let store = store.clone();
        tokio::spawn(async move {
            while status_of(&store, "b.jpg") != ItemStatus::Running {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            assert!(store.cancel());
        })
    };
    let summary = run(
        &store,
        &server.client(),
        job(&out),
        Selection::Unfinished,
        1,
        &events,
    )
    .await
    .unwrap();
    canceller.await.unwrap();

    assert_eq!((summary.done, summary.aborted, summary.skipped), (1, 1, 1));
    assert_eq!(status_of(&store, "a.jpg"), ItemStatus::Done);
    assert_eq!(status_of(&store, "b.jpg"), ItemStatus::Cancelled);
    assert_eq!(status_of(&store, "c.jpg"), ItemStatus::Skipped);
    assert_eq!(file_names(&out), ["a.png"]);
    // What was done is not part of a retry.
    assert_eq!(store.count(Selection::Failed), 2);
}

#[tokio::test]
async fn cancelling_ends_a_wait_to_retry_a_busy_server() {
    let server = MockServer::start(|_, _| Reply::busy(Some(30))).await;
    let patient = crate::api::RetryDelays {
        first: Duration::from_secs(30),
        retry_after_unit: Duration::from_secs(1),
        cap: Duration::from_secs(30),
    };
    let client = server.client().with_delays(patient);
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["a.jpg", "b.jpg"]);
    let store = Arc::new(store);
    let out = dir.path().join("out");
    let events = Recorder::default();

    // By the time the cancel comes, the file has been answered "busy" and
    // is waiting 30 seconds to try again.
    let canceller = cancel_when_running(&store, 1);
    let started = Instant::now();
    let summary = run(
        &store,
        &client,
        job(&out),
        Selection::Unfinished,
        1,
        &events,
    )
    .await
    .unwrap();
    canceller.await.unwrap();

    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(summary.cancelled);
    assert_eq!(
        (summary.aborted, summary.skipped, summary.failed),
        (1, 1, 0)
    );
    assert_eq!(status_of(&store, "a.jpg"), ItemStatus::Cancelled);
    assert_eq!(status_of(&store, "b.jpg"), ItemStatus::Skipped);
    assert_eq!(server.requests().len(), 1, "no retry after the cancel");
    assert!(file_names(&out).is_empty());
}

#[tokio::test]
async fn an_error_that_stops_the_batch_lets_files_in_flight_finish() {
    let server = MockServer::start(|r, _| match r.image_name().as_str() {
        "a.jpg" => Reply::error(402, "payment_required", "No credits left"),
        "b.jpg" => Reply::png().after(Duration::from_millis(250)),
        _ => Reply::png(),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["a.jpg", "b.jpg", "c.jpg"]);
    let out = dir.path().join("out");
    let events = Recorder::default();
    let summary = run(
        &store,
        &server.client(),
        job(&out),
        Selection::Unfinished,
        2,
        &events,
    )
    .await
    .unwrap();
    // b.jpg was under way when a.jpg was refused: it is not thrown away.
    assert_eq!(summary.stopped.as_ref().unwrap().code, "payment_required");
    assert!(!summary.cancelled);
    assert_eq!(
        (
            summary.done,
            summary.failed,
            summary.aborted,
            summary.skipped
        ),
        (1, 1, 0, 1)
    );
    assert_eq!(status_of(&store, "b.jpg"), ItemStatus::Done);
    assert_eq!(status_of(&store, "c.jpg"), ItemStatus::Skipped);
    assert_eq!(file_names(&out), ["b.png"]);
}

#[tokio::test]
async fn retry_failed_runs_only_what_failed_or_was_skipped() {
    let broken = Arc::new(AtomicBool::new(true));
    let server = {
        let broken = broken.clone();
        MockServer::start(move |r, _| {
            if broken.load(Ordering::SeqCst) && r.image_name() != "a.jpg" {
                Reply::error(500, "internal", "Something broke")
            } else {
                Reply::png()
            }
        })
        .await
    };
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["a.jpg", "b.jpg", "c.jpg"]);
    let out = dir.path().join("out");
    let events = Recorder::default();
    let client = server.client();

    let first = run(
        &store,
        &client,
        job(&out),
        Selection::Unfinished,
        3,
        &events,
    )
    .await
    .unwrap();
    assert_eq!((first.done, first.failed), (1, 2));
    assert_eq!(store.count(Selection::Failed), 2);
    assert_eq!(store.last_job(), Some(job(&out)));

    // A file added after the run is not part of "retry failed".
    store
        .add_paths(&[write_fake(dir.path(), "new.jpg")])
        .unwrap();
    broken.store(false, Ordering::SeqCst);
    let again = run(
        &store,
        &client,
        store.last_job().unwrap(),
        Selection::Failed,
        3,
        &events,
    )
    .await
    .unwrap();
    assert_eq!((again.total, again.done, again.failed), (2, 2, 0));
    assert_eq!(status_of(&store, "new.jpg"), ItemStatus::Queued);
    assert_eq!(
        events.history("b.jpg"),
        [
            ItemStatus::Running,
            ItemStatus::Failed,
            ItemStatus::Queued,
            ItemStatus::Running,
            ItemStatus::Done
        ]
    );
    let sent: Vec<String> = server.requests().iter().map(|r| r.image_name()).collect();
    assert_eq!(sent.iter().filter(|n| *n == "a.jpg").count(), 1);
    assert_eq!(sent.len(), 5);
    assert_eq!(file_names(&out), ["a.png", "b.png", "c.png"]);

    // With nothing failed there is nothing to retry.
    let nothing = run(&store, &client, job(&out), Selection::Failed, 3, &events).await;
    assert_eq!(nothing, Err(BatchError::Empty));
    assert!(!store.is_running());
}

#[tokio::test]
async fn a_busy_server_is_tried_again_within_the_batch() {
    let server = MockServer::start(|_, n| {
        if n == 0 {
            Reply::busy(Some(1))
        } else {
            Reply::png()
        }
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["a.jpg", "b.jpg"]);
    let events = Recorder::default();
    let summary = run(
        &store,
        &server.client(),
        job(&dir.path().join("out")),
        Selection::Unfinished,
        1,
        &events,
    )
    .await
    .unwrap();
    assert_eq!((summary.done, summary.failed), (2, 0));
    assert_eq!(server.requests().len(), 3);
    // Still busy after the retries: that file fails, the batch goes on.
    let server = MockServer::start(|r, _| match r.image_name().as_str() {
        "a.jpg" => Reply::busy(None),
        _ => Reply::png(),
    })
    .await;
    store.reset().unwrap();
    let summary = run(
        &store,
        &server.client(),
        job(&dir.path().join("out2")),
        Selection::Unfinished,
        1,
        &events,
    )
    .await
    .unwrap();
    assert_eq!((summary.done, summary.failed), (1, 1));
    assert!(summary.stopped.is_none());
    assert_eq!(
        store.snapshot().items[0].error.as_ref().unwrap().code,
        "busy"
    );
}

#[tokio::test]
async fn only_so_many_files_are_in_flight_at_once() {
    let arrivals: Arc<Mutex<Vec<Instant>>> = Arc::default();
    let server = {
        let arrivals = arrivals.clone();
        MockServer::start(move |_, _| {
            arrivals.lock().unwrap().push(Instant::now());
            Reply::png().after(Duration::from_millis(150))
        })
        .await
    };
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = store_with(dir.path(), &["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    let events = Recorder::default();
    run(
        &store,
        &server.client(),
        job(&dir.path().join("out")),
        Selection::Unfinished,
        2,
        &events,
    )
    .await
    .unwrap();
    let arrivals = arrivals.lock().unwrap();
    assert_eq!(arrivals.len(), 4);
    // The second arrives while the first is being worked on; the third has
    // to wait for one of them to finish.
    assert!(arrivals[1] - arrivals[0] < Duration::from_millis(120));
    assert!(arrivals[2] - arrivals[0] >= Duration::from_millis(140));
}

#[tokio::test]
async fn the_preflight_estimates_and_refuses_a_batch_the_balance_cannot_pay() {
    let server = MockServer::start(|_, _| {
        Reply::json(
            200,
            r#"{"account":"acc_1","credits":10,"prices":{"remove-background":2,"upscale":4}}"#,
        )
    })
    .await;
    let client = server.client();

    let estimate = preflight(&client, &remove_background(), 5).await.unwrap();
    assert_eq!((estimate.files, estimate.price, estimate.cost), (5, 2, 10));
    assert_eq!(estimate.credits, Some(10));
    assert!(estimate.enough);

    let error = preflight(&client, &remove_background(), 6)
        .await
        .unwrap_err();
    assert_eq!(
        error,
        BatchError::InsufficientCredits {
            cost: 12,
            credits: 10
        }
    );
    assert_eq!(error.code(), "insufficient_credits");
    assert!(
        error
            .to_string()
            .starts_with("This batch costs 12 credits and the account has 10 left.")
    );

    // Bad options are refused before anything is sent.
    let before = server.requests().len();
    let bad = Operation::Upscale {
        scale: Some(9.0),
        format: None,
        quality: None,
    };
    assert!(matches!(
        preflight(&client, &bad, 1).await,
        Err(BatchError::Invalid(_))
    ));
    assert_eq!(server.requests().len(), before);
}

#[tokio::test]
async fn a_tool_is_priced_by_its_endpoints_name_and_is_free_until_it_is_listed() {
    let server = MockServer::start(|_, _| {
        Reply::json(
            200,
            r#"{"credits":10,"prices":{"restore-faces":3,"blur-faces":1,"crop":2}}"#,
        )
    })
    .await;
    let client = server.client();
    let restore = Operation::RestoreFaces {
        strength: None,
        format: None,
        quality: None,
    };
    let estimate = preflight(&client, &restore, 3).await.unwrap();
    assert_eq!((estimate.price, estimate.cost), (3, 9));
    assert_eq!(
        preflight(&client, &restore, 4).await.unwrap_err(),
        BatchError::InsufficientCredits {
            cost: 12,
            credits: 10
        }
    );
    let crop = Operation::Crop {
        aspect: "1:1".into(),
        focus: None,
        zoom: false,
        padding: None,
        format: None,
        quality: None,
    };
    assert_eq!(preflight(&client, &crop, 5).await.unwrap().cost, 10);

    // Not in the price list: nothing is charged for it, whatever the balance.
    let denoise = Operation::Denoise {
        strength: Some(50.0),
        format: None,
        quality: None,
    };
    let estimate = preflight(&client, &denoise, 500).await.unwrap();
    assert_eq!((estimate.price, estimate.cost), (0, 0));
    assert!(estimate.enough);

    // Bad options are refused before the account is asked.
    let before = server.requests().len();
    let bad = Operation::Watermark {
        text: "  ".into(),
        color: None,
        position: None,
        scale: None,
        opacity: None,
        format: None,
        quality: None,
    };
    assert!(matches!(
        preflight(&client, &bad, 1).await,
        Err(BatchError::Invalid(_))
    ));
    assert_eq!(server.requests().len(), before);
}

#[tokio::test]
async fn the_preflight_passes_unlimited_keys_and_reports_a_refused_key() {
    let server =
        MockServer::start(|_, _| Reply::json(200, r#"{"account":"sdk","unlimited":true}"#)).await;
    let estimate = preflight(&server.client(), &remove_background(), 5000)
        .await
        .unwrap();
    assert!(estimate.unlimited && estimate.enough);

    let server =
        MockServer::start(|_, _| Reply::error(401, "unauthorized", "Unknown API key")).await;
    let error = preflight(&server.client(), &remove_background(), 1)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "unauthorized");
}

#[tokio::test]
async fn the_list_takes_each_file_once_and_knows_its_default_output_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (store, paths) = store_with(dir.path(), &["in/a.jpg", "in/b.jpg"]);
    assert_eq!(store.add_paths(&[dir.path().join("in")]).unwrap(), 0);
    let snapshot = store.snapshot();
    assert_eq!(snapshot.items.len(), 2);
    assert_eq!(snapshot.items[0].source, paths[0].display().to_string());
    assert_eq!(
        snapshot.default_output_dir,
        Some(dir.path().join("in/photos-sh").display().to_string())
    );
    assert_eq!(
        store.source_path(snapshot.items[1].id),
        Some(paths[1].clone())
    );
    assert_eq!(store.output_path(snapshot.items[1].id), None);

    store.remove(snapshot.items[0].id).unwrap();
    assert_eq!(store.snapshot().items.len(), 1);
    store.clear().unwrap();
    assert!(store.snapshot().items.is_empty());
    assert_eq!(store.snapshot().default_output_dir, None);

    // An empty list cannot be run.
    let server = MockServer::start(|_, _| Reply::png()).await;
    let events = Recorder::default();
    let result = run(
        &store,
        &server.client(),
        job(dir.path()),
        Selection::Unfinished,
        3,
        &events,
    )
    .await;
    assert_eq!(result, Err(BatchError::Empty));
}
