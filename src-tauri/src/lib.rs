//! The Photos.sh desktop app's core.
//!
//! The logic lives in modules that know nothing about the window ([`api`],
//! [`batch`], [`editor`], [`settings`], [`keychain`], brought together by
//! [`core::Core`]). This file is the Tauri side: thin commands, the events
//! that carry progress to the UI, and the `preview` protocol that serves
//! pictures to it.

pub mod api;
pub mod batch;
pub mod core;
pub mod editor;
pub mod keychain;
pub mod mask;
pub mod ops;
pub mod preview;
pub mod recipe;
pub mod settings;
pub mod updates;

#[cfg(test)]
mod core_tests;
#[cfg(test)]
pub(crate) mod testing;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::http::{Response, StatusCode, header};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::api::Account;
use crate::batch::plan::IMAGE_EXTENSIONS;
use crate::batch::{BatchEvents, BatchItem, BatchSnapshot, BatchSummary, Estimate};
use crate::core::{AppError, Catalog, Core, EditorTool};
use crate::editor::EditorSnapshot;
use crate::keychain::{KeyStatus, OsKeychain};
use crate::ops::Operation;
use crate::settings::Settings;
use crate::updates::{Pending, UpdateInfo};

/// Events the core sends to the UI.
pub mod event {
    /// One batch file changed: a `BatchItem`.
    pub const BATCH_ITEM: &str = "batch:item";
    /// A batch run ended: a `BatchSummary`.
    pub const BATCH_FINISHED: &str = "batch:finished";
    /// A batch run could not go on: an `AppError`.
    pub const BATCH_ERROR: &str = "batch:error";
}

type Shared<'a> = State<'a, Arc<Core>>;
type Answer<T> = Result<T, AppError>;

/// Runs blocking work (the keychain, the disk, a native dialog) off the
/// async runtime's threads.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Answer<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| AppError::new("internal", e.to_string()))
}

fn to_paths(list: Vec<String>) -> Vec<PathBuf> {
    list.into_iter().map(PathBuf::from).collect()
}

fn picked(path: FilePath) -> Option<String> {
    path.into_path().ok().map(|p| p.display().to_string())
}

// --- settings, key, account --------------------------------------------------

#[tauri::command]
fn settings_get(core: Shared<'_>) -> Settings {
    core.settings()
}

#[tauri::command]
async fn settings_set(core: Shared<'_>, settings: Settings) -> Answer<Settings> {
    let core = core.inner().clone();
    blocking(move || core.set_settings(settings)).await?
}

#[tauri::command]
async fn key_status(core: Shared<'_>) -> Answer<KeyStatus> {
    let core = core.inner().clone();
    blocking(move || core.key_status()).await
}

#[tauri::command]
async fn key_set(core: Shared<'_>, key: String) -> Answer<KeyStatus> {
    let core = core.inner().clone();
    blocking(move || core.set_key(&key)).await?
}

#[tauri::command]
async fn key_remove(core: Shared<'_>) -> Answer<KeyStatus> {
    let core = core.inner().clone();
    blocking(move || core.remove_key()).await?
}

#[tauri::command]
async fn account(core: Shared<'_>) -> Answer<Account> {
    core.account().await
}

#[tauri::command]
async fn catalog(core: Shared<'_>) -> Answer<Catalog> {
    core.catalog().await
}

// --- native dialogs ----------------------------------------------------------

#[tauri::command]
async fn pick_images(window: WebviewWindow) -> Answer<Vec<String>> {
    blocking(move || {
        window
            .dialog()
            .file()
            .set_parent(&window)
            .set_title("Add photos")
            .add_filter("Photos", IMAGE_EXTENSIONS)
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .filter_map(picked)
            .collect()
    })
    .await
}

#[tauri::command]
async fn pick_image(window: WebviewWindow) -> Answer<Option<String>> {
    blocking(move || {
        window
            .dialog()
            .file()
            .set_parent(&window)
            .set_title("Open a photo")
            .add_filter("Photos", IMAGE_EXTENSIONS)
            .blocking_pick_file()
            .and_then(picked)
    })
    .await
}

#[tauri::command]
async fn pick_folder(window: WebviewWindow, title: Option<String>) -> Answer<Option<String>> {
    blocking(move || {
        window
            .dialog()
            .file()
            .set_parent(&window)
            .set_title(title.unwrap_or_else(|| "Choose a folder".to_string()))
            .set_can_create_directories(true)
            .blocking_pick_folder()
            .and_then(picked)
    })
    .await
}

// --- batch -------------------------------------------------------------------

struct WindowEvents(AppHandle);

impl BatchEvents for WindowEvents {
    fn item(&self, item: &BatchItem) {
        let _ = self.0.emit(event::BATCH_ITEM, item);
    }
    fn finished(&self, summary: &BatchSummary) {
        let _ = self.0.emit(event::BATCH_FINISHED, summary);
    }
}

