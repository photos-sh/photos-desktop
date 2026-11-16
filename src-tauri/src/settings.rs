//! The app's settings: a small JSON file in the app's config folder.
//! The API key is not here; see `keychain`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::api::client::{DEFAULT_BASE_URL, normalize_base_url};
use crate::batch::{DEFAULT_PARALLEL, MAX_PARALLEL, MIN_PARALLEL};
use crate::ops::Format;

pub const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    /// Where the API is. Only changed for testing or a private engine.
    pub base_url: String,
    /// Files a batch processes at once.
    pub parallel: usize,
    /// The format tools offer first; none keeps each tool's own default.
    pub default_format: Option<Format>,
    /// Ask GitHub for a newer version when the app starts. The update is
    /// only offered: nothing is downloaded without asking.
    pub check_for_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            parallel: DEFAULT_PARALLEL,
            default_format: None,
            check_for_updates: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("{0}")]
    Invalid(String),
    #[error("Could not save the settings: {0}")]
    Save(String),
}

impl Settings {
    /// Checks the values and tidies them (an empty base URL is the default).
    pub fn validated(mut self) -> Result<Self, SettingsError> {
        if self.base_url.trim().is_empty() {
            self.base_url = DEFAULT_BASE_URL.to_string();
        }
        self.base_url = normalize_base_url(&self.base_url)
            .map_err(|e| SettingsError::Invalid(e.to_string()))?;
        if !(MIN_PARALLEL..=MAX_PARALLEL).contains(&self.parallel) {
            return Err(SettingsError::Invalid(format!(
                "Parallel files goes from {MIN_PARALLEL} to {MAX_PARALLEL}."
            )));
        }
        Ok(self)
    }

    /// Reads the settings; a missing or unreadable file gives the defaults,
    /// and values that do not hold are replaced by their defaults.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut settings: Self = serde_json::from_str(&text).unwrap_or_default();
        if normalize_base_url(&settings.base_url).is_err() {
            settings.base_url = DEFAULT_BASE_URL.to_string();
        }
        settings.parallel = settings.parallel.clamp(MIN_PARALLEL, MAX_PARALLEL);
        settings
    }

    /// Writes the settings, replacing the file in one step.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let error = |e: std::io::Error| SettingsError::Save(e.to_string());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(error)?;
        }
        let text =
            serde_json::to_string_pretty(self).map_err(|e| SettingsError::Save(e.to_string()))?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text).map_err(error)?;
        std::fs::rename(&temporary, path).map_err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let s = Settings::default();
        assert_eq!(s.base_url, "https://api.photos.sh");
        assert_eq!(s.parallel, 3);
        assert_eq!(s.default_format, None);
        assert!(s.check_for_updates);
    }

    #[test]
    fn settings_saved_before_updates_check_for_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(
            &path,
            r#"{"base_url": "https://api.photos.sh", "parallel": 4, "default_format": null}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.parallel, 4);
        assert!(settings.check_for_updates);
    }

    #[test]
    fn saved_settings_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join(FILE_NAME);
        assert_eq!(Settings::load(&path), Settings::default());
        let settings = Settings {
            base_url: "http://127.0.0.1:8098/api".into(),
            parallel: 5,
            default_format: Some(Format::Webp),
            check_for_updates: false,
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(r#""default_format": "webp""#));
        // Nothing but the settings is in the file or beside it.
        assert!(!text.to_lowercase().contains("key"));
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn values_are_checked() {
        let tidy = Settings {
            base_url: " https://example.com/api/ ".into(),
            ..Settings::default()
        }
        .validated()
        .unwrap();
        assert_eq!(tidy.base_url, "https://example.com/api");
        let empty = Settings {
            base_url: "  ".into(),
            ..Settings::default()
        }
        .validated()
        .unwrap();
        assert_eq!(empty.base_url, DEFAULT_BASE_URL);
        for parallel in [0, 9] {
            let bad = Settings {
                parallel,
                ..Settings::default()
            };
            assert!(bad.validated().is_err());
        }
        let bad = Settings {
            base_url: "photos.sh".into(),
            ..Settings::default()
        };
        assert!(bad.validated().is_err());
    }

    #[test]
    fn a_damaged_file_gives_usable_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::write(
            &path,
            r#"{"base_url":"nope","parallel":99,"later":"field"}"#,
        )
        .unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.base_url, DEFAULT_BASE_URL);
        assert_eq!(loaded.parallel, 8);
    }
}
