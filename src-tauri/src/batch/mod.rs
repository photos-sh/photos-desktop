//! The batch processor: a list of files, one operation, a few files at once.
//!
//! [`BatchStore`] holds the list and each file's status; [`run`] sends the
//! files through the API and writes the results. Neither knows about the
//! window: progress leaves through [`BatchEvents`].

pub mod plan;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures_util::StreamExt;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::api::{ApiClient, ApiError};
use crate::ops::Operation;
pub use plan::Estimate;

pub const MIN_PARALLEL: usize = 1;
pub const MAX_PARALLEL: usize = 8;
pub const DEFAULT_PARALLEL: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Queued,
    Running,
    Done,
    Failed,
    /// Not run: the batch was cancelled or stopped before its turn.
    Skipped,
    /// Given up while it was being processed, because the batch was
    /// cancelled: its request was dropped and nothing was saved.
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemError {
    pub code: String,
    pub message: String,
}

impl From<&ApiError> for ItemError {
    fn from(e: &ApiError) -> Self {
        Self {
            code: e.code().to_string(),
            message: e.to_string(),
        }
    }
}

/// One file of the batch, as the UI sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BatchItem {
    pub id: u64,
    /// The source's path, for display.
    pub source: String,
    pub name: String,
    pub status: ItemStatus,
    /// The result's path, once done.
    pub output: Option<String>,
    pub output_name: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bytes: Option<u64>,
    /// What the operation reported (text lines removed, a marketplace
    /// verdict, faces hidden or restored, a crop without a subject, an SVG's
    /// paths).
    pub note: Option<String>,
    pub error: Option<ItemError>,
    /// Grows with every change, so a preview URL can tell results apart.
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BatchSnapshot {
    pub items: Vec<BatchItem>,
    pub running: bool,
    /// Where results go unless another folder is chosen.
    pub default_output_dir: Option<String>,
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BatchSummary {
    /// Files in this run.
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
    /// Files given up while they were being processed, because the batch
    /// was cancelled. The server may have finished (and charged) some.
    pub aborted: usize,
    /// The user cancelled it.
    pub cancelled: bool,
    /// The error that stopped it (no credits left, a refused key).
    pub stopped: Option<ItemError>,
    pub output_dir: String,
}

/// The operation and output folder of a run; kept for "retry failed".
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub operation: Operation,
    pub output_dir: PathBuf,
}

/// Which files a run takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// Everything that is not done.
    Unfinished,
    /// What failed, was cancelled or was not run in an earlier run.
    Failed,
}

impl Selection {
    fn takes(self, status: ItemStatus) -> bool {
        match self {
            Self::Unfinished => status != ItemStatus::Done,
            Self::Failed => matches!(
                status,
                ItemStatus::Failed | ItemStatus::Skipped | ItemStatus::Cancelled
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BatchError {
    #[error("A batch is running. Wait for it to finish, or cancel it.")]
    Running,
    #[error("There is nothing to process.")]
    Empty,
    #[error("There is no earlier run to retry.")]
    NoEarlierRun,
    #[error("{0}")]
    Invalid(String),
    #[error(
        "This batch costs {cost} credits and the account has {credits} left. Add credits in the Photos.sh console, or process fewer files."
    )]
    InsufficientCredits { cost: u64, credits: i64 },
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("Could not create the output folder: {0}")]
    OutputDir(String),
}

impl BatchError {
    pub fn code(&self) -> &str {
        match self {
            Self::Running => "batch_running",
            Self::Empty => "batch_empty",
            Self::NoEarlierRun => "batch_no_earlier_run",
            Self::Invalid(_) => "invalid",
            Self::InsufficientCredits { .. } => "insufficient_credits",
            Self::Api(e) => e.code(),
            Self::OutputDir(_) => "io",
        }
    }
}

/// Where progress goes.
pub trait BatchEvents: Send + Sync {
    fn item(&self, item: &BatchItem);
    fn finished(&self, summary: &BatchSummary);
}

struct Entry {
    item: BatchItem,
    source: PathBuf,
    output: Option<PathBuf>,
}

#[derive(Default)]
struct Inner {
    entries: Vec<Entry>,
    next_id: u64,
    cancel: Option<CancellationToken>,
    last_job: Option<Job>,
}

/// The batch's files and their state.
#[derive(Default)]
pub struct BatchStore {
    inner: Mutex<Inner>,
}

impl BatchStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn snapshot(&self) -> BatchSnapshot {
        let inner = self.lock();
        BatchSnapshot {
            items: inner.entries.iter().map(|e| e.item.clone()).collect(),
            running: inner.cancel.is_some(),
            default_output_dir: inner
                .entries
                .first()
                .map(|e| plan::default_output_dir(&e.source).display().to_string()),
        }
    }

