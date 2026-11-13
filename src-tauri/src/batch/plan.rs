//! Planning a batch: which files to take, where results go and under what
//! names, and what it will cost.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use walkdir::WalkDir;

use crate::api::Account;

/// File extensions taken as images (lower case).
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "tif", "tiff", "bmp", "heic", "heif", "avif",
];

/// The folder results go to by default, beside the first source.
pub const OUTPUT_DIR_NAME: &str = "photos-sh";

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|n| n.starts_with('.'))
}

/// The images among `paths`: files as given, folders walked (sorted by
/// name). Hidden files and folders inside a dropped folder are skipped, and
/// so are `photos-sh` folders: those hold earlier results, not sources.
pub fn collect_images(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for path in paths {
        if path.is_dir() {
            let walker = WalkDir::new(path)
                .follow_links(false)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|e| {
                    e.depth() == 0
                        || !(is_hidden(e.file_name())
                            || e.file_type().is_dir() && e.file_name() == OUTPUT_DIR_NAME)
                });
            found.extend(
                walker
                    .filter_map(Result::ok)
                    .filter(|e| e.file_type().is_file() && is_image(e.path()))
                    .map(walkdir::DirEntry::into_path),
            );
        } else if path.is_file() && is_image(path) {
            found.push(path.clone());
        }
    }
    let mut seen = HashSet::new();
    found.retain(|p| seen.insert(p.clone()));
    found
}

/// `photos-sh` beside the first source.
pub fn default_output_dir(first_source: &Path) -> PathBuf {
    first_source
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join(OUTPUT_DIR_NAME)
}

/// The output's name without its extension: the source's.
pub fn output_stem(source: &Path) -> String {
    source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "image".to_string())
}

/// Writes `bytes` to a new file `stem.extension` in `dir`, or
/// `stem-2.extension`, `stem-3.extension`, ... when that name is taken.
///
/// The file is created with "create new", so nothing that exists is ever
/// overwritten, also when several files are written at once.
pub fn write_new(dir: &Path, stem: &str, extension: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    for n in 1..=100_000u32 {
        let name = if n == 1 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem}-{n}.{extension}")
        };
        let path = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let written = file.write_all(bytes).and_then(|()| file.flush());
                if let Err(e) = written {
                    drop(file);
                    // Leave no half-written result behind.
                    let _ = std::fs::remove_file(&path);
                    return Err(e);
                }
                return Ok(path);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("no free name for {stem}.{extension}"),
    ))
}

/// What a batch will cost, and whether the account can pay for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Estimate {
    pub files: u64,
    /// The key is not metered: nothing is counted.
    pub unlimited: bool,
    /// Credits per file.
    pub price: u32,
    /// Credits for the whole batch.
    pub cost: u64,
    /// Credits left; unknown when the API did not say.
    pub credits: Option<i64>,
    /// False when the balance is known and too low.
    pub enough: bool,
}

