//! The single-photo editor's steps.
//!
//! Each applied tool adds a step: its result, kept as a file in a temporary
//! folder that is removed when the photo is closed (or the app quits). The
//! next tool works on the current step; undo and redo move through them.
//! The original is only ever read.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tempfile::TempDir;

use crate::api::ImageResult;
use crate::api::types::extension_for;
use crate::batch::plan::is_image;
use crate::preview;

#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    #[error("Open a photo first.")]
    NothingOpen,
    #[error("That file is not a photo this app can open.")]
    NotAnImage,
    #[error("The photo could not be opened: {0}")]
    Open(String),
    #[error("That would replace the original photo.")]
    WouldReplaceOriginal,
    #[error("{0} already exists. Choose another name.")]
    Exists(String),
    #[error("Could not save the photo: {0}")]
    Save(String),
    #[error("Could not keep the result: {0}")]
    Step(String),
}

impl EditorError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NothingOpen => "nothing_open",
            Self::NotAnImage => "not_an_image",
            Self::WouldReplaceOriginal => "confirm_replace_original",
            Self::Exists(_) => "exists",
            Self::Open(_) | Self::Save(_) | Self::Step(_) => "io",
        }
    }
}

#[derive(Debug)]
struct Step {
    id: u64,
    label: String,
    path: PathBuf,
    content_type: String,
    width: Option<u32>,
    height: Option<u32>,
    bytes: u64,
    note: Option<String>,
}

/// A step as the UI sees it. Its picture is at the preview key
/// `editor/<session>/<id>`; the original's id is 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StepInfo {
    pub id: u64,
    pub label: String,
    pub content_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bytes: Option<u64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditorSnapshot {
    /// Changes with every photo opened, so pictures are not mixed up.
    pub session: u64,
    pub name: String,
    pub path: String,
    pub original: StepInfo,
    /// The steps applied, oldest first (those undone included).
    pub steps: Vec<StepInfo>,
    /// How many steps are in effect: 0 shows the original.
    pub cursor: usize,
    /// The step in effect (the original at cursor 0).
    pub current: StepInfo,
    pub can_undo: bool,
    pub can_redo: bool,
    /// A name to offer in "Save as".
    pub suggested_name: String,
}

pub struct EditorSession {
    session: u64,
    dir: TempDir,
    original: PathBuf,
    original_info: StepInfo,
    steps: Vec<Step>,
    cursor: usize,
    next_id: u64,
}

impl EditorSession {
    /// Opens a photo. Nothing is copied: the original is read in place.
    pub fn open(session: u64, path: &Path) -> Result<Self, EditorError> {
        if !is_image(path) {
            return Err(EditorError::NotAnImage);
        }
        let metadata = std::fs::metadata(path).map_err(|e| EditorError::Open(e.to_string()))?;
        if !metadata.is_file() {
            return Err(EditorError::NotAnImage);
        }
        let dir = tempfile::Builder::new()
            .prefix("photos-sh-editor-")
            .tempdir()
            .map_err(|e| EditorError::Open(e.to_string()))?;
        let size = preview::dimensions(path);
        let extension = extension_of(path);
        Ok(Self {
            session,
            dir,
            original: path.to_path_buf(),
            original_info: StepInfo {
                id: 0,
                label: "Original".to_string(),
                content_type: content_type_for(&extension).to_string(),
                width: size.map(|s| s.0),
                height: size.map(|s| s.1),
                bytes: Some(metadata.len()),
                note: None,
            },
            steps: Vec::new(),
            cursor: 0,
            next_id: 1,
        })
    }

    pub fn session(&self) -> u64 {
        self.session
    }

    /// The folder the steps are kept in.
    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// The file the next tool works on.
    pub fn current_path(&self) -> &Path {
        match self.cursor {
            0 => &self.original,
            n => &self.steps[n - 1].path,
        }
    }

    /// The current picture's size in pixels, when known.
    pub fn current_size(&self) -> Option<(u32, u32)> {
        let info = self.current_info();
        match (info.width, info.height) {
            (Some(w), Some(h)) => Some((w, h)),
            _ => preview::dimensions(self.current_path()),
        }
    }

