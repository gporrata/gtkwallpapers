//! Burns the wallpaper's file name into the lower-right corner in small print.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use anyhow::{Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{Rgb, RgbImage};
use std::path::{Path, PathBuf};

const FONT: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

/// Images wider than this are scaled down before labelling; no desktop needs
/// more and it keeps rendering/encoding fast.
const MAX_WIDTH: u32 = 3840;

/// GNOME's default `zoom` mode crops the image to the monitor's aspect ratio.
/// Keep the label inside the area visible on both 16:9 and 16:10 screens.
const SCREEN_ASPECTS: &[f32] = &[16.0 / 9.0, 16.0 / 10.0];

/// Directory for rendered (labelled) wallpapers: ~/.cache/gtkwallpapers/.
fn render_dir() -> Result<PathBuf> {
    let dir = dirs::cache_dir()
        .context("could not resolve $HOME/.cache")?
        .join("gtkwallpapers");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Render a copy of `source` with its file name in the lower-right corner and
/// return the path of the copy. Previous renders are removed. A fresh file
/// name is used each time so GNOME notices the change.
pub fn render_labelled(source: &Path) -> Result<PathBuf> {
    let label = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut img = image::open(source)
        .with_context(|| format!("failed to open {}", source.display()))?
        .into_rgb8();
    if img.width() > MAX_WIDTH {
        let height = (img.height() as u64 * MAX_WIDTH as u64 / img.width() as u64) as u32;
        img = image::imageops::resize(&img, MAX_WIDTH, height.max(1), FilterType::Triangle);
    }

    draw_label(&mut img, &label)?;

    let dir = render_dir()?;
    for entry in std::fs::read_dir(&dir)?.filter_map(|e| e.ok()) {
        if entry.file_name().to_string_lossy().starts_with("current-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dest = dir.join(format!("current-{stamp}.jpg"));
    let file = std::io::BufWriter::new(std::fs::File::create(&dest)?);
    JpegEncoder::new_with_quality(file, 92).encode_image(&img)?;
    Ok(dest)
}

/// Lower-right corner (exclusive) of the region that stays visible when the
/// image is zoomed to fill any of `SCREEN_ASPECTS`.
fn visible_corner(width: u32, height: u32) -> (f32, f32) {
    let (w, h) = (width as f32, height as f32);
    let mut right = w;
    let mut bottom = h;
    for aspect in SCREEN_ASPECTS {
        if w / h > *aspect {
            let visible_w = h * aspect;
            right = right.min((w + visible_w) / 2.0);
        } else {
            let visible_h = w / aspect;
            bottom = bottom.min((h + visible_h) / 2.0);
        }
    }
    (right, bottom)
}

fn draw_label(img: &mut RgbImage, text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let font = FontRef::try_from_slice(FONT).context("failed to load embedded font")?;

    let (right, bottom) = visible_corner(img.width(), img.height());
    // ~13px on a 1080p-tall view, scaling with the image so it looks the same
    // size on screen whatever the source resolution.
    let size = (bottom.min(img.height() as f32) * 0.012).clamp(10.0, 48.0);
    let scaled = font.as_scaled(PxScale::from(size));

    // Measure.
    let mut width = 0.0f32;
    let mut prev = None;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(prev) = prev {
            width += scaled.kern(prev, id);
        }
        width += scaled.h_advance(id);
        prev = Some(id);
    }

    let pad_x = (size * 0.5).round();
    let pad_y = (size * 0.3).round();
    let margin = (size * 1.2).round();
    let box_w = width.ceil() + pad_x * 2.0;
    let box_h = (scaled.ascent() - scaled.descent()).ceil() + pad_y * 2.0;
    let box_x = (right - margin - box_w).max(0.0);
    let box_y = (bottom - margin - box_h).max(0.0);

    // Translucent dark plate keeps white text legible on light and dark images.
    fill_rect(img, box_x, box_y, box_w, box_h, Rgb([0, 0, 0]), 0.35);

    let mut caret = point(box_x + pad_x, box_y + pad_y + scaled.ascent());
    let mut prev = None;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(prev) = prev {
            caret.x += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(scaled.scale(), caret);
        caret.x += scaled.h_advance(id);
        prev = Some(id);

        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let x = bounds.min.x as i64 + gx as i64;
                let y = bounds.min.y as i64 + gy as i64;
                blend(img, x, y, Rgb([255, 255, 255]), coverage * 0.85);
            });
        }
    }
    Ok(())
}

fn fill_rect(img: &mut RgbImage, x: f32, y: f32, w: f32, h: f32, color: Rgb<u8>, alpha: f32) {
    let (x0, y0) = (x.round() as i64, y.round() as i64);
    let (x1, y1) = ((x + w).round() as i64, (y + h).round() as i64);
    for py in y0..y1 {
        for px in x0..x1 {
            blend(img, px, py, color, alpha);
        }
    }
}

fn blend(img: &mut RgbImage, x: i64, y: i64, color: Rgb<u8>, alpha: f32) {
    if x < 0 || y < 0 || x >= img.width() as i64 || y >= img.height() as i64 {
        return;
    }
    let alpha = alpha.clamp(0.0, 1.0);
    let pixel = img.get_pixel_mut(x as u32, y as u32);
    for (dst, src) in pixel.0.iter_mut().zip(color.0) {
        *dst = (*dst as f32 * (1.0 - alpha) + src as f32 * alpha).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_corner_crops_wide_images_horizontally() {
        let (right, bottom) = visible_corner(3000, 1000);
        assert!(right < 3000.0);
        assert_eq!(bottom, 1000.0);
    }

    #[test]
    fn visible_corner_crops_tall_images_vertically() {
        let (right, bottom) = visible_corner(1000, 1000);
        assert_eq!(right, 1000.0);
        assert!(bottom < 1000.0);
    }

    #[test]
    fn label_changes_lower_right_pixels_only() {
        let mut img = RgbImage::from_pixel(1920, 1080, Rgb([200, 200, 200]));
        draw_label(&mut img, "pexels/12345.jpg").unwrap();
        assert_eq!(*img.get_pixel(10, 10), Rgb([200, 200, 200]));
        let changed = (1500..1920)
            .flat_map(|x| (1000..1080).map(move |y| (x, y)))
            .any(|(x, y)| *img.get_pixel(x, y) != Rgb([200, 200, 200]));
        assert!(changed);
    }
}