#[tauri::command]
fn batch_snapshot(core: Shared<'_>) -> BatchSnapshot {
    core.batch_snapshot()
}

#[tauri::command]
async fn batch_add(core: Shared<'_>, paths: Vec<String>) -> Answer<BatchSnapshot> {
    let core = core.inner().clone();
    blocking(move || core.batch_add(&to_paths(paths))).await?
}

#[tauri::command]
fn batch_remove(core: Shared<'_>, id: u64) -> Answer<BatchSnapshot> {
    core.batch().remove(id)?;
    Ok(core.batch_snapshot())
}

#[tauri::command]
fn batch_clear(core: Shared<'_>) -> Answer<BatchSnapshot> {
    core.batch().clear()?;
    Ok(core.batch_snapshot())
}

#[tauri::command]
fn batch_reset(core: Shared<'_>) -> Answer<BatchSnapshot> {
    core.batch().reset()?;
    Ok(core.batch_snapshot())
}

#[tauri::command]
async fn batch_estimate(core: Shared<'_>, operation: Operation) -> Answer<Estimate> {
    core.batch_estimate(&operation).await
}

/// Checks the batch (options, key, credits, output folder) and starts it.
/// It answers once the batch is under way; progress follows as events.
#[tauri::command]
async fn batch_start(
    app: AppHandle,
    core: Shared<'_>,
    operation: Operation,
    output_dir: Option<String>,
) -> Answer<Estimate> {
    let prepared = core
        .batch_prepare(operation, output_dir.map(PathBuf::from))
        .await?;
    Ok(spawn_batch(app, core.inner().clone(), prepared))
}

/// Runs the last operation again on the files that failed or were skipped.
#[tauri::command]
async fn batch_retry(app: AppHandle, core: Shared<'_>) -> Answer<Estimate> {
    let prepared = core.batch_prepare_retry().await?;
    Ok(spawn_batch(app, core.inner().clone(), prepared))
}

fn spawn_batch(app: AppHandle, core: Arc<Core>, prepared: core::PreparedBatch) -> Estimate {
    let estimate = prepared.estimate.clone();
    tauri::async_runtime::spawn(async move {
        let events = WindowEvents(app.clone());
        if let Err(error) = core.batch_run(prepared, &events).await {
            let _ = app.emit(event::BATCH_ERROR, error);
        }
    });
    estimate
}

#[tauri::command]
fn batch_cancel(core: Shared<'_>) -> bool {
    core.batch().cancel()
}

/// Shows a finished file (or, without one, its source) in the file manager.
#[tauri::command]
async fn batch_reveal(core: Shared<'_>, id: u64, source: bool) -> Answer<()> {
    let path = if source {
        core.batch().source_path(id)
    } else {
        core.batch().output_path(id)
    }
    .ok_or_else(|| AppError::new("not_found", "That file is not in the list any more."))?;
    blocking(move || {
        tauri_plugin_opener::reveal_item_in_dir(&path)
            .map_err(|e| AppError::new("io", format!("Could not open the file manager: {e}")))
    })
    .await?
}

// --- editor ------------------------------------------------------------------

#[tauri::command]
async fn editor_open(core: Shared<'_>, path: String) -> Answer<EditorSnapshot> {
    let core = core.inner().clone();
    blocking(move || core.editor_open(&PathBuf::from(path))).await?
}

#[tauri::command]
async fn editor_close(core: Shared<'_>) -> Answer<()> {
    let core = core.inner().clone();
    blocking(move || core.editor_close()).await
}

#[tauri::command]
fn editor_snapshot(core: Shared<'_>) -> Option<EditorSnapshot> {
    core.editor_snapshot()
}

#[tauri::command]
async fn editor_apply(core: Shared<'_>, tool: EditorTool) -> Answer<EditorSnapshot> {
    core.editor_apply(tool).await
}

#[tauri::command]
async fn editor_check(
    core: Shared<'_>,
    marketplace: Option<String>,
    subject: bool,
    text: bool,
) -> Answer<serde_json::Value> {
    core.editor_check(marketplace.as_deref(), subject, text)
        .await
}

#[tauri::command]
fn editor_cancel(core: Shared<'_>) {
    core.editor_cancel();
}

#[tauri::command]
fn editor_undo(core: Shared<'_>) -> Answer<EditorSnapshot> {
    core.editor_undo()
}

#[tauri::command]
fn editor_redo(core: Shared<'_>) -> Answer<EditorSnapshot> {
    core.editor_redo()
}

#[tauri::command]
fn editor_go_to(core: Shared<'_>, cursor: usize) -> Answer<EditorSnapshot> {
    core.editor_go_to(cursor)
}