    /// The file behind a step id (0 is the original).
    pub fn path_of(&self, id: u64) -> Option<&Path> {
        if id == 0 {
            return Some(&self.original);
        }
        self.steps
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.path.as_path())
    }

    /// Adds a tool's result as the step after the current one. Steps that
    /// were undone are dropped, and their files removed.
    pub fn push(&mut self, label: &str, result: &ImageResult) -> Result<(), EditorError> {
        for dropped in self.steps.drain(self.cursor..) {
            let _ = std::fs::remove_file(&dropped.path);
        }
        let id = self.next_id;
        let path = self
            .dir
            .path()
            .join(format!("step-{id}.{}", result.extension()));
        std::fs::write(&path, &result.bytes).map_err(|e| EditorError::Step(e.to_string()))?;
        self.next_id += 1;
        self.steps.push(Step {
            id,
            label: label.to_string(),
            path,
            content_type: result.content_type.clone(),
            width: result.width,
            height: result.height,
            bytes: result.bytes.len() as u64,
            note: result.note(),
        });
        self.cursor = self.steps.len();
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.cursor >= self.steps.len() {
            return false;
        }
        self.cursor += 1;
        true
    }

    /// Shows the state after `cursor` steps (0 = the original).
    pub fn go_to(&mut self, cursor: usize) -> bool {
        if cursor > self.steps.len() {
            return false;
        }
        self.cursor = cursor;
        true
    }

    fn current_info(&self) -> StepInfo {
        match self.cursor {
            0 => self.original_info.clone(),
            n => info(&self.steps[n - 1]),
        }
    }

    fn current_extension(&self) -> String {
        match self.cursor {
            0 => extension_of(&self.original),
            n => extension_for(&self.steps[n - 1].content_type).to_string(),
        }
    }

    /// `<name>-edited.<extension of the current picture>`.
    pub fn suggested_name(&self) -> String {
        let stem = crate::batch::plan::output_stem(&self.original);
        format!("{stem}-edited.{}", self.current_extension())
    }

    pub fn snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            session: self.session,
            name: self
                .original
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: self.original.display().to_string(),
            original: self.original_info.clone(),
            steps: self.steps.iter().map(info).collect(),
            cursor: self.cursor,
            current: self.current_info(),
            can_undo: self.cursor > 0,
            can_redo: self.cursor < self.steps.len(),
            suggested_name: self.suggested_name(),
        }
    }

    /// Saves the current picture to `destination`.
    ///
    /// The name gets the picture's real extension when it has another one
    /// (a PNG saved as `x.jpg` would not open everywhere). The original is
    /// replaced only with `replace_original`: the caller asks the user first.
    pub fn save_as(
        &self,
        destination: &Path,
        replace_original: bool,
    ) -> Result<PathBuf, EditorError> {
        let extension = self.current_extension();
        let mut destination = destination.to_path_buf();
        if !same_extension(&extension_of(&destination), &extension) {
            let mut name = destination
                .file_name()
                .ok_or_else(|| EditorError::Save("that is not a file name".to_string()))?
                .to_os_string();
            name.push(format!(".{extension}"));
            destination.set_file_name(name);
            // The dialog asked about the name the user typed, not this one.
            if destination.exists() {
                return Err(EditorError::Exists(destination.display().to_string()));
            }
        }
        let source = self.current_path();
        if same_file(&destination, &self.original) {
            if self.cursor == 0 {
                // Saving the untouched original onto itself: nothing to do.
                return Ok(destination);
            }
            if !replace_original {
                return Err(EditorError::WouldReplaceOriginal);
            }
        }
        if same_file(&destination, source) {
            return Ok(destination);
        }
        std::fs::copy(source, &destination).map_err(|e| EditorError::Save(e.to_string()))?;
        Ok(destination)
    }
}

fn info(step: &Step) -> StepInfo {
    StepInfo {
        id: step.id,
        label: step.label.clone(),
        content_type: step.content_type.clone(),
        width: step.width,
        height: step.height,
        bytes: Some(step.bytes),
        note: step.note.clone(),
    }
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn same_extension(a: &str, b: &str) -> bool {
    fn canonical(e: &str) -> &str {
        match e {
            "jpeg" => "jpg",
            "tif" => "tiff",
            "heif" => "heic",
            other => other,
        }
    }
    !a.is_empty() && canonical(a) == canonical(b)
}

fn content_type_for(extension: &str) -> &'static str {
    match extension {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "tif" | "tiff" => "image/tiff",
        "bmp" => "image/bmp",
        "heic" | "heif" => "image/heic",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    }
}

