//! A recipe: several steps on one upload, sent to `POST /v1/run` as a TOML
//! `recipe` part. One request, charged as one, however many steps it has.
//!
//! The service runs the steps in a fixed order, whatever the order here:
//! read barcodes and text (on the photo as it came) → remove text →
//! upscale → remove background → frame or crop → shadow → background →
//! output. The rules it would refuse a recipe for are checked here first,
//! in the app's words.

use serde::{Deserialize, Serialize};

use crate::ops::{Format, InvalidOperation, Model, Shadow, TextRegion};

/// What the UI sends: each step is on when it is set.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Recipe {
    /// Read the barcodes.
    pub barcodes: bool,
    /// Every barcode format, not only retail codes (EAN, UPC, ITF).
    pub all_formats: bool,
    /// Read the text, line by line.
    pub text: bool,
    pub remove_text: Option<TextRegion>,
    /// Upscale a photo whose longest side is under this many pixels.
    pub upscale_to: Option<u32>,
    pub remove_background: bool,
    /// The model that finds the subject; the service's default (`atlas`)
    /// when left out.
    pub model: Option<Model>,
    /// Also keep what stands out from a plain backdrop (a box, a lid).
    pub studio: bool,
    pub layout: Option<Layout>,
    pub shadow: Option<Shadow>,
    pub background: Option<Backdrop>,
    pub format: Option<Format>,
    pub quality: Option<u32>,
    /// The longest side of the result, in pixels.
    pub max_side: Option<u32>,
    pub max_bytes: Option<u64>,
}

/// Where the cutout goes.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Layout {
    /// A canvas of this size, the subject filling `fill` percent of it.
    Frame {
        width: u32,
        height: u32,
        #[serde(default)]
        fill: Option<f32>,
        /// Stand the subject on the bottom edge rather than centre it.
        #[serde(default)]
        bottom: bool,
    },
    /// Cropped to the subject, with `padding` percent of its longest side
    /// around it.
    Crop {
        #[serde(default)]
        padding: Option<f32>,
    },
}

/// What goes behind the cutout.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Backdrop {
    Color {
        color: String,
    },
    /// From the top to the bottom.
    Gradient {
        from: String,
        to: String,
    },
    /// The photo's own background, blurred.
    Blur,
}

/// The largest canvas or upscale size the app sends, in pixels.
const MAX_SIDE: u32 = 10_000;

/// How much a blurred background is blurred: the service's `social` preset.
const BLUR: f32 = 0.03;

fn invalid<T>(message: &str) -> Result<T, InvalidOperation> {
    Err(InvalidOperation(message.to_string()))
}

fn in_range(value: Option<f32>, min: f32, max: f32) -> bool {
    value.is_none_or(|v| (min..=max).contains(&v))
}

fn colour(c: &str, empty: &str) -> Result<(), InvalidOperation> {
    if c.trim().is_empty() {
        return invalid(empty);
    }
    Ok(())
}

