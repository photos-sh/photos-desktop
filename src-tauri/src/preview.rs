//! Pictures for the web view: a file as the web view can show it, or a small
//! thumbnail of it. They are served by the `preview` protocol (see `lib.rs`),
//! never sent through IPC.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    #[error("no preview for this format")]
    Unsupported,
    #[error("the file is too large to preview")]
    TooLarge,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Thumbnails fit in a square of this side.
pub const THUMB_SIDE: u32 = 160;
/// Files above this are not handed to the web view whole.
const MAX_FULL_BYTES: u64 = 200 * 1024 * 1024;
/// An SVG above this gets no thumbnail: the web view would draw every path
/// of it for a picture the size of a stamp.
const MAX_SVG_THUMB_BYTES: u64 = 2 * 1024 * 1024;

/// What a file is, by its first bytes (extensions lie).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Jpeg,
    Png,
    Gif,
    Webp,
    Bmp,
    Tiff,
    Avif,
    Heic,
    /// What `vectorize` returns.
    Svg,
    Unknown,
}

impl Kind {
    pub fn sniff(head: &[u8]) -> Self {
        if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Self::Jpeg
        } else if head.starts_with(b"\x89PNG\r\n\x1a\n") {
            Self::Png
        } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
            Self::Gif
        } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
            Self::Webp
        } else if head.starts_with(b"BM") {
            Self::Bmp
        } else if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
            Self::Tiff
        } else if head.len() >= 12 && &head[4..8] == b"ftyp" {
            match &head[8..12] {
                b"avif" | b"avis" => Self::Avif,
                b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis" | b"mif1" | b"msf1" => {
                    Self::Heic
                }
                _ => Self::Unknown,
            }
        } else if is_svg_start(head) {
            Self::Svg
        } else {
            Self::Unknown
        }
    }

    /// The content type, for the kinds every supported web view shows.
    fn web_mime(self) -> Option<&'static str> {
        match self {
            Self::Jpeg => Some("image/jpeg"),
            Self::Png => Some("image/png"),
            Self::Gif => Some("image/gif"),
            Self::Webp => Some("image/webp"),
            Self::Bmp => Some("image/bmp"),
            Self::Avif => Some("image/avif"),
            Self::Svg => Some("image/svg+xml"),
            Self::Tiff | Self::Heic | Self::Unknown => None,
        }
    }
}

/// Whether a file begins like an SVG: its root element, or the XML
/// declaration before it (a byte order mark and blank space aside).
fn is_svg_start(head: &[u8]) -> bool {
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    let head = head.trim_ascii_start();
    head.starts_with(b"<svg") || head.starts_with(b"<?xml")
}

fn sniff_file(path: &Path) -> std::io::Result<Kind> {
    use std::io::Read;
    let mut head = [0u8; 16];
    let mut file = std::fs::File::open(path)?;
    let n = file.read(&mut head)?;
    Ok(Kind::sniff(&head[..n]))
}

/// The file as the web view can show it: as it is for the formats web views
/// know (an SVG among them), converted to PNG for TIFF. HEIC cannot be shown.
pub fn full(path: &Path) -> Result<Picture, PreviewError> {
    let kind = sniff_file(path)?;
    if let Some(mime) = kind.web_mime() {
        if std::fs::metadata(path)?.len() > MAX_FULL_BYTES {
            return Err(PreviewError::TooLarge);
        }
        return Ok(Picture {
            bytes: std::fs::read(path)?,
            mime,
        });
    }
    if kind == Kind::Tiff {
        let image = decode(path)?;
        return encode(&image, ImageFormat::Png);
    }
    Err(PreviewError::Unsupported)
}

/// A thumbnail: PNG when the picture has transparency, else JPEG.
pub fn thumbnail(path: &Path) -> Result<Picture, PreviewError> {
    let kind = sniff_file(path)?;
    if matches!(kind, Kind::Heic | Kind::Avif | Kind::Svg | Kind::Unknown) {
        // Not decoded here; an AVIF or an SVG the web view can scale down
        // itself.
        return match kind {
            Kind::Avif => full(path),
            Kind::Svg if std::fs::metadata(path)?.len() > MAX_SVG_THUMB_BYTES => {
                Err(PreviewError::TooLarge)
            }
            Kind::Svg => full(path),
            _ => Err(PreviewError::Unsupported),
        };
    }
    let image = decode(path)?.thumbnail(THUMB_SIDE, THUMB_SIDE);
    if image.color().has_alpha() {
        encode(&image, ImageFormat::Png)
    } else {
        encode(&DynamicImage::ImageRgb8(image.to_rgb8()), ImageFormat::Jpeg)
    }
}

/// The picture's size in pixels as a viewer shows it (EXIF rotation
/// applied), when the file can be read here.
pub fn dimensions(path: &Path) -> Option<(u32, u32)> {
    let mut decoder = ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    let (width, height) = decoder.dimensions();
    let turned = decoder.orientation().is_ok_and(|o| {
        use image::metadata::Orientation::{Rotate90, Rotate90FlipH, Rotate270, Rotate270FlipH};
        matches!(o, Rotate90 | Rotate270 | Rotate90FlipH | Rotate270FlipH)
    });
    Some(if turned {
        (height, width)
    } else {
        (width, height)
    })
}

fn decode(path: &Path) -> Result<DynamicImage, PreviewError> {
    let mut decoder = ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()
        .map_err(|_| PreviewError::Unsupported)?;
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| PreviewError::Unsupported)?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    Ok(image)
}

