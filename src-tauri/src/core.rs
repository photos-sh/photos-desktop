//! The app without its window: settings, the key, the batch and the editor
//! behind one type. The Tauri commands in `lib.rs` are thin wrappers around
//! [`Core`], and the integration tests drive it directly.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::api::client::http_client;
use crate::api::{Account, ApiClient, ApiError, DEFAULT_TIMEOUT, ExtraPart, Marketplace, Preset};
use crate::batch::{
    self, BatchError, BatchEvents, BatchSnapshot, BatchStore, BatchSummary, Estimate, Job,
    Selection,
};
use crate::editor::{EditorError, EditorSession, EditorSnapshot};
use crate::keychain::{KeyError, KeyStatus, KeyVault, SecretStore};
use crate::mask::{MaskError, Stroke, mask_png};
use crate::ops::{Format, InvalidOperation, Operation, Pick, check_query, select_query};
use crate::preview::{self, Picture, PreviewError};
use crate::settings::{Settings, SettingsError};

/// An error as the UI gets it: a code to choose the wording by, and the
/// message of whoever raised it as a fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub code: String,
    pub message: String,
    /// The HTTP status, for errors the API answered with.
    pub status: Option<u16>,
    /// Seconds the API asked to wait, when it did.
    pub retry_after: Option<u64>,
}

impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            status: None,
            retry_after: None,
        }
    }
}

impl From<ApiError> for AppError {
    fn from(e: ApiError) -> Self {
        Self {
            code: e.code().to_string(),
            message: e.to_string(),
            status: e.status(),
            retry_after: e.retry_after(),
        }
    }
}

impl From<BatchError> for AppError {
    fn from(e: BatchError) -> Self {
        match e {
            BatchError::Api(api) => api.into(),
            other => Self::new(other.code(), other.to_string()),
        }
    }
}