    pub fn is_running(&self) -> bool {
        self.lock().cancel.is_some()
    }

    /// Adds the images among `paths` (folders are walked) that are not in
    /// the list yet. Returns how many were added.
    pub fn add_paths(&self, paths: &[PathBuf]) -> Result<usize, BatchError> {
        if self.is_running() {
            return Err(BatchError::Running);
        }
        // Walking folders can take a moment: not under the lock.
        let found = plan::collect_images(paths);
        let mut inner = self.lock();
        if inner.cancel.is_some() {
            return Err(BatchError::Running);
        }
        let mut added = 0;
        for source in found {
            if inner.entries.iter().any(|e| e.source == source) {
                continue;
            }
            inner.next_id += 1;
            let id = inner.next_id;
            inner.entries.push(Entry {
                item: BatchItem {
                    id,
                    source: source.display().to_string(),
                    name: source
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    status: ItemStatus::Queued,
                    output: None,
                    output_name: None,
                    width: None,
                    height: None,
                    bytes: None,
                    note: None,
                    error: None,
                    version: 0,
                },
                source,
                output: None,
            });
            added += 1;
        }
        Ok(added)
    }

    pub fn remove(&self, id: u64) -> Result<(), BatchError> {
        let mut inner = self.lock();
        if inner.cancel.is_some() {
            return Err(BatchError::Running);
        }
        inner.entries.retain(|e| e.item.id != id);
        Ok(())
    }

    /// Empties the list. Results already written stay on disk.
    pub fn clear(&self) -> Result<(), BatchError> {
        let mut inner = self.lock();
        if inner.cancel.is_some() {
            return Err(BatchError::Running);
        }
        inner.entries.clear();
        inner.last_job = None;
        Ok(())
    }

    /// Puts every file back in the queue, to run another operation on the
    /// same sources. Results already written stay on disk.
    pub fn reset(&self) -> Result<(), BatchError> {
        let mut inner = self.lock();
        if inner.cancel.is_some() {
            return Err(BatchError::Running);
        }
        for entry in &mut inner.entries {
            requeue(entry);
        }
        inner.last_job = None;
        Ok(())
    }

    pub fn source_path(&self, id: u64) -> Option<PathBuf> {
        let inner = self.lock();
        inner
            .entries
            .iter()
            .find(|e| e.item.id == id)
            .map(|e| e.source.clone())
    }

    pub fn output_path(&self, id: u64) -> Option<PathBuf> {
        let inner = self.lock();
        inner
            .entries
            .iter()
            .find(|e| e.item.id == id)
            .and_then(|e| e.output.clone())
    }

    pub fn last_job(&self) -> Option<Job> {
        self.lock().last_job.clone()
    }

    /// How many files a run with this selection would take.
    pub fn count(&self, selection: Selection) -> usize {
        let inner = self.lock();
        inner
            .entries
            .iter()
            .filter(|e| selection.takes(e.item.status))
            .count()
    }

    /// The output folder a run uses when none is chosen.
    pub fn default_output_dir(&self) -> Option<PathBuf> {
        let inner = self.lock();
        inner
            .entries
            .first()
            .map(|e| plan::default_output_dir(&e.source))
    }

