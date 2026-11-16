//! Updates. The release workflow publishes `latest.json` with every release
//! on GitHub; the app reads the latest one, and when it names a newer
//! version, offers it. Nothing is downloaded until the user says so.
//!
//! Every installer is signed with the release key, and the updater checks
//! that signature against the public key in `tauri.conf.json` before it
//! installs anything: a download that is not ours is refused.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::core::AppError;

/// Where a release can be downloaded by hand, when the update cannot be
/// installed from inside the app.
pub const RELEASES_PAGE: &str = "https://github.com/photos-sh/photos-desktop/releases/latest";

/// Events sent while an update downloads.
pub mod event {
    /// Bytes downloaded so far: a `Progress`.
    pub const PROGRESS: &str = "update:progress";
}

/// The update the last check found, kept until it is installed.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

impl Pending {
    fn set(&self, update: Option<Update>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = update;
    }

    fn get(&self) -> Option<Update> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// A newer version, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// The release notes, as written on the release.
    pub notes: Option<String>,
}

impl UpdateInfo {
    fn of(update: &Update) -> Self {
        Self {
            version: update.version.clone(),
            current_version: update.current_version.clone(),
            notes: update
                .body
                .as_ref()
                .map(|b| b.trim().to_string())
                .filter(|b| !b.is_empty()),
        }
    }
}

/// How far a download is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub downloaded: u64,
    /// The size, when the server said it.
    pub total: Option<u64>,
}

/// Asks whether there is a newer version. `None` when this is the latest.
pub async fn check(app: &AppHandle, pending: &Pending) -> Result<Option<UpdateInfo>, AppError> {
    let found = app
        .updater()
        .map_err(|e| failed("Could not check for updates", e))?
        .check()
        .await
        .map_err(|e| failed("Could not check for updates", e))?;
    let info = found.as_ref().map(UpdateInfo::of);
    pending.set(found);
    Ok(info)
}

/// Downloads the update the last check found, checks its signature and
/// installs it. The app has to be restarted afterwards (on Windows the
/// installer closes it itself).
pub async fn install(app: &AppHandle, pending: &Pending) -> Result<(), AppError> {
    let update = pending
        .get()
        .ok_or_else(|| AppError::new("no_update", "There is no update to install. Check again."))?;
    let mut downloaded = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit(event::PROGRESS, Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| failed("Could not install the update", e))
}

fn failed(what: &str, error: tauri_plugin_updater::Error) -> AppError {
    AppError::new("update_failed", format!("{what}: {error}"))
}
