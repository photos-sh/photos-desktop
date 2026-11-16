//! The HTTP client for the Photos.sh API.
//!
//! It follows the rules of the Photos.sh client libraries: images go as
//! `multipart/form-data` with an `image` part streamed from disk, options go
//! in the query string, and only a `503 busy` (or a 503 that is not the API's
//! JSON) and a failure to connect are tried again.

use std::path::Path;
use std::time::Duration;

use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::multipart::{Form, Part};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use tokio_util::sync::CancellationToken;

use super::error::{ApiError, code};
use super::types::{Account, ImageResult, Marketplace, Preset, Readings};

pub const DEFAULT_BASE_URL: &str = "https://api.photos.sh";
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
pub const DEFAULT_RETRIES: u32 = 2;
pub const USER_AGENT: &str = concat!("photos-sh-desktop/", env!("CARGO_PKG_VERSION"));

/// How long to wait before trying again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryDelays {
    /// The wait before the first retry without `Retry-After`; it doubles.
    pub first: Duration,
    /// What one `Retry-After` unit lasts: a second (tests shrink it).
    pub retry_after_unit: Duration,
    /// The longest wait a `Retry-After` can ask for.
    pub cap: Duration,
}

impl Default for RetryDelays {
    fn default() -> Self {
        Self {
            first: Duration::from_secs(1),
            retry_after_unit: Duration::from_secs(1),
            cap: Duration::from_secs(30),
        }
    }
}

impl RetryDelays {
    /// The wait before retry number `retry` (0 for the first).
    pub fn wait(&self, retry: u32, retry_after: Option<u64>) -> Duration {
        match retry_after {
            Some(units) => self
                .retry_after_unit
                .saturating_mul(u32::try_from(units).unwrap_or(u32::MAX))
                .min(self.cap),
            None => self.first.saturating_mul(2u32.saturating_pow(retry)),
        }
    }
}

/// A part sent beside the image (`mask` for erase, `recipe` for run).
#[derive(Debug, Clone)]
pub struct ExtraPart {
    pub name: &'static str,
    pub file_name: &'static str,
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    retries: u32,
    delays: RetryDelays,
}

/// The shared HTTP client: one connection pool for the app.
pub fn http_client(timeout: Duration) -> Result<reqwest::Client, ApiError> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(15).min(timeout))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| ApiError::Invalid(format!("could not set up the HTTP client: {e}")))
}

