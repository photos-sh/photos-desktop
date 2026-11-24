//! The operations the app offers, and how each becomes an API request:
//! an endpoint and a query string. Options that are not set are left out.

use serde::{Deserialize, Serialize};

use crate::api::ExtraPart;
use crate::recipe::Recipe;

/// An output format (`format=` in the query).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Png,
    Jpg,
    Webp,
    Avif,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Webp => "webp",
            Self::Avif => "avif",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    Contain,
    Cover,
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextRegion {
    Background,
    All,
}

/// A shadow under a cutout (`shadow=` of remove background).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Shadow {
    None,
    /// Soft, at the subject's base, as if it stands on a surface.
    Contact,
    /// The silhouette, offset and blurred.
    Drop,
}

impl Shadow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Contact => "contact",
            Self::Drop => "drop",
        }
    }
}

/// The model that finds the subject (`model=` of remove background). Each
/// costs the same; left out, the API uses `atlas`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Model {
    /// Everyday product photos: a product alone or with its box, on any
    /// backdrop. The API's default.
    Atlas,
    /// Pale packaging on a light backdrop, and close-ups where the product
    /// fills the photo.
    Lumen,
    /// People: portraits, and models wearing the product.
    Muse,
}

impl Model {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Atlas => "atlas",
            Self::Lumen => "lumen",
            Self::Muse => "muse",
        }
    }
}

/// How faces are hidden (`mode=` of blur faces).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FaceMode {
    Blur,
    Pixelate,
    /// Covered with a colour: the only one that leaves nothing of the face.
    Fill,
}

impl FaceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blur => "blur",
            Self::Pixelate => "pixelate",
            Self::Fill => "fill",
        }
    }
}

/// What a crop is built around (`focus=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CropFocus {
    Subject,
    Face,
}

impl CropFocus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Subject => "subject",
            Self::Face => "face",
        }
    }
}

/// Where a watermark sits (`position=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Position {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Position {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::Top => "top",
            Self::TopRight => "top-right",
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::BottomLeft => "bottom-left",
            Self::Bottom => "bottom",
            Self::BottomRight => "bottom-right",
        }
    }
}

/// What vectorize traces (`mode=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VectorMode {
    Color,
    /// Whatever is darker than mid grey, as one black shape.
    Mono,
}

impl VectorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Mono => "mono",
        }
    }
}

/// The lines vectorize draws (`curves=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Curves {
    Spline,
    Polygon,
    Pixel,
}

impl Curves {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spline => "spline",
            Self::Polygon => "polygon",
            Self::Pixel => "pixel",
        }
    }
}

/// Which of select's answers to take (`pick=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Pick {
    /// The surest one.
    Auto,
    /// The part clicked.
    Smallest,
    /// The whole thing it belongs to.
    Largest,
}

impl Pick {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Smallest => "smallest",
            Self::Largest => "largest",
        }
    }
}