    /// Starts a run: queues the selected files and marks the batch running.
    fn begin(&self, job: &Job, selection: Selection) -> Result<Work, BatchError> {
        let mut inner = self.lock();
        if inner.cancel.is_some() {
            return Err(BatchError::Running);
        }
        let mut files = Vec::new();
        let mut changed = Vec::new();
        for entry in &mut inner.entries {
            if selection.takes(entry.item.status) {
                if entry.item.status != ItemStatus::Queued {
                    requeue(entry);
                    changed.push(entry.item.clone());
                }
                files.push((entry.item.id, entry.source.clone()));
            }
        }
        if files.is_empty() {
            return Err(BatchError::Empty);
        }
        let cancel = CancellationToken::new();
        inner.cancel = Some(cancel.clone());
        inner.last_job = Some(job.clone());
        Ok(Work {
            files,
            changed,
            cancel,
        })
    }

    fn finish(&self) {
        self.lock().cancel = None;
    }

    /// Cancels the running batch at once: the requests in flight are
    /// dropped (nothing is saved for them; they are marked cancelled), and
    /// the files not yet started are skipped. Files already finished stay
    /// done. Returns false when no batch runs.
    pub fn cancel(&self) -> bool {
        match &self.lock().cancel {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    fn update(&self, id: u64, change: impl FnOnce(&mut Entry)) -> Option<BatchItem> {
        let mut inner = self.lock();
        let entry = inner.entries.iter_mut().find(|e| e.item.id == id)?;
        change(entry);
        entry.item.version += 1;
        Some(entry.item.clone())
    }
}

fn requeue(entry: &mut Entry) {
    entry.output = None;
    let item = &mut entry.item;
    item.status = ItemStatus::Queued;
    item.output = None;
    item.output_name = None;
    item.width = None;
    item.height = None;
    item.bytes = None;
    item.note = None;
    item.error = None;
    item.version += 1;
}

struct Work {
    files: Vec<(u64, PathBuf)>,
    changed: Vec<BatchItem>,
    cancel: CancellationToken,
}

/// Checks the account before a run: what the batch costs and whether the
/// balance covers it. A balance that is too low refuses the run.
pub async fn preflight(
    client: &ApiClient,
    operation: &Operation,
    files: usize,
) -> Result<Estimate, BatchError> {
    operation
        .validate()
        .map_err(|e| BatchError::Invalid(e.to_string()))?;
    let account = client.account().await?;
    let estimate = plan::estimate(&account, operation.price_key(), files);
    if !estimate.enough {
        return Err(BatchError::InsufficientCredits {
            cost: estimate.cost,
            credits: estimate.credits.unwrap_or(0),
        });
    }
    Ok(estimate)
}

/// Runs a batch to its end and returns how it went.
///
/// `parallel` files are in flight at once. A file that fails is marked and
/// the batch goes on, except for an error every other file would get too
/// (no credits, a refused key): that stops the batch. The files not yet
/// started are then skipped; those in flight are left to finish, since
/// they may well succeed.
///
/// Cancelling (see [`BatchStore::cancel`]) is immediate: requests in flight
/// are dropped, also while they wait to retry, and no file is written for
/// them. A result that had already arrived in full is still saved.
pub async fn run(
    store: &BatchStore,
    client: &ApiClient,
    job: Job,
    selection: Selection,
    parallel: usize,
    events: &dyn BatchEvents,
) -> Result<BatchSummary, BatchError> {
    job.operation
        .validate()
        .map_err(|e| BatchError::Invalid(e.to_string()))?;
    let work = store.begin(&job, selection)?;
    for item in &work.changed {
        events.item(item);
    }
    if let Err(e) = tokio::fs::create_dir_all(&job.output_dir).await {
        store.finish();
        return Err(BatchError::OutputDir(format!(
            "{}: {e}",
            job.output_dir.display()
        )));
    }

    let endpoint = job.operation.endpoint();
    let query = job.operation.query();
    // `cancel` is the user's: it drops what is in flight. `stop` also ends
    // on an error that stops the batch, and only keeps new work from starting.
    let cancel = &work.cancel;
    let stop = &cancel.child_token();
    let stopped: Mutex<Option<ItemError>> = Mutex::new(None);
    let total = work.files.len();
    let (failed, skipped, aborted) = (
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );

    futures_util::stream::iter(work.files)
        .for_each_concurrent(
            parallel.clamp(MIN_PARALLEL, MAX_PARALLEL),
            |(id, source)| {
                let (query, stopped, output_dir) = (&query, &stopped, &job.output_dir);
                let (failed, skipped, aborted) = (&failed, &skipped, &aborted);
                async move {
                    if stop.is_cancelled() {
                        skipped.fetch_add(1, Ordering::Relaxed);
                        mark(store, events, id, |e| e.item.status = ItemStatus::Skipped);
                        return;
                    }
                    mark(store, events, id, |e| e.item.status = ItemStatus::Running);
                    // Dropping the request closes its connection. Only the
                    // request is raced: a result that has arrived is saved.
                    let received = tokio::select! {
                        biased;
                        () = cancel.cancelled() => Err(ApiError::Cancelled),
                        result = client.process(endpoint, query, &source, &[], Some(stop)) => result,
                    };
                    let outcome = match received {
                        Ok(result) => save(output_dir, &source, result).await,
                        Err(e) => Err(e),
                    };
                    match outcome {
                        Ok(saved) => mark(store, events, id, |e| {
                            e.item.status = ItemStatus::Done;
                            e.item.output = Some(saved.path.display().to_string());
                            e.item.output_name = saved
                                .path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned());
                            e.item.width = saved.width;
                            e.item.height = saved.height;
                            e.item.bytes = Some(saved.bytes);
                            e.item.note = saved.note;
                            e.output = Some(saved.path);
                        }),
                        // Given up by the user while under way...
                        Err(ApiError::Cancelled) if cancel.is_cancelled() => {
                            aborted.fetch_add(1, Ordering::Relaxed);
                            mark(store, events, id, |e| {
                                e.item.status = ItemStatus::Cancelled;
                            });
                        }
                        // ...or waiting to retry when the batch was stopped.
                        Err(ApiError::Cancelled) => {
                            skipped.fetch_add(1, Ordering::Relaxed);
                            mark(store, events, id, |e| e.item.status = ItemStatus::Skipped);
                        }
                        Err(error) => {
                            let item_error = ItemError::from(&error);
                            if error.is_fatal() {
                                stopped
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                                    .get_or_insert_with(|| item_error.clone());
                                stop.cancel();
                            }
                            failed.fetch_add(1, Ordering::Relaxed);
                            mark(store, events, id, |e| {
                                e.item.status = ItemStatus::Failed;
                                e.item.error = Some(item_error);
                            });
                        }
                    }
                }
            },
        )
        .await;

    let stopped = stopped
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let cancelled = cancel.is_cancelled();
    store.finish();
    let (failed, skipped, aborted) = (
        failed.into_inner(),
        skipped.into_inner(),
        aborted.into_inner(),
    );
    let summary = BatchSummary {
        total,
        done: total.saturating_sub(failed + skipped + aborted),
        failed,
        skipped,
        aborted,
        cancelled,
        stopped,
        output_dir: job.output_dir.display().to_string(),
    };
    events.finished(&summary);
    Ok(summary)
}

fn mark(store: &BatchStore, events: &dyn BatchEvents, id: u64, change: impl FnOnce(&mut Entry)) {
    if let Some(item) = store.update(id, change) {
        events.item(&item);
    }
}

struct Saved {
    path: PathBuf,
    width: Option<u32>,
    height: Option<u32>,
    bytes: u64,
    note: Option<String>,
}

/// Writes a result under a name derived from its source, with the extension
/// of the type the API returned.
async fn save(
    output_dir: &Path,
    source: &Path,
    result: crate::api::ImageResult,
) -> Result<Saved, ApiError> {
    let stem = plan::output_stem(source);
    let dir = output_dir.to_path_buf();
    let (width, height, note) = (result.width, result.height, result.note());
    let extension = result.extension();
    let bytes = result.bytes;
    let size = bytes.len() as u64;
    let path = tokio::task::spawn_blocking(move || plan::write_new(&dir, &stem, extension, &bytes))
        .await
        .map_err(|e| ApiError::Io(e.to_string()))?
        .map_err(|e| ApiError::Io(format!("could not write the result: {e}")))?;
    Ok(Saved {
        path,
        width,
        height,
        bytes: size,
        note,
    })
}
