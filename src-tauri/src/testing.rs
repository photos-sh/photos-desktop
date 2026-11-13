//! Test support: an in-process HTTP server that records what it is sent and
//! answers as a test tells it to, and small image files to send.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{FromRequest, Multipart, Request};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::Response;

use crate::api::{ApiClient, RetryDelays};

#[derive(Debug, Clone)]
pub struct RecordedPart {
    pub name: String,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: HeaderMap,
    pub parts: Vec<RecordedPart>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name)?.to_str().ok()
    }

    pub fn part(&self, name: &str) -> Option<&RecordedPart> {
        self.parts.iter().find(|p| p.name == name)
    }

    /// The file name of the `image` part.
    pub fn image_name(&self) -> String {
        self.part("image")
            .and_then(|p| p.file_name.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub delay: Duration,
}

impl Reply {
    pub fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("content-type".into(), "application/json".into())],
            body: body.as_bytes().to_vec(),
            delay: Duration::ZERO,
        }
    }

    pub fn error(status: u16, code: &str, message: &str) -> Self {
        Self::json(
            status,
            &serde_json::json!({"error": {"code": code, "message": message}}).to_string(),
        )
    }

    pub fn busy(retry_after: Option<u64>) -> Self {
        let reply = Self::error(503, "busy", "All workers are busy");
        match retry_after {
            Some(seconds) => reply.header("retry-after", &seconds.to_string()),
            None => reply,
        }
    }

    pub fn image(content_type: &str, body: &[u8]) -> Self {
        Self {
            status: 200,
            headers: vec![
                ("content-type".into(), content_type.into()),
                ("x-image-width".into(), "8".into()),
                ("x-image-height".into(), "6".into()),
            ],
            body: body.to_vec(),
            delay: Duration::ZERO,
        }
    }

    pub fn png() -> Self {
        Self::image("image/png", b"PNG-RESULT")
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn after(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

type Handler = dyn Fn(&Recorded, usize) -> Reply + Send + Sync;

pub struct MockServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<Recorded>>>,
    answered: Arc<std::sync::atomic::AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl MockServer {
    /// Starts a server; `handler` gets each request and its number (from 0).
    pub async fn start(
        handler: impl Fn(&Recorded, usize) -> Reply + Send + Sync + 'static,
    ) -> Self {
        let handler: Arc<Handler> = Arc::new(handler);
        let requests: Arc<Mutex<Vec<Recorded>>> = Arc::default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let recorded = requests.clone();
        let answered: Arc<std::sync::atomic::AtomicUsize> = Arc::default();
        let counted = answered.clone();
        let app = axum::Router::new().fallback(move |request: Request| {
            let handler = handler.clone();
            let recorded = recorded.clone();
            let counted = counted.clone();
            async move {
                let record = record(request).await;
                let number = {
                    let mut all = recorded.lock().unwrap();
                    all.push(record.clone());
                    all.len() - 1
                };
                let reply = handler(&record, number);
                if !reply.delay.is_zero() {
                    tokio::time::sleep(reply.delay).await;
                }
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut response = Response::new(Body::from(reply.body));
                *response.status_mut() = StatusCode::from_u16(reply.status).unwrap();
                for (name, value) in reply.headers {
                    response.headers_mut().insert(
                        HeaderName::from_bytes(name.as_bytes()).unwrap(),
                        HeaderValue::from_str(&value).unwrap(),
                    );
                }
                response
            }
        });
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            addr,
            requests,
            answered,
            task,
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}/api", self.addr)
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    /// How many requests the server got to answer. One whose client went
    /// away while it was being worked on is not among them: the server
    /// drops it, as the engine's (the same axum) does.
    pub fn answered(&self) -> usize {
        self.answered.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// A client for this server with a key and waits of a few milliseconds.
    pub fn client(&self) -> ApiClient {
        client_for(&self.url())
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub const TEST_KEY: &str = "test-key-1234";

pub fn tiny_delays() -> RetryDelays {
    RetryDelays {
        first: Duration::from_millis(2),
        retry_after_unit: Duration::from_millis(1),
        cap: Duration::from_millis(30),
    }
}

pub fn client_for(base_url: &str) -> ApiClient {
    let http = crate::api::client::http_client(Duration::from_secs(5)).unwrap();
    ApiClient::new(http, base_url, Some(TEST_KEY.to_string()))
        .unwrap()
        .with_delays(tiny_delays())
}

/// A base URL nothing listens on.
pub async fn dead_url() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}/api")
}

async fn record(request: Request) -> Recorded {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let query = reqwest::Url::parse(&format!("http://mock{}", request.uri()))
        .map(|u| u.query_pairs().into_owned().collect())
        .unwrap_or_default();
    let headers = request.headers().clone();
    let mut parts = Vec::new();
    let is_multipart = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/form-data"));
    if is_multipart {
        let mut multipart = Multipart::from_request(request, &()).await.unwrap();
        while let Some(field) = multipart.next_field().await.unwrap() {
            parts.push(RecordedPart {
                name: field.name().unwrap_or_default().to_string(),
                file_name: field.file_name().map(str::to_string),
                content_type: field.content_type().map(str::to_string),
                bytes: field.bytes().await.unwrap().to_vec(),
            });
        }
    }
    Recorded {
        method,
        path,
        query,
        headers,
        parts,
    }
}

/// Writes a small real PNG and returns its path.
pub fn write_png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let path = dir.join(name);
    let image = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x * 40 % 256) as u8, (y * 40 % 256) as u8, 120])
    });
    image
        .save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    path
}

/// Writes a file that only has an image's name (the mock server does not
/// decode what it is sent).
pub fn write_fake(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, format!("fake image {name}")).unwrap();
    path
}
