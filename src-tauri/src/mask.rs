//! The erase mask: brush strokes painted in the editor, drawn into a
//! greyscale image the size of the photo (white = remove).
//!
//! The UI sends the strokes (a few numbers), not pixels; the mask is made
//! here, at the photo's real size.

use std::io::Cursor;

use image::{GrayImage, ImageFormat, Luma};
use serde::Deserialize;

/// One drag of the brush: a round tip moved through `points`, in the
/// photo's pixels.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Stroke {
    pub radius: f32,
    pub points: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MaskError {
    #[error("Paint over what should be erased first.")]
    Empty,
    #[error("The photo's size is not known, so no mask can be made for it.")]
    UnknownSize,
    #[error("The mask is too large or has too many strokes.")]
    TooLarge,
}

const MAX_SIDE: u32 = 20_000;
const MAX_POINTS: usize = 400_000;

/// Draws the strokes; an error when nothing ends up painted.
pub fn rasterize(width: u32, height: u32, strokes: &[Stroke]) -> Result<GrayImage, MaskError> {
    if width == 0 || height == 0 {
        return Err(MaskError::UnknownSize);
    }
    let points: usize = strokes.iter().map(|s| s.points.len()).sum();
    if width > MAX_SIDE || height > MAX_SIDE || points > MAX_POINTS {
        return Err(MaskError::TooLarge);
    }
    let mut mask = GrayImage::new(width, height);
    let mut painted = false;
    for stroke in strokes {
        if !stroke.radius.is_finite() {
            continue;
        }
        let radius = stroke.radius.clamp(0.5, 4000.0);
        let points: Vec<[f32; 2]> = stroke
            .points
            .iter()
            .copied()
            .filter(|p| p[0].is_finite() && p[1].is_finite())
            .collect();
        match points.as_slice() {
            [] => {}
            [only] => painted |= capsule(&mut mask, *only, *only, radius),
            many => {
                for pair in many.windows(2) {
                    painted |= capsule(&mut mask, pair[0], pair[1], radius);
                }
            }
        }
    }
    if painted {
        Ok(mask)
    } else {
        Err(MaskError::Empty)
    }
}

/// The mask as a PNG, as the API takes it.
pub fn mask_png(width: u32, height: u32, strokes: &[Stroke]) -> Result<Vec<u8>, MaskError> {
    let mask = rasterize(width, height, strokes)?;
    let mut png = Cursor::new(Vec::new());
    mask.write_to(&mut png, ImageFormat::Png)
        .map_err(|_| MaskError::TooLarge)?;
    Ok(png.into_inner())
}

/// Paints every pixel within `radius` of the segment a-b. Returns whether
/// any pixel of the image was touched.
fn capsule(mask: &mut GrayImage, a: [f32; 2], b: [f32; 2], radius: f32) -> bool {
    let (width, height) = mask.dimensions();
    let min_x = (a[0].min(b[0]) - radius).floor().max(0.0);
    let min_y = (a[1].min(b[1]) - radius).floor().max(0.0);
    let max_x = (a[0].max(b[0]) + radius).ceil().min(width as f32 - 1.0);
    let max_y = (a[1].max(b[1]) + radius).ceil().min(height as f32 - 1.0);
    if min_x > max_x || min_y > max_y {
        return false;
    }
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length_squared = dx * dx + dy * dy;
    let mut painted = false;
    for y in min_y as u32..=max_y as u32 {
        for x in min_x as u32..=max_x as u32 {
            // The pixel's centre against the nearest point of the segment.
            let (px, py) = (x as f32 + 0.5 - a[0], y as f32 + 0.5 - a[1]);
            let t = if length_squared > 0.0 {
                ((px * dx + py * dy) / length_squared).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (ex, ey) = (px - t * dx, py - t * dy);
            if ex * ex + ey * ey <= radius * radius {
                mask.put_pixel(x, y, Luma([255]));
                painted = true;
            }
        }
    }
    painted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(radius: f32, points: &[[f32; 2]]) -> Stroke {
        Stroke {
            radius,
            points: points.to_vec(),
        }
    }

    fn white(mask: &GrayImage, x: u32, y: u32) -> bool {
        mask.get_pixel(x, y).0[0] == 255
    }

    #[test]
    fn a_dab_is_a_disc() {
        let mask = rasterize(40, 30, &[stroke(5.0, &[[20.0, 15.0]])]).unwrap();
        assert_eq!(mask.dimensions(), (40, 30));
        assert!(white(&mask, 20, 15));
        assert!(white(&mask, 16, 15) && white(&mask, 23, 15));
        assert!(!white(&mask, 26, 15) && !white(&mask, 20, 21));
        assert!(!white(&mask, 0, 0));
        let count = mask.pixels().filter(|p| p.0[0] == 255).count() as f32;
        let area = std::f32::consts::PI * 25.0;
        assert!(
            (count - area).abs() < 12.0,
            "{count} px for an area of {area}"
        );
        // Only black and white.
        assert!(mask.pixels().all(|p| p.0[0] == 0 || p.0[0] == 255));
    }

    #[test]
    fn a_drag_covers_the_path_between_its_points() {
        let mask = rasterize(100, 40, &[stroke(4.0, &[[10.0, 20.0], [90.0, 20.0]])]).unwrap();
        for x in (10..90).step_by(5) {
            assert!(white(&mask, x, 20), "x = {x}");
            assert!(white(&mask, x, 17) && white(&mask, x, 23));
            assert!(!white(&mask, x, 10) && !white(&mask, x, 30));
        }
        assert!(!white(&mask, 2, 20) && !white(&mask, 97, 20));
    }

    #[test]
    fn strokes_are_clipped_to_the_photo() {
        let mask = rasterize(20, 20, &[stroke(6.0, &[[-2.0, -2.0], [3.0, 3.0]])]).unwrap();
        assert!(white(&mask, 0, 0));
        assert!(!white(&mask, 19, 19));
    }

    #[test]
    fn nothing_painted_is_an_error() {
        assert_eq!(rasterize(20, 20, &[]), Err(MaskError::Empty));
        assert_eq!(
            rasterize(20, 20, &[stroke(3.0, &[])]),
            Err(MaskError::Empty)
        );
        // Wholly outside the photo.
        assert_eq!(
            rasterize(20, 20, &[stroke(3.0, &[[200.0, 200.0]])]),
            Err(MaskError::Empty)
        );
        assert_eq!(
            rasterize(20, 20, &[stroke(f32::NAN, &[[5.0, 5.0]])]),
            Err(MaskError::Empty)
        );
        assert_eq!(
            rasterize(0, 20, &[stroke(3.0, &[[5.0, 5.0]])]),
            Err(MaskError::UnknownSize)
        );
        assert_eq!(
            rasterize(30_000, 20, &[stroke(3.0, &[[5.0, 5.0]])]),
            Err(MaskError::TooLarge)
        );
    }

    #[test]
    fn the_png_has_the_photos_size() {
        let png = mask_png(64, 48, &[stroke(5.0, &[[10.0, 10.0], [30.0, 30.0]])]).unwrap();
        let decoded = image::load_from_memory_with_format(&png, ImageFormat::Png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 48));
        assert_eq!(decoded.color(), image::ColorType::L8);
        assert_eq!(decoded.to_luma8().get_pixel(20, 20).0[0], 255);
    }
}
