//! The core as a whole against the in-process mock server.

use std::path::Path;
use std::sync::Mutex;

use crate::batch::{BatchEvents, BatchItem, BatchSummary, ItemStatus};
use crate::core::{Core, EditorTool};
use crate::keychain::MemoryKeychain;
use crate::mask::Stroke;
use crate::ops::{FaceMode, Format, Operation, Pick};
use crate::settings::Settings;
use crate::testing::{MockServer, Reply, TEST_KEY, write_fake, write_png};

fn core_for(dir: &Path, base_url: &str, keychain: MemoryKeychain) -> Core {
    let core = Core::new(dir.join("config/settings.json"), Box::new(keychain)).unwrap();
    core.set_settings(Settings {
        base_url: base_url.to_string(),
        parallel: 2,
        default_format: None,
    })
    .unwrap();
    core
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

fn upscale() -> Operation {
    Operation::Upscale {
        scale: Some(2.0),
        format: None,
        quality: None,
    }
}

#[tokio::test]
async fn without_a_key_nothing_is_sent() {
    let server = MockServer::start(|_, _| Reply::json(200, "{}")).await;
    let dir = tempfile::tempdir().unwrap();
    let core = core_for(dir.path(), &server.url(), MemoryKeychain::default());
    assert!(!core.key_status().set);
    assert_eq!(core.account().await.unwrap_err().code, "no_key");
    assert_eq!(core.catalog().await.unwrap_err().code, "no_key");
    core.batch_add(&[write_fake(dir.path(), "a.jpg")]).unwrap();
    assert_eq!(
        core.batch_prepare(upscale(), None)
            .await
            .err()
            .unwrap()
            .code,
        "no_key"
    );
    assert!(server.requests().is_empty());

    // Once a key is saved it is used, and the settings file does not hold it.
    let status = core.set_key(TEST_KEY).unwrap();
    assert_eq!(status.last4.as_deref(), Some("1234"));
    core.account().await.unwrap();
    assert_eq!(
        server.requests()[0].header("authorization"),
        Some("Bearer test-key-1234")
    );
    let settings = std::fs::read_to_string(dir.path().join("config/settings.json")).unwrap();
    assert!(!settings.contains(TEST_KEY));
    core.remove_key().unwrap();
    assert_eq!(core.account().await.unwrap_err().code, "no_key");
}

#[tokio::test]
async fn a_batch_is_estimated_checked_and_run() {
    let server = MockServer::start(|r, _| match r.path.as_str() {
        "/api/v1/account" => Reply::json(200, r#"{"credits":9,"prices":{"upscale":4}}"#),
        _ => Reply::png(),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = core_for(
        dir.path(),
        &server.url(),
        MemoryKeychain::with_key(TEST_KEY),
    );
    let files = [
        write_fake(dir.path(), "in/a.jpg"),
        write_fake(dir.path(), "in/b.jpg"),
        write_fake(dir.path(), "in/c.jpg"),
    ];
    let snapshot = core.batch_add(&[dir.path().join("in")]).unwrap();
    assert_eq!(snapshot.items.len(), 3);

    // Three files at 4 credits with 9 left: said before, refused at the start.
    let estimate = core.batch_estimate(&upscale()).await.unwrap();
    assert_eq!((estimate.cost, estimate.credits), (12, Some(9)));
    assert!(!estimate.enough);
    let refused = core.batch_prepare(upscale(), None).await.err().unwrap();
    assert_eq!(refused.code, "insufficient_credits");
    assert!(!dir.path().join("in/photos-sh").exists());
    assert!(server.requests().iter().all(|r| r.method == "GET"));

    // Two files fit.
    core.batch().remove(snapshot.items[2].id).unwrap();
    let prepared = core.batch_prepare(upscale(), None).await.unwrap();
    assert_eq!(prepared.estimate.cost, 8);
    assert_eq!(prepared.parallel, 2);
    assert_eq!(prepared.job.output_dir, dir.path().join("in/photos-sh"));
    let events = Recorder::default();
    let summary = core.batch_run(prepared, &events).await.unwrap();
    assert_eq!((summary.done, summary.failed), (2, 0));
    assert!(dir.path().join("in/photos-sh/a.png").exists());
    assert!(dir.path().join("in/photos-sh/b.png").exists());
    assert!(files.iter().all(|f| f.exists()));
    assert_eq!(events.1.lock().unwrap().len(), 1);

    // Thumbnails are reachable by id only; the fake files are no pictures.
    let id = core.batch_snapshot().items[0].id;
    assert!(core.preview(&format!("batch/{id}/source")).is_err());
    assert!(core.preview("batch/999/source").is_err());
    assert!(core.preview("/etc/passwd").is_err());
    assert!(core.preview("batch/../../etc/passwd").is_err());

    // Nothing failed, so there is nothing to retry.
    assert_eq!(
        core.batch_prepare_retry().await.err().unwrap().code,
        "batch_empty"
    );
}

#[tokio::test]
async fn a_chosen_output_folder_is_used_and_retry_reuses_it() {
    let server = MockServer::start(|r, n| match r.path.as_str() {
        "/api/v1/account" => Reply::json(200, r#"{"unlimited":true}"#),
        _ if n < 3 => Reply::error(422, "unprocessable", "No subject found"),
        _ => Reply::png(),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = core_for(
        dir.path(),
        &server.url(),
        MemoryKeychain::with_key(TEST_KEY),
    );
    assert_eq!(
        core.batch_prepare_retry().await.err().unwrap().code,
        "batch_no_earlier_run"
    );
    core.batch_add(&[write_fake(dir.path(), "a.jpg")]).unwrap();
    let out = dir.path().join("chosen");
    let events = Recorder::default();
    let prepared = core
        .batch_prepare(upscale(), Some(out.clone()))
        .await
        .unwrap();
    let summary = core.batch_run(prepared, &events).await.unwrap();
    assert_eq!(summary.failed, 1);
    assert_eq!(core.batch_snapshot().items[0].status, ItemStatus::Failed);

    let prepared = core.batch_prepare_retry().await.unwrap();
    assert_eq!(prepared.job.output_dir, out);
    assert_eq!(prepared.job.operation, upscale());
    let summary = core.batch_run(prepared, &events).await.unwrap();
    assert_eq!((summary.total, summary.done), (1, 1));
    assert!(out.join("a.png").exists());
}

#[tokio::test]
async fn the_editor_chains_tools_and_serves_its_pictures() {
    let dir = tempfile::tempdir().unwrap();
    let step_png = std::fs::read(write_png(dir.path(), "result.png", 8, 6)).unwrap();
    let server = {
        let step_png = step_png.clone();
        MockServer::start(move |r, _| match r.path.as_str() {
            "/api/v1/check" => Reply::json(200, r#"{"metrics":{"width":8},"findings":[]}"#),
            _ => Reply::image("image/png", &step_png),
        })
        .await
    };
    let core = core_for(
        dir.path(),
        &server.url(),
        MemoryKeychain::with_key(TEST_KEY),
    );
    let original = write_png(dir.path(), "shoe.png", 40, 30);

    assert_eq!(core.editor_undo().unwrap_err().code, "nothing_open");
    assert!(core.editor_snapshot().is_none());
    let opened = core.editor_open(&original).unwrap();
    assert_eq!((opened.cursor, opened.original.width), (0, Some(40)));
    let session = opened.session;

    // First tool: sent the original.
    let first = core
        .editor_apply(EditorTool::Operation {
            operation: Operation::RemoveBackground {
                background: None,
                crop: false,
                padding: None,
                shadow: None,
                format: Some(Format::Png),
                quality: None,
            },
        })
        .await
        .unwrap();
    assert_eq!(first.cursor, 1);
    assert_eq!(first.current.label, "Remove background");
    let requests = server.requests();
    assert_eq!(requests[0].path, "/api/v1/remove-background");
    assert_eq!(requests[0].image_name(), "shoe.png");
    assert_eq!(
        requests[0].part("image").unwrap().bytes,
        std::fs::read(&original).unwrap()
    );

    // Second tool, erase: sent the first result and a mask of its size.
    let second = core
        .editor_apply(EditorTool::Erase {
            strokes: vec![Stroke {
                radius: 2.0,
                points: vec![[1.0, 1.0], [6.0, 4.0]],
            }],
            format: None,
            quality: Some(90),
        })
        .await
        .unwrap();
    assert_eq!((second.cursor, second.current.label.as_str()), (2, "Erase"));
    let erase = &server.requests()[1];
    assert_eq!(erase.path, "/api/v1/erase");
    assert_eq!(erase.query, [("quality".to_string(), "90".to_string())]);
    assert_eq!(erase.image_name(), "step-1.png");
    assert_eq!(erase.part("image").unwrap().bytes, step_png);
    let mask = image::load_from_memory(&erase.part("mask").unwrap().bytes).unwrap();
    assert_eq!((mask.width(), mask.height()), (8, 6));
    assert_eq!(mask.to_luma8().get_pixel(1, 1).0[0], 255);
    assert_eq!(mask.to_luma8().get_pixel(7, 0).0[0], 0);

    // An empty mask is refused before anything is sent.
    let empty = core
        .editor_apply(EditorTool::Erase {
            strokes: vec![],
            format: None,
            quality: None,
        })
        .await
        .unwrap_err();
    assert_eq!(empty.code, "invalid");
    assert_eq!(server.requests().len(), 2);

    // Check looks at the current step and adds none.
    let report = core
        .editor_check(Some("amazon"), false, false)
        .await
        .unwrap();
    assert_eq!(report["metrics"]["width"], 8);
    let check = &server.requests()[2];
    assert_eq!(check.image_name(), "step-2.png");
    assert_eq!(
        check.query,
        [("marketplace".to_string(), "amazon".to_string())]
    );
    assert_eq!(core.editor_snapshot().unwrap().steps.len(), 2);

    // Pictures: the original and each step, for this session only.
    let picture = core.preview(&format!("editor/{session}/0")).unwrap();
    assert_eq!(picture.bytes, std::fs::read(&original).unwrap());
    assert_eq!(
        core.preview(&format!("editor/{session}/2")).unwrap().bytes,
        step_png
    );
    assert!(core.preview(&format!("editor/{session}/9")).is_err());
    assert!(core.preview(&format!("editor/{}/0", session + 1)).is_err());

    // Undo, then a tool: works on the earlier step.
    assert_eq!(core.editor_undo().unwrap().cursor, 1);
    assert_eq!(core.editor_redo().unwrap().cursor, 2);
    assert_eq!(core.editor_go_to(0).unwrap().cursor, 0);
    core.editor_apply(EditorTool::Operation {
        operation: upscale(),
    })
    .await
    .unwrap();
    assert_eq!(server.requests()[3].image_name(), "shoe.png");
    assert_eq!(core.editor_snapshot().unwrap().steps.len(), 1);

    // Save, then close: the steps go, the saved file and the original stay.
    let saved = core
        .editor_save(&dir.path().join("out.png"), false)
        .unwrap();
    assert_eq!(std::fs::read(&saved).unwrap(), step_png);
    assert_eq!(
        core.editor_save(&original, false).unwrap_err().code,
        "confirm_replace_original"
    );
    core.editor_close();
    assert!(core.editor_snapshot().is_none());
    assert!(core.preview(&format!("editor/{session}/0")).is_err());
    assert!(saved.exists() && original.exists());
}

#[tokio::test]
async fn an_api_error_in_the_editor_adds_no_step_and_keeps_its_code() {
    let server =
        MockServer::start(|_, _| Reply::error(402, "payment_required", "Out of credits")).await;
    let dir = tempfile::tempdir().unwrap();
    let core = core_for(
        dir.path(),
        &server.url(),
        MemoryKeychain::with_key(TEST_KEY),
    );
    core.editor_open(&write_png(dir.path(), "shoe.png", 4, 4))
        .unwrap();
    let error = core
        .editor_apply(EditorTool::Operation {
            operation: upscale(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "payment_required");
    assert_eq!(error.status, Some(402));
    assert_eq!(error.message, "Out of credits");
    assert_eq!(core.editor_snapshot().unwrap().cursor, 0);

    // Bad options never leave the app.
    let before = server.requests().len();
    let bad = Operation::Upscale {
        scale: Some(40.0),
        format: None,
        quality: None,
    };
    let error = core
        .editor_apply(EditorTool::Operation { operation: bad })
        .await
        .unwrap_err();
    assert_eq!(error.code, "invalid");
    assert_eq!(server.requests().len(), before);
}

#[tokio::test]
async fn cancelling_the_editor_drops_the_request_in_flight() {
    let server =
        MockServer::start(|_, _| Reply::png().after(std::time::Duration::from_secs(3))).await;
    let dir = tempfile::tempdir().unwrap();
    let core = std::sync::Arc::new(core_for(
        dir.path(),
        &server.url(),
        MemoryKeychain::with_key(TEST_KEY),
    ));
    core.editor_open(&write_png(dir.path(), "shoe.png", 4, 4))
        .unwrap();
    let canceller = {
        let core = core.clone();
        let requests = server.url();
        tokio::spawn(async move {
            let _ = requests;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            core.editor_cancel();
        })
    };
    let started = std::time::Instant::now();
    let error = core
        .editor_apply(EditorTool::Operation {
            operation: upscale(),
        })
        .await
        .unwrap_err();
    canceller.await.unwrap();
    assert_eq!(error.code, "cancelled");
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    // The step list is as it was, and the request was dropped: the server
    // never got to answer it.
    let snapshot = core.editor_snapshot().unwrap();
    assert_eq!((snapshot.cursor, snapshot.steps.len()), (0, 0));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert_eq!((server.requests().len(), server.answered()), (1, 0));

    // The same goes for a check, and a tool can be applied afterwards.
    let canceller = {
        let core = core.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            core.editor_cancel();
        })
    };
    let error = core.editor_check(None, false, false).await.unwrap_err();
    canceller.await.unwrap();
    assert_eq!(error.code, "cancelled");
}

fn pairs(query: &[(String, String)]) -> Vec<(&str, &str)> {
    query
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

/// A request as a test expects it: path, query, the picture's file name.
type Sent = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static str,
);

#[test]
fn the_editors_tools_as_the_window_sends_them() {
    let tool: EditorTool =
        serde_json::from_str(r#"{"tool":"select","points":[[12.5,7],[3,4]],"pick":"smallest"}"#)
            .unwrap();
    assert_eq!(
        tool,
        EditorTool::Select {
            points: vec![[12.5, 7.0], [3.0, 4.0]],
            pick: Some(Pick::Smallest),
        }
    );
    let tool: EditorTool = serde_json::from_str(
        r#"{"tool":"operation","operation":{"kind":"blur_faces","mode":"fill"}}"#,
    )
    .unwrap();
    assert_eq!(
        tool,
        EditorTool::Operation {
            operation: Operation::BlurFaces {
                mode: Some(FaceMode::Fill),
                strength: None,
                format: None,
                quality: None,
            }
        }
    );
    // Points are needed, and nothing else of select's is taken.
    assert!(serde_json::from_str::<EditorTool>(r#"{"tool":"select"}"#).is_err());
    assert!(
        serde_json::from_str::<EditorTool>(r#"{"tool":"select","points":[],"box":"1,2,3,4"}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<EditorTool>(r#"{"tool":"select","points":[[1,2]],"pick":"all"}"#)
            .is_err()
    );
}

#[tokio::test]
async fn the_editor_cuts_out_the_object_at_a_click() {
    let dir = tempfile::tempdir().unwrap();
    let step_png = std::fs::read(write_png(dir.path(), "result.png", 8, 6)).unwrap();
    let server = {
        let step_png = step_png.clone();
        MockServer::start(move |r, _| {
            if r.query.iter().any(|(_, v)| v == "1,1") {
                Reply::error(422, "unprocessable", "nothing was found at those points")
            } else {
                Reply::image("image/png", &step_png)
            }
        })
        .await
    };
    let core = core_for(
        dir.path(),
        &server.url(),
        MemoryKeychain::with_key(TEST_KEY),
    );
    let original = write_png(dir.path(), "desk.png", 40, 30);
    core.editor_open(&original).unwrap();

    // Two clicks on the 40 × 30 original: each goes as the pixel it fell in.
    let cut = core
        .editor_apply(EditorTool::Select {
            points: vec![[20.6, 15.2], [39.9, 29.9]],
            pick: Some(Pick::Largest),
        })
        .await
        .unwrap();
    assert_eq!(
        (cut.cursor, cut.current.label.as_str()),
        (1, "Select object")
    );
    let sent = &server.requests()[0];
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.path, "/api/v1/select");
    assert_eq!(
        pairs(&sent.query),
        [("point", "20,15"), ("point", "39,29"), ("pick", "largest")]
    );
    assert_eq!(sent.image_name(), "desk.png");
    assert_eq!(sent.parts.len(), 1, "the photo and nothing else");

    // The cutout is 8 × 6: a click that was right for the original is now
    // outside the picture, and never leaves the app. Nor does no click.
    let outside = core
        .editor_apply(EditorTool::Select {
            points: vec![[20.0, 3.0]],
            pick: None,
        })
        .await
        .unwrap_err();
    assert_eq!(outside.code, "invalid");
    assert_eq!(outside.message, "A point lies outside the photo.");
    let none = core
        .editor_apply(EditorTool::Select {
            points: vec![],
            pick: None,
        })
        .await
        .unwrap_err();
    assert_eq!(none.code, "invalid");
    assert_eq!(server.requests().len(), 1);

    // Nothing at the point: the API says so, and no step is added.
    let nothing = core
        .editor_apply(EditorTool::Select {
            points: vec![[1.0, 1.0]],
            pick: None,
        })
        .await
        .unwrap_err();
    assert_eq!(
        (nothing.code.as_str(), nothing.status),
        ("unprocessable", Some(422))
    );
    assert_eq!(nothing.message, "nothing was found at those points");
    let again = &server.requests()[1];
    assert_eq!(pairs(&again.query), [("point", "1,1")]);
    assert_eq!(again.image_name(), "step-1.png");
    assert_eq!(core.editor_snapshot().unwrap().steps.len(), 1);
}

#[tokio::test]
async fn the_editor_keeps_each_new_tool_as_a_step_and_refuses_an_svg() {
    let dir = tempfile::tempdir().unwrap();
    let step_png = std::fs::read(write_png(dir.path(), "result.png", 8, 6)).unwrap();
    let server = {
        let step_png = step_png.clone();
        MockServer::start(move |r, _| {
            let reply = Reply::image("image/png", &step_png);
            match r.path.as_str() {
                "/api/v1/blur-faces" => reply.header("x-faces", "2"),
                "/api/v1/restore-faces" => {
                    reply.header("x-faces", "1").header("x-faces-skipped", "1")
                }
                "/api/v1/crop" => reply.header("x-subject-found", "false"),
                _ => reply,
            }
        })
        .await
    };
    let core = core_for(
        dir.path(),
        &server.url(),
        MemoryKeychain::with_key(TEST_KEY),
    );
    core.editor_open(&write_png(dir.path(), "group.png", 40, 30))
        .unwrap();
    let apply = async |json: &str| {
        let operation: Operation = serde_json::from_str(json).unwrap();
        core.editor_apply(EditorTool::Operation { operation }).await
    };

    let hidden = apply(r#"{"kind":"blur_faces","mode":"pixelate","strength":80}"#)
        .await
        .unwrap();
    assert_eq!(hidden.current.label, "Hide faces");
    assert_eq!(hidden.current.note.as_deref(), Some("2 faces hidden"));

    let restored = apply(r#"{"kind":"restore_faces","strength":60}"#)
        .await
        .unwrap();
    assert_eq!(
        restored.current.note.as_deref(),
        Some("1 face restored, 1 left alone (already large and detailed)")
    );

    let cropped =
        apply(r#"{"kind":"crop","aspect":"4:5","focus":"face","zoom":true,"padding":0.6}"#)
            .await
            .unwrap();
    assert_eq!(cropped.current.label, "Crop to 4:5");
    assert_eq!(
        cropped.current.note.as_deref(),
        Some("No subject or face found: the crop is centred")
    );

    let steps = [
        (r#"{"kind":"enhance","sharpen":30}"#, "Enhance"),
        (r#"{"kind":"denoise"}"#, "Denoise"),
        (r#"{"kind":"colorize","strength":70}"#, "Colourise"),
        (
            r#"{"kind":"expand","aspect":"16:9","fill":"white"}"#,
            "Expand to 16:9",
        ),
        (
            r#"{"kind":"watermark","text":"shop.example","opacity":30}"#,
            "Watermark",
        ),
        (
            r#"{"kind":"remove_background","shadow":"contact"}"#,
            "Remove background",
        ),
    ];
    for (json, label) in steps {
        let snapshot = apply(json).await.unwrap();
        assert_eq!(snapshot.current.label, label);
        assert_eq!(snapshot.current.note, None, "{label}");
    }
    assert_eq!(core.editor_snapshot().unwrap().cursor, 9);

    // Each request: its path, its query, and the picture it carried.
    let expected: [Sent; 9] = [
        (
            "/api/v1/blur-faces",
            &[("mode", "pixelate"), ("strength", "80")],
            "group.png",
        ),
        ("/api/v1/restore-faces", &[("strength", "60")], "step-1.png"),
        (
            "/api/v1/crop",
            &[
                ("aspect", "4:5"),
                ("focus", "face"),
                ("zoom", "true"),
                ("padding", "0.6"),
            ],
            "step-2.png",
        ),
        ("/api/v1/enhance", &[("sharpen", "30")], "step-3.png"),
        ("/api/v1/denoise", &[], "step-4.png"),
        ("/api/v1/colorize", &[("strength", "70")], "step-5.png"),
        (
            "/api/v1/expand",
            &[("aspect", "16:9"), ("fill", "white")],
            "step-6.png",
        ),
        (
            "/api/v1/watermark",
            &[("text", "shop.example"), ("opacity", "30")],
            "step-7.png",
        ),
        (
            "/api/v1/remove-background",
            &[("shadow", "contact")],
            "step-8.png",
        ),
    ];
    let requests = server.requests();
    assert_eq!(requests.len(), expected.len());
    for (request, (path, query, image)) in requests.iter().zip(expected) {
        assert_eq!(request.path, path);
        assert_eq!(pairs(&request.query), query, "{path}");
        assert_eq!(request.image_name(), image, "{path}");
    }

    // An SVG is no picture for the next tool: vectorize is for the batch.
    let refused = apply(r#"{"kind":"vectorize"}"#).await.unwrap_err();
    assert_eq!(refused.code, "invalid");
    assert!(refused.message.contains("Batch"));
    assert_eq!(server.requests().len(), 9);
    assert_eq!(core.editor_snapshot().unwrap().steps.len(), 9);
}

#[tokio::test]
async fn a_batch_of_a_new_tool_is_free_until_the_account_lists_a_price() {
    let server = MockServer::start(|r, _| match r.path.as_str() {
        "/api/v1/account" => Reply::json(200, r#"{"credits":0,"prices":{"upscale":4}}"#),
        _ => Reply::image("image/svg+xml", b"<svg/>").header("x-paths", "7"),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let core = core_for(
        dir.path(),
        &server.url(),
        MemoryKeychain::with_key(TEST_KEY),
    );
    core.batch_add(&[
        write_fake(dir.path(), "in/logo.png"),
        write_fake(dir.path(), "in/mark.png"),
    ])
    .unwrap();
    let vectorize = Operation::Vectorize {
        mode: None,
        curves: None,
    };

    // No credits, but nothing to pay: the batch may start.
    let estimate = core.batch_estimate(&vectorize).await.unwrap();
    assert_eq!((estimate.price, estimate.cost), (0, 0));
    assert!(estimate.enough);
    let prepared = core.batch_prepare(vectorize, None).await.unwrap();
    let events = Recorder::default();
    let summary = core.batch_run(prepared, &events).await.unwrap();
    assert_eq!((summary.done, summary.failed), (2, 0));
    assert!(dir.path().join("in/photos-sh/logo.svg").exists());
    assert!(dir.path().join("in/photos-sh/mark.svg").exists());

    // The list shows the SVG itself as the result's thumbnail.
    let item = &core.batch_snapshot().items[0];
    assert_eq!(item.note.as_deref(), Some("7 paths"));
    let thumb = core.preview(&format!("batch/{}/result", item.id)).unwrap();
    assert_eq!(thumb.mime, "image/svg+xml");
    assert_eq!(thumb.bytes, b"<svg/>");
}
