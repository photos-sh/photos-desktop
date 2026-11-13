//! What the API returns. Unknown fields are kept (`extra`): the API will grow.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `GET /v1/account`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Account {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Credits left; absent for unlimited keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credits: Option<i64>,
    /// Credits per image by operation (`remove-background`, `resize`, ...).
    #[serde(default)]
    pub prices: BTreeMap<String, u32>,
    #[serde(default)]
    pub unlimited: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// An item of `GET /v1/presets`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// An item of `GET /v1/marketplaces`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Marketplace {
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// An image response: the encoded image (an SVG file from `vectorize`) and
/// what its headers say.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageResult {
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Lossy formats only.
    pub quality: Option<u32>,
    pub server_timing: Option<String>,
    /// `remove-text`: lines painted out, and lines left (on the product).
    pub text_removed: Option<u32>,
    pub text_kept: Option<u32>,
    /// `fix`: whether the result passes, and the codes of what is still found.
    pub marketplace_passed: Option<bool>,
    pub marketplace_findings: Option<Vec<String>>,
    /// `blur-faces`: faces hidden. `restore-faces`: faces restored.
    pub faces: Option<u32>,
    /// `restore-faces` only: faces left alone, being large and detailed enough.
    pub faces_skipped: Option<u32>,
    /// `crop`: false when nothing was found to crop around; the crop is
    /// then centred.
    pub subject_found: Option<bool>,
    /// `vectorize`: how many paths the SVG has.
    pub paths: Option<u32>,
}

impl ImageResult {
    /// The file extension for the result's content type.
    pub fn extension(&self) -> &'static str {
        extension_for(&self.content_type)
    }

    /// A short remark for the UI about what the operation reported.
    pub fn note(&self) -> Option<String> {
        if let Some(removed) = self.text_removed {
            let kept = self.text_kept.unwrap_or(0);
            return Some(match (removed, kept) {
                (0, 0) => "No text found".to_string(),
                (r, 0) => format!("{r} text {} removed", plural(r, "line", "lines")),
                (r, k) => format!(
                    "{r} text {} removed, {k} kept on the product",
                    plural(r, "line", "lines")
                ),
            });
        }
        if let Some(passed) = self.marketplace_passed {
            let findings = self.marketplace_findings.clone().unwrap_or_default();
            return Some(match (passed, findings.is_empty()) {
                (true, true) => "Passes the marketplace's rules".to_string(),
                (true, false) => format!("Passes, with remarks: {}", findings.join(", ")),
                (false, true) => "Does not pass the marketplace's rules".to_string(),
                (false, false) => format!("Does not pass: {}", findings.join(", ")),
            });
        }
        if let Some(faces) = self.faces {
            let found = match faces {
                0 => "No faces".to_string(),
                n => format!("{n} {}", plural(n, "face", "faces")),
            };
            return Some(match self.faces_skipped {
                // Only restore-faces says how many it left alone.
                Some(0) if faces == 0 => "No faces found".to_string(),
                Some(0) => format!("{found} restored"),
                Some(skipped) => {
                    format!("{found} restored, {skipped} left alone (already large and detailed)")
                }
                None if faces == 0 => "No faces found: nothing was hidden".to_string(),
                None => format!("{found} hidden"),
            });
        }
        if self.subject_found == Some(false) {
            return Some("No subject or face found: the crop is centred".to_string());
        }
        if let Some(paths) = self.paths {
            return Some(format!("{paths} {}", plural(paths, "path", "paths")));
        }
        None
    }
}

fn plural<'a>(n: u32, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// The file extension for an image content type (`bin` when unknown).
pub fn extension_for(content_type: &str) -> &'static str {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match essence.as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/avif" => "avif",
        "image/gif" => "gif",
        "image/tiff" => "tiff",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_follow_the_content_type() {
        assert_eq!(extension_for("image/png"), "png");
        assert_eq!(extension_for("image/jpeg"), "jpg");
        assert_eq!(extension_for("IMAGE/WEBP; charset=binary"), "webp");
        assert_eq!(extension_for("image/avif"), "avif");
        assert_eq!(extension_for("image/svg+xml"), "svg");
        assert_eq!(extension_for("image/svg+xml; charset=utf-8"), "svg");
        assert_eq!(extension_for("application/json"), "bin");
    }

    #[test]
    fn an_unlimited_account_parses_and_keeps_unknown_fields() {
        let a: Account =
            serde_json::from_str(r#"{"account":"sdk","unlimited":true,"plan":"x"}"#).unwrap();
        assert!(a.unlimited);
        assert_eq!(a.credits, None);
        assert_eq!(a.extra["plan"], "x");
        let back = serde_json::to_value(&a).unwrap();
        assert_eq!(back["plan"], "x");
    }

    #[test]
    fn notes_describe_text_and_marketplace_results() {
        let mut r = ImageResult {
            content_type: "image/png".into(),
            text_removed: Some(1),
            text_kept: Some(2),
            ..ImageResult::default()
        };
        assert_eq!(
            r.note().unwrap(),
            "1 text line removed, 2 kept on the product"
        );
        r.text_removed = None;
        r.marketplace_passed = Some(false);
        r.marketplace_findings = Some(vec!["too_small".into()]);
        assert_eq!(r.note().unwrap(), "Does not pass: too_small");
        r.marketplace_passed = None;
        assert_eq!(r.note(), None);
    }

    fn noted(change: impl FnOnce(&mut ImageResult)) -> Option<String> {
        let mut r = ImageResult {
            content_type: "image/png".into(),
            ..ImageResult::default()
        };
        change(&mut r);
        r.note()
    }

    #[test]
    fn notes_say_how_many_faces_were_hidden() {
        assert_eq!(noted(|r| r.faces = Some(3)).unwrap(), "3 faces hidden");
        assert_eq!(noted(|r| r.faces = Some(1)).unwrap(), "1 face hidden");
        // The one to notice: the photo came back as it was.
        assert_eq!(
            noted(|r| r.faces = Some(0)).unwrap(),
            "No faces found: nothing was hidden"
        );
    }

    #[test]
    fn notes_say_how_many_faces_were_restored_and_left_alone() {
        let restored = |faces, skipped| {
            noted(|r| {
                r.faces = Some(faces);
                r.faces_skipped = Some(skipped);
            })
            .unwrap()
        };
        assert_eq!(restored(2, 0), "2 faces restored");
        assert_eq!(
            restored(1, 2),
            "1 face restored, 2 left alone (already large and detailed)"
        );
        assert_eq!(
            restored(0, 1),
            "No faces restored, 1 left alone (already large and detailed)"
        );
        assert_eq!(restored(0, 0), "No faces found");
    }

    #[test]
    fn notes_say_when_a_crop_found_no_subject_and_how_many_paths_an_svg_has() {
        assert_eq!(
            noted(|r| r.subject_found = Some(false)).unwrap(),
            "No subject or face found: the crop is centred"
        );
        // A subject that was found is not worth a remark.
        assert_eq!(noted(|r| r.subject_found = Some(true)), None);
        assert_eq!(noted(|r| r.paths = Some(2435)).unwrap(), "2435 paths");
        assert_eq!(noted(|r| r.paths = Some(1)).unwrap(), "1 path");
    }
}