/// Whether two paths name the same existing file.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{write_fake, write_png};

    fn result(content_type: &str, bytes: &[u8]) -> ImageResult {
        ImageResult {
            bytes: bytes.to_vec(),
            content_type: content_type.to_string(),
            width: Some(10),
            height: Some(5),
            ..ImageResult::default()
        }
    }

    #[test]
    fn a_step_keeps_what_its_tool_reported() {
        let dir = tempfile::tempdir().unwrap();
        let original = write_png(dir.path(), "group.png", 4, 4);
        let mut editor = EditorSession::open(1, &original).unwrap();
        let hidden = ImageResult {
            faces: Some(2),
            ..result("image/png", b"a")
        };
        editor.push("Hide faces", &hidden).unwrap();
        let centred = ImageResult {
            endpoint: "/v1/crop".into(),
            subject_found: Some(false),
            ..result("image/png", b"b")
        };
        editor.push("Crop to 4:5", &centred).unwrap();
        let snapshot = editor.snapshot();
        assert_eq!(snapshot.steps[0].note.as_deref(), Some("2 faces hidden"));
        assert_eq!(
            snapshot.current.note.as_deref(),
            Some("No subject or face found: the crop is centred")
        );
        assert_eq!(snapshot.original.note, None);
    }

    #[test]
    fn only_photos_open() {
        let dir = tempfile::tempdir().unwrap();
        let text = write_fake(dir.path(), "notes.txt");
        assert!(matches!(
            EditorSession::open(1, &text),
            Err(EditorError::NotAnImage)
        ));
        assert!(matches!(
            EditorSession::open(1, &dir.path().join("missing.png")),
            Err(EditorError::Open(_))
        ));
        assert!(matches!(
            EditorSession::open(1, dir.path()),
            Err(EditorError::NotAnImage)
        ));
    }

    #[test]
    fn each_tool_works_on_the_previous_result() {
        let dir = tempfile::tempdir().unwrap();
        let original = write_png(dir.path(), "shoe.png", 40, 30);
        let mut editor = EditorSession::open(7, &original).unwrap();
        assert_eq!(editor.current_path(), original);
        assert_eq!(editor.current_size(), Some((40, 30)));
        let start = editor.snapshot();
        assert_eq!((start.session, start.cursor), (7, 0));
        assert_eq!(start.name, "shoe.png");
        assert_eq!(start.original.content_type, "image/png");
        assert_eq!(start.current, start.original);
        assert!(!start.can_undo && !start.can_redo);
        assert_eq!(start.suggested_name, "shoe-edited.png");

        editor
            .push("Remove background", &result("image/png", b"one"))
            .unwrap();
        let first = editor.current_path().to_path_buf();
        assert!(first.starts_with(editor.dir()));
        assert_eq!(std::fs::read(&first).unwrap(), b"one");
        assert_eq!(editor.current_size(), Some((10, 5)));

        editor
            .push("Resize / convert", &result("image/webp", b"two"))
            .unwrap();
        let second = editor.current_path().to_path_buf();
        assert_ne!(first, second);
        assert_eq!(second.extension().unwrap(), "webp");
        let snapshot = editor.snapshot();
        assert_eq!(snapshot.cursor, 2);
        assert_eq!(snapshot.steps.len(), 2);
        assert_eq!(snapshot.current.id, 2);
        assert_eq!(snapshot.current.label, "Resize / convert");
        assert_eq!(snapshot.suggested_name, "shoe-edited.webp");
        assert_eq!(editor.path_of(0), Some(original.as_path()));
        assert_eq!(editor.path_of(1), Some(first.as_path()));
        assert_eq!(editor.path_of(99), None);
        // The original was only read.
        assert_eq!(image::image_dimensions(&original).unwrap(), (40, 30));
    }

    #[test]
    fn undo_and_redo_move_through_the_steps() {
        let dir = tempfile::tempdir().unwrap();
        let original = write_png(dir.path(), "shoe.png", 4, 4);
        let mut editor = EditorSession::open(1, &original).unwrap();
        assert!(!editor.undo() && !editor.redo());
        editor.push("A", &result("image/png", b"a")).unwrap();
        editor.push("B", &result("image/png", b"b")).unwrap();
        let (a, b) = (
            editor.path_of(1).unwrap().to_path_buf(),
            editor.path_of(2).unwrap().to_path_buf(),
        );

        assert!(editor.undo());
        assert_eq!(editor.current_path(), a);
        assert!(editor.snapshot().can_redo);
        assert!(editor.undo());
        assert_eq!(editor.current_path(), original);
        assert!(!editor.undo());
        assert!(editor.redo() && editor.redo());
        assert_eq!(editor.current_path(), b);
        assert!(!editor.redo());
        assert!(editor.go_to(1));
        assert_eq!(editor.current_path(), a);
        assert!(!editor.go_to(3));

        // A new tool after an undo drops what was undone, files included.
        editor.push("C", &result("image/jpeg", b"c")).unwrap();
        let snapshot = editor.snapshot();
        assert_eq!(
            snapshot
                .steps
                .iter()
                .map(|s| s.label.as_str())
                .collect::<Vec<_>>(),
            ["A", "C"]
        );
        assert!(!snapshot.can_redo);
        assert!(!b.exists());
        assert!(a.exists());
        assert_eq!(editor.path_of(2), None);
        assert_eq!(snapshot.current.id, 3);
    }

    #[test]
    fn closing_removes_the_steps() {
        let dir = tempfile::tempdir().unwrap();
        let original = write_png(dir.path(), "shoe.png", 4, 4);
        let mut editor = EditorSession::open(1, &original).unwrap();
        editor.push("A", &result("image/png", b"a")).unwrap();
        let steps = editor.dir().to_path_buf();
        let step = editor.current_path().to_path_buf();
        assert!(step.exists());
        drop(editor);
        assert!(!step.exists());
        assert!(!steps.exists());
        assert!(original.exists());
    }

    #[test]
    fn save_as_writes_the_current_step() {
        let dir = tempfile::tempdir().unwrap();
        let original = write_png(dir.path(), "shoe.png", 4, 4);
        let mut editor = EditorSession::open(1, &original).unwrap();
        editor.push("A", &result("image/png", b"a")).unwrap();
        editor.push("B", &result("image/jpeg", b"b")).unwrap();

        let saved = editor.save_as(&dir.path().join("out.jpg"), false).unwrap();
        assert_eq!(saved, dir.path().join("out.jpg"));
        assert_eq!(std::fs::read(&saved).unwrap(), b"b");
        // .jpeg is the same type as .jpg.
        let saved = editor.save_as(&dir.path().join("out.JPEG"), false).unwrap();
        assert_eq!(saved, dir.path().join("out.JPEG"));

        // After undo, the earlier step is what gets saved.
        editor.undo();
        let saved = editor
            .save_as(&dir.path().join("first.png"), false)
            .unwrap();
        assert_eq!(std::fs::read(&saved).unwrap(), b"a");
    }

    #[test]
    fn save_as_gives_the_name_the_pictures_real_extension() {
        let dir = tempfile::tempdir().unwrap();
        let original = write_png(dir.path(), "shoe.png", 4, 4);
        let mut editor = EditorSession::open(1, &original).unwrap();
        editor.push("A", &result("image/webp", b"w")).unwrap();

        let saved = editor.save_as(&dir.path().join("out.png"), false).unwrap();
        assert_eq!(saved, dir.path().join("out.png.webp"));
        let saved = editor.save_as(&dir.path().join("plain"), false).unwrap();
        assert_eq!(saved, dir.path().join("plain.webp"));
        // The changed name is not allowed to land on an existing file.
        let again = editor.save_as(&dir.path().join("plain"), false);
        assert!(matches!(again, Err(EditorError::Exists(_))));
        assert_eq!(std::fs::read(dir.path().join("plain.webp")).unwrap(), b"w");
    }

    #[test]
    fn the_original_is_replaced_only_when_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let original = write_png(dir.path(), "shoe.png", 4, 4);
        let before = std::fs::read(&original).unwrap();
        let mut editor = EditorSession::open(1, &original).unwrap();

        // With no step, saving over the original changes nothing.
        assert_eq!(editor.save_as(&original, false).unwrap(), original);
        assert_eq!(std::fs::read(&original).unwrap(), before);

        editor.push("A", &result("image/png", b"edited")).unwrap();
        let refused = editor.save_as(&original, false).unwrap_err();
        assert!(matches!(refused, EditorError::WouldReplaceOriginal));
        assert_eq!(refused.code(), "confirm_replace_original");
        assert_eq!(std::fs::read(&original).unwrap(), before);
        // The same file by another spelling is still the original.
        let other_spelling = dir.path().join(".").join("shoe.png");
        assert!(matches!(
            editor.save_as(&other_spelling, false),
            Err(EditorError::WouldReplaceOriginal)
        ));

        assert_eq!(editor.save_as(&original, true).unwrap(), original);
        assert_eq!(std::fs::read(&original).unwrap(), b"edited");
    }
}