/// Checks a base URL and drops trailing slashes.
pub fn normalize_base_url(raw: &str) -> Result<String, ApiError> {
    let trimmed = raw.trim().trim_end_matches('/');
    let url = reqwest::Url::parse(trimmed)
        .map_err(|_| ApiError::Invalid(format!("\"{raw}\" is not a URL")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(ApiError::Invalid(
            "the base URL must start with http:// or https://".to_string(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ApiError::Invalid(
            "the base URL cannot have a query or a fragment".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

impl ApiClient {
    pub fn new(
        http: reqwest::Client,
        base_url: &str,
        api_key: Option<String>,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            http,
            base_url: normalize_base_url(base_url)?,
            api_key: api_key.filter(|k| !k.is_empty()),
            retries: DEFAULT_RETRIES,
            delays: RetryDelays::default(),
        })
    }

    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    pub fn with_delays(mut self, delays: RetryDelays) -> Self {
        self.delays = delays;
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn has_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// The account behind the key: credits left and prices, or `unlimited`.
    pub async fn account(&self) -> Result<Account, ApiError> {
        self.get_json("/v1/account").await
    }

    /// The built-in presets `run` takes.
    pub async fn presets(&self) -> Result<Vec<Preset>, ApiError> {
        self.get_json("/v1/presets").await
    }

    /// The marketplaces `fix` and `check` know.
    pub async fn marketplaces(&self) -> Result<Vec<Marketplace>, ApiError> {
        self.get_json("/v1/marketplaces").await
    }

    /// Whether the service is ready: `ok`, or `loading`/`failed` (which the
    /// API answers with a 503; that is not an error here). Needs no key.
    pub async fn health(&self) -> Result<String, ApiError> {
        #[derive(serde::Deserialize)]
        struct Health {
            status: String,
        }
        let request = self
            .http
            .get(format!("{}/health", self.base_url))
            .build()
            .map_err(|e| ApiError::Invalid(e.to_string()))?;
        let response = self.http.execute(request).await.map_err(transport_error)?;
        let status = response.status();
        let retry_after = retry_after(response.headers());
        let body = response.bytes().await.map_err(transport_error)?;
        if let Ok(health) = serde_json::from_slice::<Health>(&body) {
            return Ok(health.status);
        }
        if status.is_success() {
            return Err(ApiError::Invalid(
                "unexpected response from /health".to_string(),
            ));
        }
        Err(ApiError::from_response(status.as_u16(), retry_after, &body))
    }

    /// Sends an image to an endpoint that answers with an image (for
    /// `vectorize`, an SVG one).
    ///
    /// `stop` ends the waiting between retries early (with `Cancelled`); a
    /// request already on its way is not interrupted.
    pub async fn process(
        &self,
        endpoint: &str,
        query: &[(String, String)],
        image: &Path,
        extras: &[ExtraPart],
        stop: Option<&CancellationToken>,
    ) -> Result<ImageResult, ApiError> {
        let response = self
            .post_image(endpoint, query, image, extras, stop)
            .await?;
        let headers = response.headers().clone();
        let content_type = header_str(&headers, "content-type")
            .unwrap_or("application/octet-stream")
            .to_string();
        if !content_type.starts_with("image/") {
            return Err(ApiError::Invalid(format!(
                "expected an image from {endpoint}, got {content_type}"
            )));
        }
        let bytes = response.bytes().await.map_err(transport_error)?.to_vec();
        Ok(ImageResult {
            endpoint: endpoint.to_string(),
            bytes,
            content_type,
            width: header_number(&headers, "x-image-width"),
            height: header_number(&headers, "x-image-height"),
            quality: header_number(&headers, "x-image-quality"),
            server_timing: header_str(&headers, "server-timing").map(str::to_string),
            text_removed: header_number(&headers, "x-text-removed"),
            text_kept: header_number(&headers, "x-text-kept"),
            marketplace_passed: header_bool(&headers, "x-marketplace-passed"),
            marketplace_findings: header_str(&headers, "x-marketplace-findings").map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            }),
            faces: header_number(&headers, "x-faces"),
            faces_skipped: header_number(&headers, "x-faces-skipped"),
            subject_found: header_bool(&headers, "x-subject-found"),
            paths: header_number(&headers, "x-paths"),
            readings: Readings::from_headers(
                header_str(&headers, "x-barcodes"),
                header_str(&headers, "x-text"),
                header_bool(&headers, "x-text-truncated"),
            ),
        })
    }

    /// Sends an image to an endpoint that answers with JSON (`check`).
    pub async fn inspect<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        query: &[(String, String)],
        image: &Path,
        stop: Option<&CancellationToken>,
    ) -> Result<T, ApiError> {
        let response = self.post_image(endpoint, query, image, &[], stop).await?;
        let body = response.bytes().await.map_err(transport_error)?;
        serde_json::from_slice(&body)
            .map_err(|e| ApiError::Invalid(format!("unexpected response from {endpoint}: {e}")))
    }

    async fn get_json<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T, ApiError> {
        let key = self.api_key.as_deref().ok_or(ApiError::NoKey)?;
        let url = format!("{}{endpoint}", self.base_url);
        let response = self
            .send(None, || async {
                Ok(self.http.request(Method::GET, &url).bearer_auth(key))
            })
            .await?;
        let body = response.bytes().await.map_err(transport_error)?;
        serde_json::from_slice(&body)
            .map_err(|e| ApiError::Invalid(format!("unexpected response from {endpoint}: {e}")))
    }

    async fn post_image(
        &self,
        endpoint: &str,
        query: &[(String, String)],
        image: &Path,
        extras: &[ExtraPart],
        stop: Option<&CancellationToken>,
    ) -> Result<reqwest::Response, ApiError> {
        let key = self.api_key.as_deref().ok_or(ApiError::NoKey)?;
        let url = format!("{}{endpoint}", self.base_url);
        self.send(stop, || async {
            // Built again for every attempt: the file is streamed from disk.
            let part = Part::file(image)
                .await
                .map_err(|e| ApiError::Io(format!("could not read {}: {e}", image.display())))?;
            let mut form = Form::new().part("image", part);
            for extra in extras {
                let part = Part::bytes(extra.bytes.clone())
                    .file_name(extra.file_name)
                    .mime_str(extra.mime)
                    .map_err(|e| ApiError::Invalid(e.to_string()))?;
                form = form.part(extra.name, part);
            }
            Ok(self
                .http
                .request(Method::POST, &url)
                .bearer_auth(key)
                .query(query)
                .multipart(form))
        })
        .await
    }

    /// Sends a request, trying again on `503 busy` and connection failures.
    async fn send<F, Fut>(
        &self,
        stop: Option<&CancellationToken>,
        build: F,
    ) -> Result<reqwest::Response, ApiError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<reqwest::RequestBuilder, ApiError>>,
    {
        let mut retry = 0;
        loop {
            if stop.is_some_and(CancellationToken::is_cancelled) {
                return Err(ApiError::Cancelled);
            }
            let error = match build().await?.send().await {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let retry_after = retry_after(response.headers());
                    let body = response.bytes().await.map_err(transport_error)?;
                    let error = ApiError::from_response(status.as_u16(), retry_after, &body);
                    // A 503 is worth another try unless the API says the tool
                    // is not there at all.
                    let again = status == StatusCode::SERVICE_UNAVAILABLE
                        && matches!(error.code(), code::BUSY | code::UNKNOWN);
                    if !again {
                        return Err(error);
                    }
                    error
                }
                // No connection was made, so nothing was processed or charged.
                Err(e) if e.is_connect() => transport_error(e),
                Err(e) => return Err(transport_error(e)),
            };
            if retry >= self.retries {
                return Err(error);
            }
            let wait = self.delays.wait(retry, error.retry_after());
            retry += 1;
            match stop {
                Some(stop) => tokio::select! {
                    () = tokio::time::sleep(wait) => {}
                    () = stop.cancelled() => return Err(ApiError::Cancelled),
                },
                None => tokio::time::sleep(wait).await,
            }
        }
    }
}

fn transport_error(e: reqwest::Error) -> ApiError {
    if e.is_timeout() {
        return ApiError::Timeout;
    }
    // The innermost cause says what happened ("Connection refused").
    let mut source: &dyn std::error::Error = &e;
    while let Some(next) = source.source() {
        source = next;
    }
    ApiError::Connection(source.to_string())
}

fn retry_after(headers: &HeaderMap) -> Option<u64> {
    headers.get(RETRY_AFTER)?.to_str().ok()?.trim().parse().ok()
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

fn header_number(headers: &HeaderMap, name: &str) -> Option<u32> {
    header_str(headers, name)?.trim().parse().ok()
}

fn header_bool(headers: &HeaderMap, name: &str) -> Option<bool> {
    header_str(headers, name).map(|v| v.trim().eq_ignore_ascii_case("true"))
}