/// A TOML basic string.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.trim().chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl Recipe {
    /// Whether it edits the picture, not only reads it.
    fn edits(&self) -> bool {
        self.remove_text.is_some()
            || self.upscale_to.is_some()
            || self.remove_background
            || self.layout.is_some()
            || self.shadow().is_some()
            || self.background.is_some()
            || self.format.is_some()
            || self.quality.is_some()
            || self.max_side.is_some()
            || self.max_bytes.is_some()
    }

    fn shadow(&self) -> Option<Shadow> {
        self.shadow.filter(|s| *s != Shadow::None)
    }

    pub fn validate(&self) -> Result<(), InvalidOperation> {
        if !self.edits() && !self.barcodes && !self.text {
            return invalid("Choose at least one step.");
        }
        let staged = self.layout.is_some() || self.shadow().is_some() || self.background.is_some();
        if staged && !self.remove_background {
            return invalid(
                "Framing, cropping, a shadow and a background need \"Remove background\".",
            );
        }
        if let Some(size) = self.upscale_to
            && !(16..=MAX_SIDE).contains(&size)
        {
            return invalid("Upscale to between 16 and 10,000 pixels.");
        }
        match &self.layout {
            Some(Layout::Frame {
                width,
                height,
                fill,
                ..
            }) => {
                if !(1..=MAX_SIDE).contains(width) || !(1..=MAX_SIDE).contains(height) {
                    return invalid("The canvas goes from 1 to 10,000 pixels a side.");
                }
                if !in_range(*fill, 5.0, 100.0) {
                    return invalid("The product fills between 5 and 100 percent of the canvas.");
                }
            }
            Some(Layout::Crop { padding }) if !in_range(*padding, 0.0, 100.0) => {
                return invalid("The room around the product goes from 0 to 100 percent.");
            }
            Some(Layout::Crop { .. }) | None => {}
        }
        match &self.background {
            Some(Backdrop::Color { color }) => colour(color, "The background colour is empty.")?,
            Some(Backdrop::Gradient { from, to }) => {
                colour(from, "The gradient's top colour is empty.")?;
                colour(to, "The gradient's bottom colour is empty.")?;
            }
            Some(Backdrop::Blur) | None => {}
        }
        if self.remove_background && self.background.is_none() && self.format == Some(Format::Jpg) {
            return invalid("JPEG has no transparency: choose a background, or PNG, WebP or AVIF.");
        }
        if let Some(q) = self.quality
            && !(1..=100).contains(&q)
        {
            return invalid("Quality goes from 1 to 100.");
        }
        if self.max_side == Some(0) {
            return invalid("The longest side must be at least 1 pixel.");
        }
        if self.max_bytes == Some(0) {
            return invalid("The size limit must be at least 1 byte.");
        }
        Ok(())
    }

    /// The recipe as the service takes it.
    pub fn to_toml(&self) -> String {
        let mut t = String::new();
        let mut table = |name: &str, lines: &[String]| {
            t.push_str(&format!("[{name}]\n"));
            for line in lines {
                t.push_str(line);
                t.push('\n');
            }
        };
        if self.barcodes {
            let all = if self.all_formats {
                vec!["all_formats = true".to_string()]
            } else {
                Vec::new()
            };
            table("barcodes", &all);
        }
        if self.text {
            table("text", &[]);
        }
        if let Some(region) = self.remove_text {
            let region = match region {
                TextRegion::Background => "background",
                TextRegion::All => "all",
            };
            table("remove_text", &[format!("region = {}", quote(region))]);
        }
        if let Some(size) = self.upscale_to {
            table("upscale", &[format!("min_side = {size}")]);
        }
        if self.remove_background {
            let mut lines = Vec::new();
            if let Some(model) = self.model {
                lines.push(format!("model = {}", quote(model.as_str())));
            }
            if self.studio {
                lines.push("studio = true".to_string());
            }
            table("remove_background", &lines);
        }
        match &self.layout {
            Some(Layout::Frame {
                width,
                height,
                fill,
                bottom,
            }) => {
                let mut lines = vec![format!("width = {width}"), format!("height = {height}")];
                if let Some(fill) = fill {
                    lines.push(format!("fill = {}", fill / 100.0));
                }
                if *bottom {
                    lines.push("align = \"bottom\"".to_string());
                }
                table("frame", &lines);
            }
            Some(Layout::Crop { padding }) => {
                let lines: Vec<String> = padding
                    .map(|p| format!("padding = {}", p / 100.0))
                    .into_iter()
                    .collect();
                table("crop", &lines);
            }
            None => {}
        }
        if let Some(shadow) = self.shadow() {
            table("shadow", &[format!("kind = {}", quote(shadow.as_str()))]);
        }
        match &self.background {
            Some(Backdrop::Color { color }) => {
                table("background", &[format!("color = {}", quote(color))]);
            }
            Some(Backdrop::Gradient { from, to }) => table(
                "background",
                &[format!("gradient = [{}, {}]", quote(from), quote(to))],
            ),
            Some(Backdrop::Blur) => table("background", &[format!("blur_original = {BLUR}")]),
            None => {}
        }
        let mut output = Vec::new();
        if let Some(format) = self.format {
            output.push(format!("format = {}", quote(format.as_str())));
        }
        if let Some(q) = self.quality {
            output.push(format!("quality = {q}"));
        }
        if let Some(side) = self.max_side {
            output.push(format!("max_side = {side}"));
        }
        if let Some(bytes) = self.max_bytes {
            output.push(format!("max_bytes = {bytes}"));
        }
        if !output.is_empty() {
            table("output", &output);
        }
        t
    }

    /// The steps in the order they run, for the step list.
    pub fn summary(&self) -> String {
        let mut steps: Vec<&str> = Vec::new();
        if self.barcodes {
            steps.push("barcodes");
        }
        if self.text {
            steps.push("text");
        }
        if self.remove_text.is_some() {
            steps.push("remove text");
        }
        if self.upscale_to.is_some() {
            steps.push("upscale");
        }
        if self.remove_background {
            steps.push("remove background");
        }
        match self.layout {
            Some(Layout::Frame { .. }) => steps.push("frame"),
            Some(Layout::Crop { .. }) => steps.push("crop"),
            None => {}
        }
        if self.shadow().is_some() {
            steps.push("shadow");
        }
        if self.background.is_some() {
            steps.push("background");
        }
        if self.format.is_some()
            || self.quality.is_some()
            || self.max_side.is_some()
            || self.max_bytes.is_some()
        {
            steps.push("output");
        }
        steps.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(json: &str) -> Recipe {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn every_step_becomes_its_table() {
        let r = recipe(
            r##"{"barcodes": true, "all_formats": true, "text": true, "remove_text": "all",
                "upscale_to": 1000, "remove_background": true, "model": "lumen", "studio": true,
                "layout": {"kind": "frame", "width": 2000, "height": 1600, "fill": 85, "bottom": true},
                "shadow": "contact", "background": {"kind": "gradient", "from": "#ffffff", "to": "#e6e9ed"},
                "format": "webp", "quality": 85, "max_side": 1600, "max_bytes": 300000}"##,
        );
        r.validate().unwrap();
        assert_eq!(
            r.to_toml(),
            "[barcodes]\nall_formats = true\n[text]\n[remove_text]\nregion = \"all\"\n\
             [upscale]\nmin_side = 1000\n[remove_background]\nmodel = \"lumen\"\nstudio = true\n\
             [frame]\nwidth = 2000\nheight = 1600\nfill = 0.85\nalign = \"bottom\"\n\
             [shadow]\nkind = \"contact\"\n\
             [background]\ngradient = [\"#ffffff\", \"#e6e9ed\"]\n\
             [output]\nformat = \"webp\"\nquality = 85\nmax_side = 1600\nmax_bytes = 300000\n"
        );
        assert_eq!(
            r.summary(),
            "barcodes, text, remove text, upscale, remove background, frame, shadow, background, output"
        );
    }

    #[test]
    fn only_what_is_set_is_sent() {
        let r = recipe(
            r##"{"remove_background": true, "layout": {"kind": "crop"}, "shadow": "none"}"##,
        );
        r.validate().unwrap();
        assert_eq!(r.to_toml(), "[remove_background]\n[crop]\n");
        // A model chosen is sent; none chosen leaves the service's default.
        let r = recipe(r##"{"remove_background": true, "model": "muse"}"##);
        r.validate().unwrap();
        assert_eq!(r.to_toml(), "[remove_background]\nmodel = \"muse\"\n");
        let r = recipe(
            r##"{"remove_background": true, "layout": {"kind": "crop", "padding": 4},
                "background": {"kind": "blur"}}"##,
        );
        assert_eq!(
            r.to_toml(),
            "[remove_background]\n[crop]\npadding = 0.04\n[background]\nblur_original = 0.03\n"
        );
        // Reading alone is a recipe too.
        let r = recipe(r##"{"barcodes": true}"##);
        r.validate().unwrap();
        assert_eq!(r.to_toml(), "[barcodes]\n");
    }

    #[test]
    fn colours_are_quoted_safely() {
        let r = recipe(
            r##"{"remove_background": true, "background": {"kind": "color", "color": " white\"\n[x] "}}"##,
        );
        assert_eq!(
            r.to_toml(),
            "[remove_background]\n[background]\ncolor = \"white\\\"\\u000A[x]\"\n"
        );
    }

    #[test]
    fn what_the_service_would_refuse_is_refused_first() {
        let refused = [
            ("{}", "at least one step"),
            (
                r##"{"layout": {"kind": "crop"}}"##,
                "need \"Remove background\"",
            ),
            (r##"{"shadow": "drop"}"##, "need \"Remove background\""),
            (
                r##"{"background": {"kind": "color", "color": "white"}}"##,
                "need \"Remove background\"",
            ),
            (
                r##"{"remove_background": true, "format": "jpg"}"##,
                "JPEG has no transparency",
            ),
            (r##"{"upscale_to": 8}"##, "Upscale to between"),
            (
                r##"{"remove_background": true, "layout": {"kind": "frame", "width": 0, "height": 10}}"##,
                "canvas",
            ),
            (
                r##"{"remove_background": true, "layout": {"kind": "frame", "width": 10, "height": 10, "fill": 2}}"##,
                "fills between",
            ),
            (
                r##"{"remove_background": true, "layout": {"kind": "crop", "padding": 150}}"##,
                "room around",
            ),
            (
                r##"{"remove_background": true, "background": {"kind": "gradient", "from": "#fff", "to": " "}}"##,
                "bottom colour",
            ),
            (r##"{"barcodes": true, "quality": 0}"##, "Quality"),
            (r##"{"max_side": 0}"##, "longest side"),
            (r##"{"max_bytes": 0}"##, "size limit"),
        ];
        for (json, expected) in refused {
            let err = recipe(json).validate().unwrap_err().to_string();
            assert!(err.contains(expected), "{json}: {err}");
        }
        // A shadow of "none" is no shadow, so it needs nothing.
        recipe(r##"{"shadow": "none", "text": true}"##)
            .validate()
            .unwrap();
        // With a background, JPEG is fine.
        recipe(
            r##"{"remove_background": true, "format": "jpg", "background": {"kind": "color", "color": "white"}}"##,
        )
        .validate()
        .unwrap();
    }
}