/// One operation with its options, as the UI sends it.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    RemoveBackground {
        /// A colour to put behind the subject; none for transparency.
        #[serde(default)]
        background: Option<String>,
        #[serde(default)]
        crop: bool,
        #[serde(default)]
        padding: Option<u32>,
        /// A shadow under the subject; none when left out.
        #[serde(default)]
        shadow: Option<Shadow>,
        /// The model that finds the subject; the API's default when left out.
        #[serde(default)]
        model: Option<Model>,
        /// A studio shot (a product on a plain, light backdrop): also keep
        /// what stands out from the backdrop and touches the product, such
        /// as its box or lid.
        #[serde(default)]
        studio: bool,
        /// Moves the cutout's edge, in pixels: inward when negative, outward
        /// when positive (-10 to 10). 0, like none, is the API's default.
        #[serde(default)]
        shift: Option<f32>,
        /// Makes the edge crisper: 0 to 100, about a pixel wide at 100.
        #[serde(default)]
        hardness: Option<f32>,
        /// Irons out ripples in the outline, 0 to 10.
        #[serde(default)]
        smooth: Option<f32>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    Upscale {
        #[serde(default)]
        scale: Option<f32>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    RemoveText {
        #[serde(default)]
        region: Option<TextRegion>,
        #[serde(default)]
        matching: Option<String>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    Resize {
        #[serde(default)]
        width: Option<u32>,
        #[serde(default)]
        height: Option<u32>,
        #[serde(default)]
        fit: Option<Fit>,
        #[serde(default)]
        enlarge: bool,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
        #[serde(default)]
        max_bytes: Option<u64>,
        #[serde(default)]
        background: Option<String>,
    },
    Fix {
        marketplace: String,
    },
    Run {
        preset: String,
    },
    /// Several steps on one upload: `/v1/run` with a recipe of the user's.
    Recipe {
        recipe: Recipe,
    },
    /// Light and colour. Without any setting the API fixes both by itself.
    Enhance {
        /// Keep the automatic fix beside the settings given.
        #[serde(default)]
        auto: bool,
        /// -100 to 100.
        #[serde(default)]
        brightness: Option<f32>,
        #[serde(default)]
        contrast: Option<f32>,
        /// -100 is black and white.
        #[serde(default)]
        saturation: Option<f32>,
        /// 0 to 100.
        #[serde(default)]
        sharpen: Option<f32>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    Denoise {
        /// How much of the cleaned result replaces the photo, 1 to 100.
        #[serde(default)]
        strength: Option<f32>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    Colorize {
        /// How colourful the result is, 1 to 100.
        #[serde(default)]
        strength: Option<f32>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    RestoreFaces {
        /// How much of the restored face replaces the original, 1 to 100.
        #[serde(default)]
        strength: Option<f32>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    BlurFaces {
        #[serde(default)]
        mode: Option<FaceMode>,
        /// How strong the blur or how coarse the mosaic is, 1 to 100.
        #[serde(default)]
        strength: Option<f32>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    Crop {
        /// The shape, as `4:5` or a number like `1.5`.
        aspect: String,
        #[serde(default)]
        focus: Option<CropFocus>,
        /// Crop closely around the subject, instead of as large as possible.
        #[serde(default)]
        zoom: bool,
        /// Room around the subject when cropping closely, as a fraction of
        /// its longest side (0 to 2).
        #[serde(default)]
        padding: Option<f32>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    Expand {
        /// The shape to reach, as `1:1` or a number like `1.5`.
        aspect: String,
        /// A colour for the new area; none continues the photo by inpainting.
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    /// A line of text on the photo.
    Watermark {
        text: String,
        /// The text's colour; white when left out.
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        position: Option<Position>,
        /// The text's width, in percent of the photo's width (1 to 100).
        #[serde(default)]
        scale: Option<f32>,
        /// 1 to 100.
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        format: Option<Format>,
        #[serde(default)]
        quality: Option<u32>,
    },
    /// Traces the picture into an SVG file: the one result that is not pixels.
    Vectorize {
        #[serde(default)]
        mode: Option<VectorMode>,
        #[serde(default)]
        curves: Option<Curves>,
    },
}

/// What is wrong with an operation's options, in plain words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct InvalidOperation(pub String);

fn invalid<T>(message: &str) -> Result<T, InvalidOperation> {
    Err(InvalidOperation(message.to_string()))
}

fn check_quality(quality: Option<u32>) -> Result<(), InvalidOperation> {
    match quality {
        Some(q) if !(1..=100).contains(&q) => invalid("Quality goes from 1 to 100."),
        _ => Ok(()),
    }
}

/// A colour that is given must not be blank; `empty` says which one it is.
fn check_colour(colour: Option<&str>, empty: &str) -> Result<(), InvalidOperation> {
    match colour {
        Some(c) if c.trim().is_empty() => invalid(empty),
        _ => Ok(()),
    }
}

const EMPTY_BACKGROUND: &str = "The background colour is empty.";

/// A setting that is given must lie between `min` and `max`.
fn check_range(
    value: Option<f32>,
    min: f32,
    max: f32,
    message: &str,
) -> Result<(), InvalidOperation> {
    match value {
        // A NaN is in no range, so it is refused too.
        Some(v) if !(min..=max).contains(&v) => invalid(message),
        _ => Ok(()),
    }
}

fn check_strength(strength: Option<f32>) -> Result<(), InvalidOperation> {
    check_range(strength, 1.0, 100.0, "The strength goes from 1 to 100.")
}

/// The edge controls of a cutout (`shift`, `hardness` and `smooth` of remove
/// background, and of `[remove_background]` in a recipe), each in its range.
pub fn check_edge(
    shift: Option<f32>,
    hardness: Option<f32>,
    smooth: Option<f32>,
) -> Result<(), InvalidOperation> {
    check_range(
        shift,
        -10.0,
        10.0,
        "The edge shift goes from -10 to 10 pixels.",
    )?;
    check_range(
        hardness,
        0.0,
        100.0,
        "The edge hardness goes from 0 to 100.",
    )?;
    check_range(smooth, 0.0, 10.0, "The edge smoothing goes from 0 to 10.")
}

/// An edge control as it is sent: not at all at 0, the API's default, where
/// it changes nothing. The request is then what it was before the API had
/// these controls, which an engine from before them still takes (it refuses
/// a parameter it does not know).
pub fn edge(value: Option<f32>) -> Option<f32> {
    value.filter(|v| *v != 0.0)
}

/// A shape as the API takes it: `4:5`, or a number like `1.5`.
fn check_aspect(aspect: &str) -> Result<(), InvalidOperation> {
    let aspect = aspect.trim();
    let whole = |s: &str| s.trim().parse::<u32>().is_ok_and(|n| n > 0);
    let ok = match aspect.split_once(':') {
        Some((width, height)) => whole(width) && whole(height),
        None => aspect
            .parse::<f64>()
            .is_ok_and(|n| n.is_finite() && n > 0.0),
    };
    if ok {
        Ok(())
    } else {
        invalid("Choose a shape, such as 4:5 or 16:9.")
    }
}

/// The longest text a watermark takes, in characters.
pub const MAX_WATERMARK_TEXT: usize = 200;

fn push(query: &mut Vec<(String, String)>, name: &str, value: Option<impl ToString>) {
    if let Some(value) = value {
        query.push((name.to_string(), value.to_string()));
    }
}

fn push_output(query: &mut Vec<(String, String)>, format: Option<Format>, quality: Option<u32>) {
    push(query, "format", format.map(Format::as_str));
    push(query, "quality", quality);
}

impl Operation {
    /// The parts sent beside the image: a recipe's TOML.
    pub fn parts(&self) -> Vec<ExtraPart> {
        match self {
            Self::Recipe { recipe } => vec![ExtraPart {
                name: "recipe",
                file_name: "recipe.toml",
                mime: "application/toml",
                bytes: recipe.to_toml().into_bytes(),
            }],
            _ => Vec::new(),
        }
    }

    /// The endpoint's path under the base URL.
    pub fn endpoint(&self) -> &'static str {
        match self {
            Self::RemoveBackground { .. } => "/v1/remove-background",
            Self::Upscale { .. } => "/v1/upscale",
            Self::RemoveText { .. } => "/v1/remove-text",
            Self::Resize { .. } => "/v1/resize",
            Self::Fix { .. } => "/v1/fix",
            Self::Run { .. } | Self::Recipe { .. } => "/v1/run",
            Self::Enhance { .. } => "/v1/enhance",
            Self::Denoise { .. } => "/v1/denoise",
            Self::Colorize { .. } => "/v1/colorize",
            Self::RestoreFaces { .. } => "/v1/restore-faces",
            Self::BlurFaces { .. } => "/v1/blur-faces",
            Self::Crop { .. } => "/v1/crop",
            Self::Expand { .. } => "/v1/expand",
            Self::Watermark { .. } => "/v1/watermark",
            Self::Vectorize { .. } => "/v1/vectorize",
        }
    }

    /// The operation's name in the account's price list: the endpoint's last
    /// path segment, which is what the engine charges by.
    pub fn price_key(&self) -> &'static str {
        self.endpoint().trim_start_matches("/v1/")
    }

    /// Whether the result is a picture in pixels, which the editor can show
    /// and hand to the next tool. Vectorize answers with an SVG file.
    pub fn gives_pixels(&self) -> bool {
        !matches!(self, Self::Vectorize { .. })
    }

    /// A short name for the step list and messages.
    pub fn label(&self) -> String {
        match self {
            Self::RemoveBackground { .. } => "Remove background".to_string(),
            Self::Upscale { scale, .. } => match scale {
                Some(s) => format!("Upscale {s}×"),
                None => "Upscale".to_string(),
            },
            Self::RemoveText { .. } => "Remove text".to_string(),
            Self::Resize { .. } => "Resize / convert".to_string(),
            Self::Fix { marketplace } => format!("Fix for {marketplace}"),
            Self::Run { preset } => format!("Preset: {preset}"),
            Self::Recipe { recipe } => format!("Recipe: {}", recipe.summary()),
            Self::Enhance { .. } => "Enhance".to_string(),
            Self::Denoise { .. } => "Denoise".to_string(),
            Self::Colorize { .. } => "Colourise".to_string(),
            Self::RestoreFaces { .. } => "Restore faces".to_string(),
            Self::BlurFaces { .. } => "Hide faces".to_string(),
            Self::Crop { aspect, .. } => format!("Crop to {}", aspect.trim()),
            Self::Expand { aspect, .. } => format!("Expand to {}", aspect.trim()),
            Self::Watermark { .. } => "Watermark".to_string(),
            Self::Vectorize { .. } => "Vectorize".to_string(),
        }
    }

    pub fn validate(&self) -> Result<(), InvalidOperation> {
        match self {
            Self::RemoveBackground {
                background,
                shift,
                hardness,
                smooth,
                quality,
                ..
            } => {
                check_colour(background.as_deref(), EMPTY_BACKGROUND)?;
                check_edge(*shift, *hardness, *smooth)?;
                check_quality(*quality)
            }
            Self::Upscale { scale, quality, .. } => {
                if let Some(s) = scale
                    && !(1.0..=4.0).contains(s)
                {
                    return invalid("The scale goes from 1 to 4.");
                }
                check_quality(*quality)
            }
            Self::RemoveText { quality, .. } => check_quality(*quality),
            Self::Resize {
                width,
                height,
                fit,
                quality,
                max_bytes,
                background,
                ..
            } => {
                if *width == Some(0) || *height == Some(0) {
                    return invalid("Width and height must be at least 1 pixel.");
                }
                if matches!(fit, Some(Fit::Cover | Fit::Fill))
                    && (width.is_none() || height.is_none())
                {
                    return invalid("Cover and fill need both a width and a height.");
                }
                if *max_bytes == Some(0) {
                    return invalid("The size limit must be at least 1 byte.");
                }
                check_colour(background.as_deref(), EMPTY_BACKGROUND)?;
                check_quality(*quality)
            }
            Self::Fix { marketplace } => {
                if marketplace.trim().is_empty() {
                    return invalid("Choose a marketplace.");
                }
                Ok(())
            }
            Self::Run { preset } => {
                if preset.trim().is_empty() {
                    return invalid("Choose a preset.");
                }
                Ok(())
            }
            Self::Recipe { recipe } => recipe.validate(),
            Self::Enhance {
                brightness,
                contrast,
                saturation,
                sharpen,
                quality,
                ..
            } => {
                check_range(
                    *brightness,
                    -100.0,
                    100.0,
                    "Brightness goes from -100 to 100.",
                )?;
                check_range(*contrast, -100.0, 100.0, "Contrast goes from -100 to 100.")?;
                check_range(
                    *saturation,
                    -100.0,
                    100.0,
                    "Saturation goes from -100 to 100.",
                )?;
                check_range(*sharpen, 0.0, 100.0, "Sharpening goes from 0 to 100.")?;
                check_quality(*quality)
            }
            Self::Denoise {
                strength, quality, ..
            }
            | Self::Colorize {
                strength, quality, ..
            }
            | Self::RestoreFaces {
                strength, quality, ..
            }
            | Self::BlurFaces {
                strength, quality, ..
            } => {
                check_strength(*strength)?;
                check_quality(*quality)
            }
            Self::Crop {
                aspect,
                padding,
                quality,
                ..
            } => {
                check_aspect(aspect)?;
                check_range(
                    *padding,
                    0.0,
                    2.0,
                    "The room around the subject goes from 0 to 2 times its longest side.",
                )?;
                check_quality(*quality)
            }
            Self::Expand {
                aspect,
                fill,
                quality,
                ..
            } => {
                check_aspect(aspect)?;
                check_colour(fill.as_deref(), "The colour for the new area is empty.")?;
                check_quality(*quality)
            }
            Self::Watermark {
                text,
                color,
                scale,
                opacity,
                quality,
                ..
            } => {
                let text = text.trim();
                if text.is_empty() {
                    return invalid("Type the watermark's text.");
                }
                if text.chars().count() > MAX_WATERMARK_TEXT {
                    return invalid("The watermark's text can have 200 characters at most.");
                }
                check_colour(color.as_deref(), "The text's colour is empty.")?;
                check_range(
                    *scale,
                    1.0,
                    100.0,
                    "The watermark's size goes from 1 to 100 percent of the photo's width.",
                )?;
                check_range(*opacity, 1.0, 100.0, "The opacity goes from 1 to 100.")?;
                check_quality(*quality)
            }
            Self::Vectorize { .. } => Ok(()),
        }
    }

    /// The query string: only the options that were set, booleans as
    /// `true` (a `false` is the API's default and is left out).
    pub fn query(&self) -> Vec<(String, String)> {
        let mut q = Vec::new();
        match self {
            Self::RemoveBackground {
                background,
                crop,
                padding,
                shadow,
                model,
                studio,
                shift,
                hardness,
                smooth,
                format,
                quality,
            } => {
                push(&mut q, "background", background.as_deref().map(str::trim));
                push(&mut q, "crop", crop.then_some("true"));
                // Padding only means something when cropping.
                push(&mut q, "padding", padding.filter(|_| *crop));
                push(&mut q, "shadow", shadow.map(Shadow::as_str));
                push(&mut q, "model", model.map(Model::as_str));
                push(&mut q, "studio", studio.then_some("true"));
                push(&mut q, "shift", edge(*shift));
                push(&mut q, "hardness", edge(*hardness));
                push(&mut q, "smooth", edge(*smooth));
                push_output(&mut q, *format, *quality);
            }
            Self::Upscale {
                scale,
                format,
                quality,
            } => {
                push(&mut q, "scale", *scale);
                push_output(&mut q, *format, *quality);
            }
            Self::RemoveText {
                region,
                matching,
                format,
                quality,
            } => {
                push(
                    &mut q,
                    "region",
                    region.map(|r| match r {
                        TextRegion::Background => "background",
                        TextRegion::All => "all",
                    }),
                );
                push(
                    &mut q,
                    "matching",
                    matching.as_deref().filter(|m| !m.is_empty()),
                );
                push_output(&mut q, *format, *quality);
            }
            Self::Resize {
                width,
                height,
                fit,
                enlarge,
                format,
                quality,
                max_bytes,
                background,
            } => {
                push(&mut q, "width", *width);
                push(&mut q, "height", *height);
                push(
                    &mut q,
                    "fit",
                    fit.map(|f| match f {
                        Fit::Contain => "contain",
                        Fit::Cover => "cover",
                        Fit::Fill => "fill",
                    }),
                );
                push(&mut q, "enlarge", enlarge.then_some("true"));
                push_output(&mut q, *format, *quality);
                push(&mut q, "max_bytes", *max_bytes);
                push(&mut q, "background", background.as_deref().map(str::trim));
            }
            Self::Fix { marketplace } => push(&mut q, "marketplace", Some(marketplace.trim())),
            Self::Run { preset } => push(&mut q, "preset", Some(preset.trim())),
            // Everything is in the recipe part.
            Self::Recipe { .. } => {}
            Self::Enhance {
                auto,
                brightness,
                contrast,
                saturation,
                sharpen,
                format,
                quality,
            } => {
                push(&mut q, "auto", auto.then_some("true"));
                push(&mut q, "brightness", *brightness);
                push(&mut q, "contrast", *contrast);
                push(&mut q, "saturation", *saturation);
                push(&mut q, "sharpen", *sharpen);
                push_output(&mut q, *format, *quality);
            }
            Self::Denoise {
                strength,
                format,
                quality,
            }
            | Self::Colorize {
                strength,
                format,
                quality,
            }
            | Self::RestoreFaces {
                strength,
                format,
                quality,
            } => {
                push(&mut q, "strength", *strength);
                push_output(&mut q, *format, *quality);
            }
            Self::BlurFaces {
                mode,
                strength,
                format,
                quality,
            } => {
                push(&mut q, "mode", mode.map(FaceMode::as_str));
                // A fill covers the face whole: it has no strength.
                push(
                    &mut q,
                    "strength",
                    strength.filter(|_| *mode != Some(FaceMode::Fill)),
                );
                push_output(&mut q, *format, *quality);
            }
            Self::Crop {
                aspect,
                focus,
                zoom,
                padding,
                format,
                quality,
            } => {
                push(&mut q, "aspect", Some(aspect.trim()));
                push(&mut q, "focus", focus.map(CropFocus::as_str));
                push(&mut q, "zoom", zoom.then_some("true"));
                // Room around the subject only means something with zoom.
                push(&mut q, "padding", padding.filter(|_| *zoom));
                push_output(&mut q, *format, *quality);
            }
            Self::Expand {
                aspect,
                fill,
                format,
                quality,
            } => {
                push(&mut q, "aspect", Some(aspect.trim()));
                push(&mut q, "fill", fill.as_deref().map(str::trim));
                push_output(&mut q, *format, *quality);
            }
            Self::Watermark {
                text,
                color,
                position,
                scale,
                opacity,
                format,
                quality,
            } => {
                push(&mut q, "text", Some(text.trim()));
                push(&mut q, "color", color.as_deref().map(str::trim));
                push(&mut q, "position", position.map(Position::as_str));
                push(&mut q, "scale", *scale);
                push(&mut q, "opacity", *opacity);
                push_output(&mut q, *format, *quality);
            }
            Self::Vectorize { mode, curves } => {
                push(&mut q, "mode", mode.map(VectorMode::as_str));
                push(&mut q, "curves", curves.map(Curves::as_str));
            }
        }
        q
    }
}

/// The most points `select` takes.
pub const MAX_SELECT_POINTS: usize = 32;

/// The query for `select`: each point clicked as `point=x,y`, in the photo's
/// pixels, and which of the model's answers to take.
///
/// `size` is the photo's, when it is known: a point outside it is refused
/// here instead of by the API.
pub fn select_query(
    points: &[[f32; 2]],
    pick: Option<Pick>,
    size: Option<(u32, u32)>,
) -> Result<Vec<(String, String)>, InvalidOperation> {
    if points.is_empty() {
        return invalid("Click the object to cut out first.");
    }
    if points.len() > MAX_SELECT_POINTS {
        return invalid("An object can be marked with 32 points at most.");
    }
    let mut q = Vec::new();
    for [x, y] in points {
        let inside = x.is_finite()
            && y.is_finite()
            && *x >= 0.0
            && *y >= 0.0
            && size.is_none_or(|(width, height)| *x < width as f32 && *y < height as f32);
        if !inside {
            return invalid("A point lies outside the photo.");
        }
        // The pixel the click fell in: finer than that the model does not see.
        q.push((
            "point".to_string(),
            format!("{},{}", x.floor() as u32, y.floor() as u32),
        ));
    }
    push(&mut q, "pick", pick.map(Pick::as_str));
    Ok(q)
}

/// The query for `check`.
pub fn check_query(marketplace: Option<&str>, subject: bool, text: bool) -> Vec<(String, String)> {
    let mut q = Vec::new();
    push(
        &mut q,
        "marketplace",
        marketplace.map(str::trim).filter(|m| !m.is_empty()),
    );
    push(&mut q, "subject", subject.then_some("true"));
    push(&mut q, "text", text.then_some("true"));
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(q: &[(String, String)]) -> Vec<(&str, &str)> {
        q.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }

    #[test]
    fn unset_options_are_left_out() {
        let op: Operation = serde_json::from_str(r#"{"kind":"remove_background"}"#).unwrap();
        assert_eq!(op.endpoint(), "/v1/remove-background");
        assert_eq!(op.price_key(), "remove-background");
        assert!(op.query().is_empty());
        let op: Operation = serde_json::from_str(r#"{"kind":"upscale"}"#).unwrap();
        assert!(op.query().is_empty());
    }

    #[test]
    fn remove_background_options() {
        let op = Operation::RemoveBackground {
            background: Some("#ffffff".into()),
            crop: true,
            padding: Some(20),
            shadow: Some(Shadow::Contact),
            model: Some(Model::Lumen),
            studio: true,
            shift: Some(-1.5),
            hardness: Some(50.0),
            smooth: Some(2.5),
            format: Some(Format::Jpg),
            quality: Some(90),
        };
        assert_eq!(
            pairs(&op.query()),
            [
                ("background", "#ffffff"),
                ("crop", "true"),
                ("padding", "20"),
                ("shadow", "contact"),
                ("model", "lumen"),
                ("studio", "true"),
                ("shift", "-1.5"),
                ("hardness", "50"),
                ("smooth", "2.5"),
                ("format", "jpg"),
                ("quality", "90")
            ]
        );
        // Padding without cropping is not sent.
        let op = Operation::RemoveBackground {
            background: None,
            crop: false,
            padding: Some(20),
            shadow: None,
            model: None,
            studio: false,
            shift: None,
            hardness: None,
            smooth: None,
            format: None,
            quality: None,
        };
        assert!(op.query().is_empty());
        let op: Operation =
            serde_json::from_str(r#"{"kind":"remove_background","shadow":"drop"}"#).unwrap();
        assert_eq!(pairs(&op.query()), [("shadow", "drop")]);
        assert!(
            serde_json::from_str::<Operation>(r#"{"kind":"remove_background","shadow":"long"}"#)
                .is_err()
        );
        // A studio shot is sent only when asked for; `false` is the API's default.
        let op: Operation =
            serde_json::from_str(r#"{"kind":"remove_background","studio":true}"#).unwrap();
        assert_eq!(pairs(&op.query()), [("studio", "true")]);
        let op: Operation =
            serde_json::from_str(r#"{"kind":"remove_background","studio":false}"#).unwrap();
        assert!(op.query().is_empty());
        // A model is sent only when one is given; left out, the API uses `atlas`.
        for model in ["atlas", "lumen", "muse"] {
            let op: Operation = serde_json::from_str(&format!(
                r#"{{"kind":"remove_background","model":"{model}","studio":true}}"#
            ))
            .unwrap();
            assert_eq!(pairs(&op.query()), [("model", model), ("studio", "true")]);
        }
        assert!(
            serde_json::from_str::<Operation>(r#"{"kind":"remove_background","model":"orbit"}"#)
                .is_err()
        );
    }

    fn parsed(json: &str) -> Operation {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_edge_of_a_cutout() {
        let op = parsed(r#"{"kind":"remove_background","shift":-1.5,"hardness":50,"smooth":2.5}"#);
        assert!(op.validate().is_ok());
        assert_eq!(
            pairs(&op.query()),
            [("shift", "-1.5"), ("hardness", "50"), ("smooth", "2.5")]
        );
        // Each is sent by itself, a shift outward too.
        for (json, sent) in [
            (
                r#"{"kind":"remove_background","shift":-1}"#,
                ("shift", "-1"),
            ),
            (
                r#"{"kind":"remove_background","shift":0.5}"#,
                ("shift", "0.5"),
            ),
            (
                r#"{"kind":"remove_background","hardness":100}"#,
                ("hardness", "100"),
            ),
            (
                r#"{"kind":"remove_background","smooth":0.5}"#,
                ("smooth", "0.5"),
            ),
        ] {
            let op = parsed(json);
            assert!(op.validate().is_ok(), "{json}");
            assert_eq!(pairs(&op.query()), [sent], "{json}");
        }
        // At 0, the API's default, nothing is sent: the request is what it
        // was before the API had these controls.
        for json in [
            r#"{"kind":"remove_background","shift":0,"hardness":0,"smooth":0}"#,
            r#"{"kind":"remove_background","shift":-0.0,"hardness":0.0,"smooth":0.0}"#,
            r#"{"kind":"remove_background","shift":null,"hardness":null,"smooth":null}"#,
        ] {
            let op = parsed(json);
            assert!(op.validate().is_ok(), "{json}");
            assert!(op.query().is_empty(), "{json}");
            assert_eq!(
                op.query(),
                parsed(r#"{"kind":"remove_background"}"#).query()
            );
        }
        let op = parsed(r#"{"kind":"remove_background","shift":0,"studio":true,"hardness":40}"#);
        assert_eq!(pairs(&op.query()), [("studio", "true"), ("hardness", "40")]);
        // The ends of each range are in it.
        for ok in [
            r#"{"kind":"remove_background","shift":-10,"hardness":0,"smooth":0}"#,
            r#"{"kind":"remove_background","shift":10,"hardness":100,"smooth":10}"#,
        ] {
            assert!(parsed(ok).validate().is_ok(), "{ok}");
        }
        for (bad, message) in [
            (
                r#"{"kind":"remove_background","shift":-10.5}"#,
                "The edge shift goes from -10 to 10 pixels.",
            ),
            (
                r#"{"kind":"remove_background","shift":11}"#,
                "The edge shift goes from -10 to 10 pixels.",
            ),
            (
                r#"{"kind":"remove_background","hardness":-1}"#,
                "The edge hardness goes from 0 to 100.",
            ),
            (
                r#"{"kind":"remove_background","hardness":100.5}"#,
                "The edge hardness goes from 0 to 100.",
            ),
            (
                r#"{"kind":"remove_background","smooth":-0.5}"#,
                "The edge smoothing goes from 0 to 10.",
            ),
            (
                r#"{"kind":"remove_background","smooth":10.5}"#,
                "The edge smoothing goes from 0 to 10.",
            ),
        ] {
            assert_eq!(
                parsed(bad).validate().unwrap_err().to_string(),
                message,
                "{bad}"
            );
        }
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let op = Operation::RemoveBackground {
                background: None,
                crop: false,
                padding: None,
                shadow: None,
                model: None,
                studio: false,
                shift: Some(value),
                hardness: None,
                smooth: None,
                format: None,
                quality: None,
            };
            assert!(op.validate().is_err(), "{value}");
        }
        // Another tool does not take them.
        assert!(serde_json::from_str::<Operation>(r#"{"kind":"upscale","shift":-1}"#).is_err());
    }

    #[test]
    fn the_new_tools_send_nothing_they_were_not_given() {
        for (json, endpoint, price_key) in [
            (r#"{"kind":"enhance"}"#, "/v1/enhance", "enhance"),
            (r#"{"kind":"denoise"}"#, "/v1/denoise", "denoise"),
            (r#"{"kind":"colorize"}"#, "/v1/colorize", "colorize"),
            (
                r#"{"kind":"restore_faces"}"#,
                "/v1/restore-faces",
                "restore-faces",
            ),
            (r#"{"kind":"blur_faces"}"#, "/v1/blur-faces", "blur-faces"),
            (r#"{"kind":"vectorize"}"#, "/v1/vectorize", "vectorize"),
        ] {
            let op = parsed(json);
            assert_eq!(op.endpoint(), endpoint);
            assert_eq!(op.price_key(), price_key);
            assert!(op.validate().is_ok(), "{json}");
            assert!(op.query().is_empty(), "{json}");
        }
        // These three cannot be run without their one required option.
        assert!(serde_json::from_str::<Operation>(r#"{"kind":"crop"}"#).is_err());
        assert!(serde_json::from_str::<Operation>(r#"{"kind":"expand"}"#).is_err());
        assert!(serde_json::from_str::<Operation>(r#"{"kind":"watermark"}"#).is_err());
    }

    #[test]
    fn enhance_is_automatic_or_by_settings() {
        let op = parsed(
            r#"{"kind":"enhance","auto":true,"brightness":10,"contrast":-20.5,
                "saturation":-100,"sharpen":40,"format":"jpg","quality":85}"#,
        );
        assert!(op.validate().is_ok());
        assert_eq!(op.label(), "Enhance");
        assert_eq!(
            pairs(&op.query()),
            [
                ("auto", "true"),
                ("brightness", "10"),
                ("contrast", "-20.5"),
                ("saturation", "-100"),
                ("sharpen", "40"),
                ("format", "jpg"),
                ("quality", "85")
            ]
        );
        // One setting alone: the automatic part is not asked for.
        let op = parsed(r#"{"kind":"enhance","sharpen":30}"#);
        assert_eq!(pairs(&op.query()), [("sharpen", "30")]);
        for bad in [
            r#"{"kind":"enhance","brightness":101}"#,
            r#"{"kind":"enhance","contrast":-100.5}"#,
            r#"{"kind":"enhance","saturation":300}"#,
            r#"{"kind":"enhance","sharpen":-1}"#,
            r#"{"kind":"enhance","quality":0}"#,
        ] {
            assert!(parsed(bad).validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn the_strength_tools() {
        let op = parsed(r#"{"kind":"denoise","strength":60,"format":"png"}"#);
        assert_eq!(pairs(&op.query()), [("strength", "60"), ("format", "png")]);
        assert_eq!(op.label(), "Denoise");
        let op = parsed(r#"{"kind":"colorize","strength":72.5}"#);
        assert_eq!(pairs(&op.query()), [("strength", "72.5")]);
        assert_eq!(op.label(), "Colourise");
        let op = parsed(r#"{"kind":"restore_faces","strength":100,"quality":90}"#);
        assert_eq!(pairs(&op.query()), [("strength", "100"), ("quality", "90")]);
        assert_eq!(op.label(), "Restore faces");
        for kind in ["denoise", "colorize", "restore_faces", "blur_faces"] {
            for strength in ["0", "0.5", "101"] {
                let bad = format!(r#"{{"kind":"{kind}","strength":{strength}}}"#);
                assert!(parsed(&bad).validate().is_err(), "{bad}");
            }
        }
        let nan = Operation::Denoise {
            strength: Some(f32::NAN),
            format: None,
            quality: None,
        };
        assert!(nan.validate().is_err());
    }

    #[test]
    fn hiding_faces() {
        let op = parsed(r#"{"kind":"blur_faces","mode":"pixelate","strength":80}"#);
        assert!(op.validate().is_ok());
        assert_eq!(op.label(), "Hide faces");
        assert_eq!(
            pairs(&op.query()),
            [("mode", "pixelate"), ("strength", "80")]
        );
        // A fill has no strength to send.
        let op = parsed(r#"{"kind":"blur_faces","mode":"fill","strength":80,"format":"webp"}"#);
        assert_eq!(pairs(&op.query()), [("mode", "fill"), ("format", "webp")]);
        assert!(
            serde_json::from_str::<Operation>(r#"{"kind":"blur_faces","mode":"erase"}"#).is_err()
        );
    }

    #[test]
    fn cropping_to_a_shape() {
        let op = parsed(
            r#"{"kind":"crop","aspect":" 4:5 ","focus":"face","zoom":true,"padding":0.6,
                "format":"jpg"}"#,
        );
        assert!(op.validate().is_ok());
        assert_eq!(op.endpoint(), "/v1/crop");
        assert_eq!(op.label(), "Crop to 4:5");
        assert_eq!(
            pairs(&op.query()),
            [
                ("aspect", "4:5"),
                ("focus", "face"),
                ("zoom", "true"),
                ("padding", "0.6"),
                ("format", "jpg")
            ]
        );
        // Room around the subject without zoom is not sent.
        let op = parsed(r#"{"kind":"crop","aspect":"16:9","padding":0.6}"#);
        assert_eq!(pairs(&op.query()), [("aspect", "16:9")]);
        assert!(
            parsed(r#"{"kind":"crop","aspect":"1.5"}"#)
                .validate()
                .is_ok()
        );
        for bad in [
            r#"{"kind":"crop","aspect":""}"#,
            r#"{"kind":"crop","aspect":"wide"}"#,
            r#"{"kind":"crop","aspect":"4:0"}"#,
            r#"{"kind":"crop","aspect":"4:5:6"}"#,
            r#"{"kind":"crop","aspect":"-2"}"#,
            r#"{"kind":"crop","aspect":"1:1","zoom":true,"padding":2.5}"#,
            r#"{"kind":"crop","aspect":"1:1","padding":-0.1}"#,
        ] {
            assert!(parsed(bad).validate().is_err(), "{bad}");
        }
        assert!(
            serde_json::from_str::<Operation>(r#"{"kind":"crop","aspect":"1:1","focus":"dog"}"#)
                .is_err()
        );
    }

    #[test]
    fn expanding_to_a_shape() {
        let op = parsed(r#"{"kind":"expand","aspect":"1:1"}"#);
        assert!(op.validate().is_ok());
        assert_eq!(op.endpoint(), "/v1/expand");
        assert_eq!(op.label(), "Expand to 1:1");
        // No fill: the API continues the photo by inpainting.
        assert_eq!(pairs(&op.query()), [("aspect", "1:1")]);
        let op = parsed(r#"{"kind":"expand","aspect":"16:9","fill":"white","quality":80}"#);
        assert_eq!(
            pairs(&op.query()),
            [("aspect", "16:9"), ("fill", "white"), ("quality", "80")]
        );
        assert!(
            parsed(r#"{"kind":"expand","aspect":"1:1","fill":" "}"#)
                .validate()
                .is_err()
        );
        assert!(
            parsed(r#"{"kind":"expand","aspect":"square"}"#)
                .validate()
                .is_err()
        );
    }

    #[test]
    fn a_text_watermark() {
        let op = parsed(
            r#"{"kind":"watermark","text":"  © Šoštanj Foto  ","color":"black",
                "position":"top-left","scale":40,"opacity":35,"format":"webp"}"#,
        );
        assert!(op.validate().is_ok());
        assert_eq!(op.endpoint(), "/v1/watermark");
        assert_eq!(
            pairs(&op.query()),
            [
                ("text", "© Šoštanj Foto"),
                ("color", "black"),
                ("position", "top-left"),
                ("scale", "40"),
                ("opacity", "35"),
                ("format", "webp")
            ]
        );
        let op = parsed(r#"{"kind":"watermark","text":"shop.example"}"#);
        assert_eq!(pairs(&op.query()), [("text", "shop.example")]);
        for position in [
            "top-left",
            "top",
            "top-right",
            "left",
            "center",
            "right",
            "bottom-left",
            "bottom",
            "bottom-right",
        ] {
            let op = parsed(&format!(
                r#"{{"kind":"watermark","text":"x","position":"{position}"}}"#
            ));
            assert_eq!(
                op.query()[1],
                ("position".to_string(), position.to_string())
            );
        }
        // 200 characters are counted as characters, not bytes.
        let longest = "š".repeat(200);
        let fits = Operation::Watermark {
            text: longest.clone(),
            color: None,
            position: None,
            scale: None,
            opacity: None,
            format: None,
            quality: None,
        };
        assert!(fits.validate().is_ok());
        for bad in [
            r#"{"kind":"watermark","text":"   "}"#.to_string(),
            format!(r#"{{"kind":"watermark","text":"{longest}š"}}"#),
            r#"{"kind":"watermark","text":"x","opacity":0}"#.to_string(),
            r#"{"kind":"watermark","text":"x","scale":101}"#.to_string(),
            r#"{"kind":"watermark","text":"x","color":""}"#.to_string(),
        ] {
            assert!(parsed(&bad).validate().is_err(), "{bad}");
        }
        assert!(
            serde_json::from_str::<Operation>(
                r#"{"kind":"watermark","text":"x","position":"middle"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn vectorize_gives_an_svg_not_pixels() {
        let op = parsed(r#"{"kind":"vectorize","mode":"mono","curves":"polygon"}"#);
        assert!(op.validate().is_ok());
        assert_eq!(
            pairs(&op.query()),
            [("mode", "mono"), ("curves", "polygon")]
        );
        assert!(!op.gives_pixels());
        // It has no output format to choose.
        assert!(
            serde_json::from_str::<Operation>(r#"{"kind":"vectorize","format":"png"}"#).is_err()
        );
        for json in [
            r#"{"kind":"remove_background"}"#,
            r#"{"kind":"enhance"}"#,
            r#"{"kind":"crop","aspect":"1:1"}"#,
            r#"{"kind":"blur_faces"}"#,
        ] {
            assert!(parsed(json).gives_pixels(), "{json}");
        }
    }

    #[test]
    fn select_sends_each_click_as_a_point() {
        let q = select_query(&[[320.4, 240.9]], None, Some((640, 480))).unwrap();
        assert_eq!(pairs(&q), [("point", "320,240")]);
        let q = select_query(
            &[[0.0, 0.0], [639.9, 479.9]],
            Some(Pick::Largest),
            Some((640, 480)),
        )
        .unwrap();
        assert_eq!(
            pairs(&q),
            [("point", "0,0"), ("point", "639,479"), ("pick", "largest")]
        );
        // Without a known size the API is left to judge the points.
        let q = select_query(&[[5000.0, 10.0]], Some(Pick::Smallest), None).unwrap();
        assert_eq!(pairs(&q), [("point", "5000,10"), ("pick", "smallest")]);

        assert!(select_query(&[], None, Some((640, 480))).is_err());
        for outside in [
            [640.0, 10.0],
            [10.0, 480.0],
            [-1.0, 10.0],
            [f32::NAN, 10.0],
            [10.0, f32::INFINITY],
        ] {
            assert!(
                select_query(&[outside], None, Some((640, 480))).is_err(),
                "{outside:?}"
            );
        }
        let many = vec![[1.0, 1.0]; MAX_SELECT_POINTS];
        assert_eq!(select_query(&many, None, None).unwrap().len(), 32);
        let too_many = vec![[1.0, 1.0]; MAX_SELECT_POINTS + 1];
        assert!(select_query(&too_many, None, None).is_err());
    }

    #[test]
    fn resize_options_and_rules() {
        let op: Operation = serde_json::from_str(
            r#"{"kind":"resize","width":800,"height":600,"fit":"cover","enlarge":true,
                "format":"webp","quality":80,"max_bytes":300000,"background":"white"}"#,
        )
        .unwrap();
        assert!(op.validate().is_ok());
        assert_eq!(op.price_key(), "resize");
        assert_eq!(
            pairs(&op.query()),
            [
                ("width", "800"),
                ("height", "600"),
                ("fit", "cover"),
                ("enlarge", "true"),
                ("format", "webp"),
                ("quality", "80"),
                ("max_bytes", "300000"),
                ("background", "white")
            ]
        );
        let cover_without_height: Operation =
            serde_json::from_str(r#"{"kind":"resize","width":800,"fit":"cover"}"#).unwrap();
        assert!(cover_without_height.validate().is_err());
        let zero: Operation = serde_json::from_str(r#"{"kind":"resize","width":0}"#).unwrap();
        assert!(zero.validate().is_err());
        let quality: Operation =
            serde_json::from_str(r#"{"kind":"resize","quality":101}"#).unwrap();
        assert!(quality.validate().is_err());
    }

    #[test]
    fn the_other_operations() {
        let op = Operation::Upscale {
            scale: Some(2.0),
            format: None,
            quality: None,
        };
        assert_eq!(pairs(&op.query()), [("scale", "2")]);
        assert_eq!(op.label(), "Upscale 2×");
        assert!(
            Operation::Upscale {
                scale: Some(5.0),
                format: None,
                quality: None
            }
            .validate()
            .is_err()
        );

        let op = Operation::RemoveText {
            region: Some(TextRegion::All),
            matching: Some("sale|€".into()),
            format: Some(Format::Avif),
            quality: None,
        };
        assert_eq!(op.endpoint(), "/v1/remove-text");
        assert_eq!(
            pairs(&op.query()),
            [
                ("region", "all"),
                ("matching", "sale|€"),
                ("format", "avif")
            ]
        );

        let op = Operation::Fix {
            marketplace: "amazon".into(),
        };
        assert_eq!(op.endpoint(), "/v1/fix");
        assert_eq!(pairs(&op.query()), [("marketplace", "amazon")]);
        assert!(
            Operation::Fix {
                marketplace: " ".into()
            }
            .validate()
            .is_err()
        );

        let op = Operation::Run {
            preset: "studio".into(),
        };
        assert_eq!(op.endpoint(), "/v1/run");
        assert_eq!(op.price_key(), "run");
        assert_eq!(pairs(&op.query()), [("preset", "studio")]);
    }

    #[test]
    fn unknown_fields_and_kinds_are_refused() {
        assert!(serde_json::from_str::<Operation>(r#"{"kind":"identify"}"#).is_err());
        // The tools that answer with JSON, and identity photos, are not offered.
        for kind in ["caption", "palette", "faces", "id_photo"] {
            let json = format!(r#"{{"kind":"{kind}"}}"#);
            assert!(serde_json::from_str::<Operation>(&json).is_err(), "{kind}");
        }
        assert!(serde_json::from_str::<Operation>(r#"{"kind":"upscale","path":"/x"}"#).is_err());
    }

    #[test]
    fn check_query_options() {
        assert!(check_query(None, false, false).is_empty());
        assert_eq!(
            pairs(&check_query(Some("etsy"), true, true)),
            [
                ("marketplace", "etsy"),
                ("subject", "true"),
                ("text", "true")
            ]
        );
        assert!(check_query(Some(""), false, false).is_empty());
    }
}