impl From<EditorError> for AppError {
    fn from(e: EditorError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

impl From<SettingsError> for AppError {
    fn from(e: SettingsError) -> Self {
        let code = match e {
            SettingsError::Invalid(_) => "invalid",
            SettingsError::Save(_) => "io",
        };
        Self::new(code, e.to_string())
    }
}

impl From<KeyError> for AppError {
    fn from(e: KeyError) -> Self {
        let code = match e {
            KeyError::Empty | KeyError::Malformed => "invalid",
            KeyError::Keychain(_) => "keychain",
        };
        Self::new(code, e.to_string())
    }
}

impl From<MaskError> for AppError {
    fn from(e: MaskError) -> Self {
        Self::new("invalid", e.to_string())
    }
}

impl From<InvalidOperation> for AppError {
    fn from(e: InvalidOperation) -> Self {
        Self::new("invalid", e.to_string())
    }
}

/// The lists the operation forms offer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Catalog {
    pub presets: Vec<Preset>,
    pub marketplaces: Vec<Marketplace>,
}

/// A tool of the editor.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorTool {
    /// One of the operations a batch can run, too.
    Operation { operation: Operation },
    /// Paint out what the brush covered.
    Erase {
        strokes: Vec<Stroke>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    /// Cut out the object at the points clicked, in the photo's pixels.
    Select {
        points: Vec<[f32; 2]>,
        #[serde(default)]
        pick: Option<Pick>,
    },
}

/// A batch that passed its checks and can start.
pub struct PreparedBatch {
    pub client: ApiClient,
    pub job: Job,
    pub selection: Selection,
    pub parallel: usize,
    pub estimate: Estimate,
}

pub struct Core {
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    vault: KeyVault,
    http: reqwest::Client,
    batch: BatchStore,
    editor: Mutex<Option<EditorSession>>,
    /// Stops the editor's request in flight.
    editor_cancel: Mutex<Option<CancellationToken>>,
    sessions: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Core {
    /// `settings_path` is the settings file; `secrets` is where the key is kept.
    pub fn new(settings_path: PathBuf, secrets: Box<dyn SecretStore>) -> Result<Self, AppError> {
        Ok(Self {
            settings: Mutex::new(Settings::load(&settings_path)),
            settings_path,
            vault: KeyVault::new(secrets),
            http: http_client(DEFAULT_TIMEOUT)?,
            batch: BatchStore::default(),
            editor: Mutex::new(None),
            editor_cancel: Mutex::new(None),
            sessions: AtomicU64::new(0),
        })
    }

    // --- settings and key ---------------------------------------------------

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    pub fn set_settings(&self, settings: Settings) -> Result<Settings, AppError> {
        let settings = settings.validated()?;
        settings.save(&self.settings_path)?;
        *lock(&self.settings) = settings.clone();
        Ok(settings)
    }

    /// Reads the keychain the first time: call it off the UI thread.
    pub fn key_status(&self) -> KeyStatus {
        self.vault.status()
    }

    pub fn set_key(&self, key: &str) -> Result<KeyStatus, AppError> {
        Ok(self.vault.set(key)?)
    }

    pub fn remove_key(&self) -> Result<KeyStatus, AppError> {
        Ok(self.vault.remove()?)
    }

    /// A client for the configured base URL with the stored key.
    pub fn client(&self) -> Result<ApiClient, AppError> {
        let base_url = lock(&self.settings).base_url.clone();
        let key = self.vault.key();
        if key.is_none() {
            return Err(ApiError::NoKey.into());
        }
        Ok(ApiClient::new(self.http.clone(), &base_url, key)?)
    }

    pub async fn account(&self) -> Result<Account, AppError> {
        Ok(self.client()?.account().await?)
    }

    pub async fn catalog(&self) -> Result<Catalog, AppError> {
        let client = self.client()?;
        let (presets, marketplaces) = tokio::try_join!(client.presets(), client.marketplaces())?;
        Ok(Catalog {
            presets,
            marketplaces,
        })
    }

    // --- batch ----------------------------------------------------------------

    pub fn batch(&self) -> &BatchStore {
        &self.batch
    }

    pub fn batch_snapshot(&self) -> BatchSnapshot {
        self.batch.snapshot()
    }

    pub fn batch_add(&self, paths: &[PathBuf]) -> Result<BatchSnapshot, AppError> {
        self.batch.add_paths(paths)?;
        Ok(self.batch.snapshot())
    }

    /// What running `operation` on the files not yet done would cost.
    pub async fn batch_estimate(&self, operation: &Operation) -> Result<Estimate, AppError> {
        let files = self.batch.count(Selection::Unfinished);
        let account = self.client()?.account().await?;
        Ok(batch::plan::estimate(
            &account,
            operation.price_key(),
            files,
        ))
    }

    /// Checks everything a run needs: the options, the key, the balance and
    /// the output folder. A balance that is too low refuses the run.
    pub async fn batch_prepare(
        &self,
        operation: Operation,
        output_dir: Option<PathBuf>,
    ) -> Result<PreparedBatch, AppError> {
        operation.validate()?;
        let output_dir = match output_dir.filter(|d| !d.as_os_str().is_empty()) {
            Some(dir) => dir,
            None => self.batch.default_output_dir().ok_or(BatchError::Empty)?,
        };
        self.prepare(
            Job {
                operation,
                output_dir,
            },
            Selection::Unfinished,
        )
        .await
    }

    /// The same checks for running the last operation again on what failed.
    pub async fn batch_prepare_retry(&self) -> Result<PreparedBatch, AppError> {
        let job = self.batch.last_job().ok_or(BatchError::NoEarlierRun)?;
        self.prepare(job, Selection::Failed).await
    }

    async fn prepare(&self, job: Job, selection: Selection) -> Result<PreparedBatch, AppError> {
        if self.batch.is_running() {
            return Err(BatchError::Running.into());
        }
        let files = self.batch.count(selection);
        if files == 0 {
            return Err(BatchError::Empty.into());
        }
        let client = self.client()?;
        let estimate = batch::preflight(&client, &job.operation, files).await?;
        tokio::fs::create_dir_all(&job.output_dir)
            .await
            .map_err(|e| BatchError::OutputDir(format!("{}: {e}", job.output_dir.display())))?;
        Ok(PreparedBatch {
            client,
            job,
            selection,
            parallel: lock(&self.settings).parallel,
            estimate,
        })
    }

    /// Runs a prepared batch to its end.
    pub async fn batch_run(
        &self,
        prepared: PreparedBatch,
        events: &dyn BatchEvents,
    ) -> Result<BatchSummary, AppError> {
        Ok(batch::run(
            &self.batch,
            &prepared.client,
            prepared.job,
            prepared.selection,
            prepared.parallel,
            events,
        )
        .await?)
    }

    // --- editor ---------------------------------------------------------------

    fn with_editor<T>(
        &self,
        f: impl FnOnce(&mut EditorSession) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        match lock(&self.editor).as_mut() {
            Some(editor) => f(editor),
            None => Err(EditorError::NothingOpen.into()),
        }
    }

    /// Opens a photo, closing the one that was open (its steps are removed).
    pub fn editor_open(&self, path: &Path) -> Result<EditorSnapshot, AppError> {
        let session = self.sessions.fetch_add(1, Ordering::Relaxed) + 1;
        let editor = EditorSession::open(session, path)?;
        let snapshot = editor.snapshot();
        self.editor_cancel();
        *lock(&self.editor) = Some(editor);
        Ok(snapshot)
    }

    /// Closes the photo and removes its steps.
    pub fn editor_close(&self) {
        self.editor_cancel();
        *lock(&self.editor) = None;
    }

    pub fn editor_snapshot(&self) -> Option<EditorSnapshot> {
        lock(&self.editor).as_ref().map(EditorSession::snapshot)
    }

    pub fn editor_undo(&self) -> Result<EditorSnapshot, AppError> {
        self.with_editor(|e| {
            e.undo();
            Ok(e.snapshot())
        })
    }

    pub fn editor_redo(&self) -> Result<EditorSnapshot, AppError> {
        self.with_editor(|e| {
            e.redo();
            Ok(e.snapshot())
        })
    }

    pub fn editor_go_to(&self, cursor: usize) -> Result<EditorSnapshot, AppError> {
        self.with_editor(|e| {
            e.go_to(cursor);
            Ok(e.snapshot())
        })
    }

    pub fn editor_save(
        &self,
        destination: &Path,
        replace_original: bool,
    ) -> Result<PathBuf, AppError> {
        self.with_editor(|e| Ok(e.save_as(destination, replace_original)?))
    }

    /// Stops the editor's request in flight, if any.
    pub fn editor_cancel(&self) {
        if let Some(token) = lock(&self.editor_cancel).take() {
            token.cancel();
        }
    }

    /// The state a request starts from: checked again when it comes back.
    fn editor_start(&self) -> Result<(u64, usize, PathBuf, CancellationToken), AppError> {
        let (session, cursor, path) = self.with_editor(|e| {
            Ok((
                e.session(),
                e.snapshot().cursor,
                e.current_path().to_path_buf(),
            ))
        })?;
        let token = CancellationToken::new();
        if let Some(earlier) = lock(&self.editor_cancel).replace(token.clone()) {
            earlier.cancel();
        }
        Ok((session, cursor, path, token))
    }

    /// Applies a tool to the current picture; its result becomes a step.
    pub async fn editor_apply(&self, tool: EditorTool) -> Result<EditorSnapshot, AppError> {
        let (label, endpoint, query, extras) = match &tool {
            EditorTool::Operation { operation } => {
                operation.validate()?;
                // A step is a picture the next tool can work on.
                if !operation.gives_pixels() {
                    return Err(AppError::new(
                        "invalid",
                        "This tool gives an SVG file, which the editor cannot work on. Use it in Batch.",
                    ));
                }
                (
                    operation.label(),
                    operation.endpoint(),
                    operation.query(),
                    operation.parts(),
                )
            }
            EditorTool::Erase {
                strokes,
                format,
                quality,
            } => {
                let (width, height) = self
                    .with_editor(|e| Ok(e.current_size()))?
                    .ok_or(MaskError::UnknownSize)?;
                let strokes = strokes.clone();
                let mask = tokio::task::spawn_blocking(move || mask_png(width, height, &strokes))
                    .await
                    .map_err(|e| AppError::new("internal", e.to_string()))??;
                let mut query = Vec::new();
                if let Some(format) = format {
                    query.push(("format".to_string(), format.as_str().to_string()));
                }
                if let Some(quality) = quality {
                    query.push(("quality".to_string(), quality.to_string()));
                }
                let mask = ExtraPart {
                    name: "mask",
                    file_name: "mask.png",
                    mime: "image/png",
                    bytes: mask,
                };
                ("Erase".to_string(), "/v1/erase", query, vec![mask])
            }
            EditorTool::Select { points, pick } => {
                // The points are in the current picture's pixels; when its
                // size is not known here, the API judges them.
                let size = self.with_editor(|e| Ok(e.current_size()))?;
                let query = select_query(points, *pick, size)?;
                ("Select object".to_string(), "/v1/select", query, Vec::new())
            }
        };
        let client = self.client()?;
        let (session, cursor, path, token) = self.editor_start()?;
        let result = tokio::select! {
            result = client.process(endpoint, &query, &path, &extras, Some(&token)) => result?,
            () = token.cancelled() => return Err(ApiError::Cancelled.into()),
        };
        self.with_editor(|e| {
            // The photo was closed or changed while the request was out.
            if e.session() != session || e.snapshot().cursor != cursor {
                return Err(ApiError::Cancelled.into());
            }
            e.push(&label, &result)?;
            Ok(e.snapshot())
        })
    }

    /// Checks the current picture's quality and, with a marketplace, its rules.
    pub async fn editor_check(
        &self,
        marketplace: Option<&str>,
        subject: bool,
        text: bool,
    ) -> Result<serde_json::Value, AppError> {
        let client = self.client()?;
        let (_, _, path, token) = self.editor_start()?;
        let query = check_query(marketplace, subject, text);
        tokio::select! {
            report = client.inspect("/v1/check", &query, &path, Some(&token)) => Ok(report?),
            () = token.cancelled() => Err(ApiError::Cancelled.into()),
        }
    }

    // --- previews -------------------------------------------------------------

    /// The picture behind a preview key:
    /// `batch/<id>/source`, `batch/<id>/result` (thumbnails) or
    /// `editor/<session>/<step>` (the whole picture). Only files the user
    /// put in the batch or opened in the editor can be reached this way.
    pub fn preview(&self, key: &str) -> Result<Picture, PreviewError> {
        let parts: Vec<&str> = key.trim_matches('/').split('/').collect();
        let number = |s: &str| s.parse::<u64>().map_err(|_| PreviewError::Unsupported);
        match parts.as_slice() {
            ["batch", id, which] => {
                let id = number(id)?;
                let path = match *which {
                    "source" => self.batch.source_path(id),
                    "result" => self.batch.output_path(id),
                    _ => None,
                };
                preview::thumbnail(&path.ok_or(PreviewError::Unsupported)?)
            }
            ["editor", session, step] => {
                let (session, step) = (number(session)?, number(step)?);
                let path = lock(&self.editor)
                    .as_ref()
                    .filter(|e| e.session() == session)
                    .and_then(|e| e.path_of(step).map(Path::to_path_buf));
                preview::full(&path.ok_or(PreviewError::Unsupported)?)
            }
            _ => Err(PreviewError::Unsupported),
        }
    }
}