fn encode(image: &DynamicImage, format: ImageFormat) -> Result<Picture, PreviewError> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, format)
        .map_err(|_| PreviewError::Unsupported)?;
    Ok(Picture {
        bytes: out.into_inner(),
        mime: if format == ImageFormat::Png {
            "image/png"
        } else {
            "image/jpeg"
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::write_png;

    #[test]
    fn kinds_are_told_by_content() {
        assert_eq!(Kind::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Kind::Jpeg);
        assert_eq!(Kind::sniff(b"\x89PNG\r\n\x1a\n...."), Kind::Png);
        assert_eq!(Kind::sniff(b"GIF89a......"), Kind::Gif);
        assert_eq!(Kind::sniff(b"RIFF\0\0\0\0WEBPVP8 "), Kind::Webp);
        assert_eq!(Kind::sniff(b"II*\0........"), Kind::Tiff);
        assert_eq!(Kind::sniff(b"\0\0\0\x20ftypavif...."), Kind::Avif);
        assert_eq!(Kind::sniff(b"\0\0\0\x20ftypheic...."), Kind::Heic);
        assert_eq!(Kind::sniff(b"hello"), Kind::Unknown);
        assert_eq!(Kind::sniff(b""), Kind::Unknown);
        assert_eq!(Kind::sniff(b"<svg xmlns=\"http"), Kind::Svg);
        assert_eq!(Kind::sniff(b"<?xml version=\"1"), Kind::Svg);
        assert_eq!(Kind::sniff(b"\xEF\xBB\xBF\n <svg wid"), Kind::Svg);
        assert_eq!(Kind::sniff(b"<html><svg>"), Kind::Unknown);
    }

    #[test]
    fn an_svg_is_served_as_it_is_and_is_its_own_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        // What vectorize returns, whatever the file is called.
        let svg = br#"<?xml version="1.0" encoding="UTF-8"?>
<svg version="1.1" xmlns="http://www.w3.org/2000/svg" width="8" height="6"><path d="M0 0h8v6H0z"/></svg>"#;
        let path = dir.path().join("logo.svg");
        std::fs::write(&path, svg).unwrap();
        let picture = full(&path).unwrap();
        assert_eq!(picture.mime, "image/svg+xml");
        assert_eq!(picture.bytes, svg);
        assert_eq!(thumbnail(&path).unwrap(), picture);

        // A photograph traced into megabytes of paths gets no thumbnail.
        let mut large = svg.to_vec();
        large.resize(MAX_SVG_THUMB_BYTES as usize + 1, b' ');
        let path = dir.path().join("photo.svg");
        std::fs::write(&path, large).unwrap();
        assert!(matches!(thumbnail(&path), Err(PreviewError::TooLarge)));
        assert_eq!(full(&path).unwrap().mime, "image/svg+xml");
    }

    #[test]
    fn a_web_format_is_served_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        // The name does not matter: the content does.
        let path = write_png(dir.path(), "photo.jpg", 30, 20);
        let picture = full(&path).unwrap();
        assert_eq!(picture.mime, "image/png");
        assert_eq!(picture.bytes, std::fs::read(&path).unwrap());
        assert_eq!(dimensions(&path), Some((30, 20)));
    }

    #[test]
    fn a_tiff_is_converted_for_the_web_view() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scan.tiff");
        image::RgbImage::from_pixel(24, 12, image::Rgb([200, 10, 10]))
            .save_with_format(&path, ImageFormat::Tiff)
            .unwrap();
        let picture = full(&path).unwrap();
        assert_eq!(picture.mime, "image/png");
        let decoded = image::load_from_memory(&picture.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (24, 12));
        assert_eq!(thumbnail(&path).unwrap().mime, "image/jpeg");
    }

    #[test]
    fn thumbnails_are_small_and_keep_transparency() {
        let dir = tempfile::tempdir().unwrap();
        let opaque = write_png(dir.path(), "big.png", 800, 400);
        let thumb = thumbnail(&opaque).unwrap();
        assert_eq!(thumb.mime, "image/jpeg");
        let decoded = image::load_from_memory(&thumb.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (160, 80));

        let cutout = dir.path().join("cutout.png");
        image::RgbaImage::from_pixel(300, 300, image::Rgba([0, 0, 0, 0]))
            .save(&cutout)
            .unwrap();
        let thumb = thumbnail(&cutout).unwrap();
        assert_eq!(thumb.mime, "image/png");
        let decoded = image::load_from_memory(&thumb.bytes).unwrap();
        assert_eq!(decoded.to_rgba8().get_pixel(5, 5).0[3], 0);
    }

    #[test]
    fn what_cannot_be_shown_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let heic = dir.path().join("phone.heic");
        std::fs::write(&heic, b"\0\0\0\x20ftypheic\0\0\0\0mif1").unwrap();
        assert!(matches!(full(&heic), Err(PreviewError::Unsupported)));
        assert!(matches!(thumbnail(&heic), Err(PreviewError::Unsupported)));
        assert_eq!(dimensions(&heic), None);
        let text = dir.path().join("notes.png");
        std::fs::write(&text, b"not an image").unwrap();
        assert!(matches!(full(&text), Err(PreviewError::Unsupported)));
        assert!(matches!(
            full(&dir.path().join("missing.png")),
            Err(PreviewError::Io(_))
        ));
    }
}