/// The cost of running `price_key` on `files` files with this account.
///
/// An operation missing from the price list is free: that is how the engine
/// charges.
pub fn estimate(account: &Account, price_key: &str, files: usize) -> Estimate {
    let files = files as u64;
    let price = account.prices.get(price_key).copied().unwrap_or(0);
    let cost = u64::from(price).saturating_mul(files);
    let credits = if account.unlimited {
        None
    } else {
        account.credits
    };
    let enough = match credits {
        Some(left) => cost == 0 || u64::try_from(left).is_ok_and(|left| left >= cost),
        None => true,
    };
    Estimate {
        files,
        unlimited: account.unlimited,
        price,
        cost,
        credits,
        enough,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::write_fake;

    fn names(paths: &[PathBuf], root: &Path) -> Vec<String> {
        paths
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn folders_are_walked_and_only_images_are_taken() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in [
            "b.PNG",
            "a.jpg",
            "notes.txt",
            "sub/c.webp",
            "sub/deep/d.HEIC",
            "sub/e.avif",
            "sub/f.pdf",
            ".hidden/g.jpg",
            ".h.jpg",
            "photos-sh/a.png",
            "sub/photos-sh/c.png",
            "t.tiff",
            "u.tif",
            "v.gif",
            "w.bmp",
            "x.heif",
            "y.jpeg",
        ] {
            write_fake(root, name);
        }
        let found = collect_images(&[root.to_path_buf()]);
        assert_eq!(
            names(&found, root),
            [
                "a.jpg",
                "b.PNG",
                "sub/c.webp",
                "sub/deep/d.HEIC",
                "sub/e.avif",
                "t.tiff",
                "u.tif",
                "v.gif",
                "w.bmp",
                "x.heif",
                "y.jpeg"
            ]
        );
    }

    #[test]
    fn files_are_taken_as_given_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let a = write_fake(root, "a.jpg");
        let text = write_fake(root, "readme.md");
        // A file given directly counts even in a photos-sh or hidden folder.
        let earlier = write_fake(root, "photos-sh/old.png");
        let found = collect_images(&[
            a.clone(),
            text,
            earlier.clone(),
            root.to_path_buf(),
            a.clone(),
            root.join("missing.jpg"),
        ]);
        assert_eq!(found, [a, earlier]);
        // A dropped photos-sh folder itself is walked.
        assert_eq!(collect_images(&[root.join("photos-sh")]).len(), 1);
    }

    #[test]
    fn the_default_output_folder_is_beside_the_first_source() {
        assert_eq!(
            default_output_dir(Path::new("/shop/in/a.jpg")),
            Path::new("/shop/in/photos-sh")
        );
        assert_eq!(
            default_output_dir(Path::new("a.jpg")),
            Path::new("./photos-sh")
        );
    }

    #[test]
    fn names_come_from_the_source() {
        assert_eq!(output_stem(Path::new("/x/shoe.final.JPG")), "shoe.final");
        assert_eq!(output_stem(Path::new("/x/shoe")), "shoe");
    }

    #[test]
    fn an_existing_file_gets_a_numbered_name_and_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let first = write_new(&out, "shoe", "png", b"one").unwrap();
        let second = write_new(&out, "shoe", "png", b"two").unwrap();
        let third = write_new(&out, "shoe", "png", b"three").unwrap();
        let other = write_new(&out, "shoe", "jpg", b"jpg").unwrap();
        assert_eq!(first, out.join("shoe.png"));
        assert_eq!(second, out.join("shoe-2.png"));
        assert_eq!(third, out.join("shoe-3.png"));
        assert_eq!(other, out.join("shoe.jpg"));
        assert_eq!(std::fs::read(&first).unwrap(), b"one");
        assert_eq!(std::fs::read(&second).unwrap(), b"two");
        assert_eq!(std::fs::read(&third).unwrap(), b"three");
    }

    #[test]
    fn a_result_in_the_sources_folder_does_not_replace_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_fake(dir.path(), "shoe.jpg");
        let written = write_new(dir.path(), &output_stem(&source), "jpg", b"result").unwrap();
        assert_eq!(written, dir.path().join("shoe-2.jpg"));
        assert_eq!(std::fs::read(&source).unwrap(), b"fake image shoe.jpg");
    }

    #[test]
    fn concurrent_writers_never_share_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().to_path_buf();
        let handles: Vec<_> = (0..16)
            .map(|n| {
                let out = out.clone();
                std::thread::spawn(move || {
                    write_new(&out, "same", "png", format!("{n}").as_bytes()).unwrap()
                })
            })
            .collect();
        let paths: HashSet<PathBuf> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(paths.len(), 16);
        let contents: HashSet<String> = paths
            .iter()
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect();
        assert_eq!(contents.len(), 16);
    }

    fn account(json: &str) -> Account {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_estimate_is_price_times_files() {
        let a = account(r#"{"credits":100,"prices":{"upscale":4,"resize":1}}"#);
        let e = estimate(&a, "upscale", 12);
        assert_eq!((e.price, e.cost, e.credits), (4, 48, Some(100)));
        assert!(e.enough && !e.unlimited);
        // Exactly enough is enough; one more file is not.
        assert!(estimate(&a, "upscale", 25).enough);
        let short = estimate(&a, "upscale", 26);
        assert_eq!(short.cost, 104);
        assert!(!short.enough);
    }

    #[test]
    fn unlimited_keys_and_unlisted_operations_cost_nothing() {
        let unlimited = account(r#"{"account":"sdk","unlimited":true}"#);
        let e = estimate(&unlimited, "upscale", 1000);
        assert!(e.unlimited && e.enough);
        assert_eq!((e.cost, e.credits), (0, None));

        let a = account(r#"{"credits":0,"prices":{"upscale":4}}"#);
        let free = estimate(&a, "run", 10);
        assert_eq!(free.cost, 0);
        assert!(free.enough);
        assert!(!estimate(&a, "upscale", 1).enough);
        // A balance the API did not give cannot refuse a batch.
        let unknown = account(r#"{"prices":{"upscale":4}}"#);
        assert!(estimate(&unknown, "upscale", 5).enough);
        // A negative balance pays for nothing that costs.
        let negative = account(r#"{"credits":-3,"prices":{"upscale":4}}"#);
        assert!(!estimate(&negative, "upscale", 1).enough);
    }
}