/// Asks where to save the current picture. `None` when the dialog was
/// dismissed.
#[tauri::command]
async fn editor_pick_save(core: Shared<'_>, window: WebviewWindow) -> Answer<Option<String>> {
    let snapshot = core
        .editor_snapshot()
        .ok_or_else(|| AppError::from(editor::EditorError::NothingOpen))?;
    blocking(move || {
        let extension = snapshot
            .suggested_name
            .rsplit('.')
            .next()
            .unwrap_or("png")
            .to_string();
        let mut dialog = window
            .dialog()
            .file()
            .set_parent(&window)
            .set_title("Save as")
            .set_file_name(&snapshot.suggested_name)
            .add_filter(extension.to_uppercase(), &[extension.as_str()]);
        if let Some(folder) = PathBuf::from(&snapshot.path).parent() {
            dialog = dialog.set_directory(folder);
        }
        dialog.blocking_save_file().and_then(picked)
    })
    .await
}

/// Saves the current picture. Answers with the path written. Replacing the
/// original is refused (`confirm_replace_original`) unless `replace_original`
/// is set, which the UI does only after asking.
#[tauri::command]
async fn editor_save(core: Shared<'_>, path: String, replace_original: bool) -> Answer<String> {
    let core = core.inner().clone();
    blocking(move || {
        core.editor_save(&PathBuf::from(path), replace_original)
            .map(|p| p.display().to_string())
    })
    .await?
}

// --- updates -----------------------------------------------------------------

#[tauri::command]
async fn update_check(app: AppHandle, pending: State<'_, Pending>) -> Answer<Option<UpdateInfo>> {
    updates::check(&app, &pending).await
}

/// Installs the update the last check found and restarts the app. Refused
/// while a batch runs; a photo open in the editor is lost unless saved, which
/// the UI asks about first.
#[tauri::command]
async fn update_install(
    app: AppHandle,
    core: Shared<'_>,
    pending: State<'_, Pending>,
) -> Answer<()> {
    if core.batch().is_running() {
        return Err(AppError::new(
            "batch_running",
            "A batch is running. Install the update when it has finished, or cancel it.",
        ));
    }
    updates::install(&app, &pending).await?;
    core.editor_close();
    app.restart()
}

/// Opens the latest release on GitHub, for an update the app cannot install
/// itself.
#[tauri::command]
async fn update_open_releases() -> Answer<()> {
    blocking(|| {
        tauri_plugin_opener::open_url(updates::RELEASES_PAGE, None::<&str>)
            .map_err(|e| AppError::new("io", format!("Could not open the browser: {e}")))
    })
    .await?
}

// --- the preview protocol ----------------------------------------------------

/// Answers `preview://localhost/<key>` (`http://preview.localhost/<key>` on
/// Windows) with the picture behind the key; see [`Core::preview`].
fn preview_response(core: &Core, uri_path: &str) -> Response<Vec<u8>> {
    let key = percent_encoding::percent_decode_str(uri_path).decode_utf8_lossy();
    match core.preview(&key) {
        Ok(picture) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, picture.mime)
            .header(header::CACHE_CONTROL, "private, max-age=3600")
            .header("X-Content-Type-Options", "nosniff")
            .body(picture.bytes),
        Err(error) => Response::builder()
            .status(match error {
                preview::PreviewError::Io(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            })
            .header(header::CONTENT_TYPE, "text/plain")
            .body(error.to_string().into_bytes()),
    }
    .unwrap_or_else(|_| Response::new(Vec::new()))
}

/// Starts the app.
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Pending::default())
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join(settings::FILE_NAME);
            let core = Core::new(settings_path, Box::new(OsKeychain::default()))?;
            app.manage(Arc::new(core));
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("preview", |context, request, responder| {
            let core = context.app_handle().state::<Arc<Core>>().inner().clone();
            let path = request.uri().path().to_string();
            // Decoding a photo takes a moment: not on the web view's thread.
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(preview_response(&core, &path));
            });
        })
        .invoke_handler(tauri::generate_handler![
            settings_get,
            settings_set,
            key_status,
            key_set,
            key_remove,
            account,
            catalog,
            pick_images,
            pick_image,
            pick_folder,
            batch_snapshot,
            batch_add,
            batch_remove,
            batch_clear,
            batch_reset,
            batch_estimate,
            batch_start,
            batch_retry,
            batch_cancel,
            batch_reveal,
            editor_open,
            editor_close,
            editor_snapshot,
            editor_apply,
            editor_check,
            editor_cancel,
            editor_undo,
            editor_redo,
            editor_go_to,
            editor_pick_save,
            editor_save,
            update_check,
            update_install,
            update_open_releases,
        ])
        .build(tauri::generate_context!())
        .expect("the app could not start");

    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            // Remove the editor's temporary steps.
            if let Some(core) = app.try_state::<Arc<Core>>() {
                core.editor_close();
            }
        }
    });
}
