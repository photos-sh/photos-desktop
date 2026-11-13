//! The client against the in-process mock server.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use super::client::{USER_AGENT, normalize_base_url};
use super::{ApiClient, ApiError, ExtraPart, RetryDelays};
use crate::testing::{MockServer, Reply, TEST_KEY, client_for, dead_url, tiny_delays, write_fake};

fn query(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[tokio::test]
async fn sends_the_image_as_multipart_with_auth_and_query() {
    let server = MockServer::start(|_, _| {
        Reply::image("image/webp", b"RESULT")
            .header("x-image-quality", "82")
            .header("server-timing", "decode;dur=3")
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let image = write_fake(dir.path(), "shoe one.jpg");

    let result = server
        .client()
        .process(
            "/v1/resize",
            &query(&[("width", "800"), ("enlarge", "true"), ("format", "webp")]),
            &image,
            &[],
            None,
        )
        .await
        .unwrap();

    assert_eq!(result.bytes, b"RESULT");
    assert_eq!(result.content_type, "image/webp");
    assert_eq!(result.extension(), "webp");
    assert_eq!((result.width, result.height), (Some(8), Some(6)));
    assert_eq!(result.quality, Some(82));
    assert_eq!(result.server_timing.as_deref(), Some("decode;dur=3"));
    assert_eq!(result.text_removed, None);
    assert_eq!(result.marketplace_passed, None);

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    let r = &requests[0];
    assert_eq!(r.method, "POST");
    assert_eq!(r.path, "/api/v1/resize");
    assert_eq!(
        r.query,
        query(&[("width", "800"), ("enlarge", "true"), ("format", "webp")])
    );
    assert_eq!(
        r.header("authorization"),
        Some(format!("Bearer {TEST_KEY}").as_str())
    );
    assert_eq!(r.header("user-agent"), Some(USER_AGENT));
    assert!(USER_AGENT.starts_with("photos-sh-desktop/0."));
    assert_eq!(r.parts.len(), 1);
    let part = r.part("image").unwrap();
    assert_eq!(part.file_name.as_deref(), Some("shoe one.jpg"));
    assert_eq!(part.bytes, b"fake image shoe one.jpg");
}

#[tokio::test]
async fn sends_extra_parts_and_reads_the_operation_headers() {
    let server = MockServer::start(|r, _| match r.path.as_str() {
        "/api/v1/remove-text" => Reply::png()
            .header("x-text-removed", "3")
            .header("x-text-kept", "1"),
        "/api/v1/fix" => Reply::image("image/jpeg", b"J")
            .header("x-marketplace-passed", "false")
            .header("x-marketplace-findings", "too_small, text_overlay"),
        _ => Reply::png()
            .header("x-marketplace-passed", "true")
            .header("x-marketplace-findings", ""),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let image = write_fake(dir.path(), "a.png");
    let client = server.client();

    let mask = ExtraPart {
        name: "mask",
        file_name: "mask.png",
        mime: "image/png",
        bytes: b"MASK".to_vec(),
    };
    let erased = client
        .process("/v1/erase", &[], &image, &[mask], None)
        .await
        .unwrap();
    assert_eq!(erased.marketplace_passed, Some(true));
    assert_eq!(erased.marketplace_findings, Some(vec![]));
    let sent = &server.requests()[0];
    assert_eq!(sent.parts.len(), 2);
    let mask = sent.part("mask").unwrap();
    assert_eq!(mask.file_name.as_deref(), Some("mask.png"));
    assert_eq!(mask.content_type.as_deref(), Some("image/png"));
    assert_eq!(mask.bytes, b"MASK");

    let text = client
        .process("/v1/remove-text", &[], &image, &[], None)
        .await
        .unwrap();
    assert_eq!((text.text_removed, text.text_kept), (Some(3), Some(1)));

    let fixed = client
        .process(
            "/v1/fix",
            &query(&[("marketplace", "amazon")]),
            &image,
            &[],
            None,
        )
        .await
        .unwrap();
    assert_eq!(fixed.marketplace_passed, Some(false));
    assert_eq!(
        fixed.marketplace_findings,
        Some(vec!["too_small".to_string(), "text_overlay".to_string()])
    );
    assert_eq!(fixed.extension(), "jpg");
}

#[tokio::test]
async fn reads_what_the_face_crop_and_vector_tools_report() {
    let server = MockServer::start(|r, _| match r.path.as_str() {
        "/api/v1/blur-faces" => Reply::png().header("x-faces", "3"),
        "/api/v1/restore-faces" => Reply::png()
            .header("x-faces", "1")
            .header("x-faces-skipped", "2"),
        "/api/v1/crop" => Reply::image("image/jpeg", b"J").header(
            "x-subject-found",
            if r.query.iter().any(|(k, _)| k == "zoom") {
                "true"
            } else {
                "false"
            },
        ),
        "/api/v1/vectorize" => Reply::image("image/svg+xml", b"<svg/>").header("x-paths", "23"),
        _ => Reply::png(),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let image = write_fake(dir.path(), "a.jpg");
    let client = server.client();
    let send = async |endpoint: &str, pairs: &[(&str, &str)]| {
        client
            .process(endpoint, &query(pairs), &image, &[], None)
            .await
            .unwrap()
    };

    let hidden = send("/v1/blur-faces", &[("mode", "fill")]).await;
    assert_eq!((hidden.faces, hidden.faces_skipped), (Some(3), None));
    assert_eq!(hidden.note().as_deref(), Some("3 faces hidden"));

    let restored = send("/v1/restore-faces", &[]).await;
    assert_eq!((restored.faces, restored.faces_skipped), (Some(1), Some(2)));
    assert_eq!(
        restored.note().as_deref(),
        Some("1 face restored, 2 left alone (already large and detailed)")
    );

    let centred = send("/v1/crop", &[("aspect", "4:5")]).await;
    assert_eq!(centred.subject_found, Some(false));
    assert_eq!(
        centred.note().as_deref(),
        Some("No subject or face found: the crop is centred")
    );
    let found = send("/v1/crop", &[("aspect", "4:5"), ("zoom", "true")]).await;
    assert_eq!(found.subject_found, Some(true));
    assert_eq!(found.note(), None);

    // An SVG is an image as far as the client goes, with its own extension.
    let traced = send("/v1/vectorize", &[("mode", "mono")]).await;
    assert_eq!(traced.content_type, "image/svg+xml");
    assert_eq!(traced.extension(), "svg");
    assert_eq!(traced.bytes, b"<svg/>");
    assert_eq!(traced.paths, Some(23));
    assert_eq!(traced.note().as_deref(), Some("23 paths"));

    // A tool that reports none of this leaves it all unset.
    let plain = send("/v1/enhance", &[]).await;
    assert_eq!(
        (
            plain.faces,
            plain.faces_skipped,
            plain.subject_found,
            plain.paths
        ),
        (None, None, None, None)
    );
    assert_eq!(plain.note(), None);
}

#[tokio::test]
async fn reads_the_account_presets_and_marketplaces() {
    let server = MockServer::start(|r, _| match r.path.as_str() {
        "/api/v1/account" => Reply::json(
            200,
            r#"{"account":"acc_1","key":"key_9","credits":42,"prices":{"resize":1,"upscale":4}}"#,
        ),
        "/api/v1/presets" => Reply::json(200, r#"[{"name":"web","description":"Web-ready"}]"#),
        "/api/v1/marketplaces" => Reply::json(
            200,
            r#"[{"name":"amazon","title":"Amazon","checked":"2026-11-06","rules":[]}]"#,
        ),
        _ => Reply::error(404, "not_found", "no such route"),
    })
    .await;
    let client = server.client();

    let account = client.account().await.unwrap();
    assert_eq!(account.credits, Some(42));
    assert!(!account.unlimited);
    assert_eq!(account.prices["upscale"], 4);

    let presets = client.presets().await.unwrap();
    assert_eq!(presets[0].name, "web");
    let marketplaces = client.marketplaces().await.unwrap();
    assert_eq!(marketplaces[0].title, "Amazon");
    assert_eq!(marketplaces[0].extra["checked"], "2026-11-06");

    let requests = server.requests();
    assert!(requests.iter().all(|r| r.method == "GET"));
    assert!(
        requests
            .iter()
            .all(|r| r.header("authorization") == Some("Bearer test-key-1234"))
    );
}

#[tokio::test]
async fn inspect_returns_the_json() {
    let server = MockServer::start(|_, _| {
        Reply::json(
            200,
            r#"{"metrics":{"width":10},"findings":[{"code":"soft"}]}"#,
        )
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let image = write_fake(dir.path(), "a.png");
    let report: serde_json::Value = server
        .client()
        .inspect(
            "/v1/check",
            &query(&[("marketplace", "etsy")]),
            &image,
            None,
        )
        .await
        .unwrap();
    assert_eq!(report["findings"][0]["code"], "soft");
    assert_eq!(
        server.requests()[0].query,
        query(&[("marketplace", "etsy")])
    );
}

#[tokio::test]
async fn maps_api_errors_by_code_and_does_not_retry_them() {
    for (status, code) in [
        (400, "bad_request"),
        (401, "unauthorized"),
        (402, "payment_required"),
        (403, "account_suspended"),
        (413, "payload_too_large"),
        (415, "unsupported_format"),
        (422, "unprocessable"),
        (500, "internal"),
        (503, "model_unavailable"),
    ] {
        let server = MockServer::start(move |_, _| Reply::error(status, code, "Nope")).await;
        let dir = tempfile::tempdir().unwrap();
        let image = write_fake(dir.path(), "a.png");
        let error = server
            .client()
            .process("/v1/upscale", &[], &image, &[], None)
            .await
            .unwrap_err();
        assert_eq!(error.code(), code);
        assert_eq!(error.status(), Some(status));
        assert_eq!(error.to_string(), "Nope");
        assert_eq!(server.requests().len(), 1, "{code} must not be retried");
    }
}

#[tokio::test]
async fn a_non_json_error_is_an_api_error_with_the_start_of_the_body() {
    let server = MockServer::start(|_, _| Reply {
        status: 502,
        headers: vec![],
        body: b"<html>Bad Gateway</html>".to_vec(),
        delay: Duration::ZERO,
    })
    .await;
    let error = server.client().account().await.unwrap_err();
    assert_eq!(error.code(), "unknown");
    assert_eq!(error.status(), Some(502));
    assert_eq!(error.to_string(), "<html>Bad Gateway</html>");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn retries_busy_then_succeeds_sending_the_file_again() {
    let server = MockServer::start(|_, n| {
        if n < 2 {
            Reply::busy(Some(2))
        } else {
            Reply::png()
        }
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let image = write_fake(dir.path(), "a.png");
    let result = server
        .client()
        .process("/v1/remove-background", &[], &image, &[], None)
        .await
        .unwrap();
    assert_eq!(result.bytes, b"PNG-RESULT");
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    // Every attempt carried the whole file.
    assert!(
        requests
            .iter()
            .all(|r| r.part("image").unwrap().bytes == b"fake image a.png")
    );
}

#[tokio::test]
async fn gives_up_on_busy_after_the_retries_and_keeps_retry_after() {
    let server = MockServer::start(|_, _| Reply::busy(Some(7))).await;
    let error = server.client().account().await.unwrap_err();
    assert_eq!(error.code(), "busy");
    assert_eq!(error.retry_after(), Some(7));
    assert_eq!(server.requests().len(), 3, "one try and two retries");

    let server = MockServer::start(|_, _| Reply::busy(None)).await;
    let error = server.client().with_retries(0).account().await.unwrap_err();
    assert_eq!(error.code(), "busy");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn retries_a_503_that_is_not_the_apis_json() {
    let server = MockServer::start(|_, n| {
        if n == 0 {
            Reply {
                status: 503,
                headers: vec![],
                body: b"upstream unavailable".to_vec(),
                delay: Duration::ZERO,
            }
        } else {
            Reply::json(200, r#"{"unlimited":true}"#)
        }
    })
    .await;
    assert!(server.client().account().await.unwrap().unlimited);
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn waits_as_retry_after_says_capped() {
    let delays = RetryDelays {
        first: Duration::from_millis(1),
        retry_after_unit: Duration::from_millis(40),
        cap: Duration::from_millis(60),
    };
    // 1 unit = 40 ms; 100 units is capped at 60 ms.
    assert_eq!(delays.wait(0, Some(1)), Duration::from_millis(40));
    assert_eq!(delays.wait(0, Some(100)), Duration::from_millis(60));
    assert_eq!(delays.wait(0, None), Duration::from_millis(1));
    assert_eq!(delays.wait(1, None), Duration::from_millis(2));
    let real = RetryDelays::default();
    assert_eq!(real.wait(0, None), Duration::from_secs(1));
    assert_eq!(real.wait(1, None), Duration::from_secs(2));
    assert_eq!(real.wait(0, Some(5)), Duration::from_secs(5));
    assert_eq!(real.wait(0, Some(600)), Duration::from_secs(30));

    let server = MockServer::start(|_, n| {
        if n == 0 {
            Reply::busy(Some(1))
        } else {
            Reply::json(200, "{}")
        }
    })
    .await;
    let started = Instant::now();
    server.client().with_delays(delays).account().await.unwrap();
    assert!(started.elapsed() >= Duration::from_millis(40));
}

#[tokio::test]
async fn retries_connection_failures_and_reports_them() {
    let error = client_for(&dead_url().await).account().await.unwrap_err();
    assert_eq!(error.code(), "connection");
    assert!(matches!(error, ApiError::Connection(_)));
}

#[tokio::test]
async fn a_timeout_is_its_own_error_and_is_not_retried() {
    let server =
        MockServer::start(|_, _| Reply::json(200, "{}").after(Duration::from_millis(400))).await;
    let http = super::client::http_client(Duration::from_millis(60)).unwrap();
    let client = ApiClient::new(http, &server.url(), Some("k".into()))
        .unwrap()
        .with_delays(tiny_delays());
    let error = client.account().await.unwrap_err();
    assert_eq!(error, ApiError::Timeout);
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn stopping_ends_the_wait_between_retries() {
    static HITS: AtomicUsize = AtomicUsize::new(0);
    let server = MockServer::start(|_, _| {
        HITS.fetch_add(1, Ordering::SeqCst);
        Reply::busy(Some(30))
    })
    .await;
    let slow = RetryDelays {
        first: Duration::from_secs(30),
        retry_after_unit: Duration::from_secs(1),
        cap: Duration::from_secs(30),
    };
    let client = server.client().with_delays(slow);
    let stop = CancellationToken::new();
    let dir = tempfile::tempdir().unwrap();
    let image = write_fake(dir.path(), "a.png");
    let stopper = stop.clone();
    tokio::spawn(async move {
        while HITS.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        stopper.cancel();
    });
    let started = Instant::now();
    let error = client
        .process("/v1/upscale", &[], &image, &[], Some(&stop))
        .await
        .unwrap_err();
    assert_eq!(error, ApiError::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn needs_a_key_except_for_health() {
    let server = MockServer::start(|r, _| match r.path.as_str() {
        "/api/health" => Reply::json(503, r#"{"status":"loading"}"#),
        _ => Reply::json(200, "{}"),
    })
    .await;
    let http = super::client::http_client(Duration::from_secs(5)).unwrap();
    let client = ApiClient::new(http, &format!("{}/", server.url()), None).unwrap();
    assert!(!client.has_key());
    assert_eq!(client.account().await.unwrap_err(), ApiError::NoKey);
    // A 503 from /health is a status, not an error; no key is sent.
    assert_eq!(client.health().await.unwrap(), "loading");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/api/health");
    assert_eq!(requests[0].header("authorization"), None);
}

#[tokio::test]
async fn a_missing_file_is_an_io_error() {
    let server = MockServer::start(|_, _| Reply::png()).await;
    let error = server
        .client()
        .process(
            "/v1/resize",
            &[],
            std::path::Path::new("/nonexistent/x.png"),
            &[],
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "io");
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn a_json_answer_where_an_image_is_due_is_an_error() {
    let server = MockServer::start(|_, _| Reply::json(200, "{}")).await;
    let dir = tempfile::tempdir().unwrap();
    let image = write_fake(dir.path(), "a.png");
    let error = server
        .client()
        .process("/v1/resize", &[], &image, &[], None)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "invalid");
}

#[test]
fn base_urls_are_checked_and_tidied() {
    assert_eq!(
        normalize_base_url(" https://api.photos.sh/ ").unwrap(),
        "https://api.photos.sh"
    );
    assert_eq!(
        normalize_base_url("http://127.0.0.1:8098/api").unwrap(),
        "http://127.0.0.1:8098/api"
    );
    assert!(normalize_base_url("photos.sh/api").is_err());
    assert!(normalize_base_url("ftp://photos.sh").is_err());
    assert!(normalize_base_url("https://api.photos.sh?x=1").is_err());
    assert!(normalize_base_url("").is_err());
}
