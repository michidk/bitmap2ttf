use super::CliError;
use bitmap2ttf::BitmapGlyph;
use image::ImageReader;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod bmfont;
mod json;

#[derive(Debug, Clone)]
pub(super) struct GlyphRect {
    pub(super) id: u32,
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) offset_x: i16,
    pub(super) offset_y: i16,
    pub(super) advance_width: u16,
    pub(super) page: u16,
}

#[derive(Debug, Clone)]
pub(super) struct LoadedDescriptor {
    pub(super) line_height: u16,
    pub(super) glyphs: Vec<BitmapGlyph>,
}

pub(super) fn load_descriptor(input_path: &Path) -> Result<LoadedDescriptor, CliError> {
    let extension = input_path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "fnt" => bmfont::load(input_path),
        "json" => json::load(input_path),
        _ => Err(CliError::Parse(
            "unsupported descriptor extension; use .fnt or .json".to_string(),
        )),
    }
}

pub(super) fn load_page_images(
    input_path: &Path,
    pages: &HashMap<u16, String>,
) -> Result<HashMap<u16, image::RgbaImage>, CliError> {
    let base_dir = input_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut images = HashMap::new();
    for (&id, filename) in pages {
        let image = ImageReader::open(base_dir.join(filename))?
            .decode()?
            .to_rgba8();
        images.insert(id, image);
    }
    Ok(images)
}

pub(super) fn glyph_rects_to_bitmap_glyphs(
    glyphs: &[GlyphRect],
    images: &HashMap<u16, image::RgbaImage>,
) -> Result<Vec<BitmapGlyph>, CliError> {
    glyphs
        .iter()
        .map(|glyph| {
            let image = images
                .get(&glyph.page)
                .ok_or_else(|| CliError::Parse(format!("missing image for page {}", glyph.page)))?;
            Ok(BitmapGlyph {
                codepoint: glyph.id,
                width: glyph.width,
                height: glyph.height,
                offset_x: glyph.offset_x,
                offset_y: glyph.offset_y,
                advance_width: Some(glyph.advance_width),
                pixels: extract_glyph_pixels(image, glyph)?,
            })
        })
        .collect()
}

fn extract_glyph_pixels(image: &image::RgbaImage, glyph: &GlyphRect) -> Result<Vec<u8>, CliError> {
    let width = u32::from(glyph.width);
    let height = u32::from(glyph.height);
    let x_end = glyph
        .x
        .checked_add(width)
        .ok_or_else(|| CliError::Parse("glyph x range overflow".to_string()))?;
    let y_end = glyph
        .y
        .checked_add(height)
        .ok_or_else(|| CliError::Parse("glyph y range overflow".to_string()))?;
    if x_end > image.width() || y_end > image.height() {
        return Err(CliError::Parse(format!(
            "glyph rectangle out of bounds: x={}, y={}, width={}, height={}, image={}x{}",
            glyph.x,
            glyph.y,
            glyph.width,
            glyph.height,
            image.width(),
            image.height()
        )));
    }
    let area = width
        .checked_mul(height)
        .ok_or_else(|| CliError::Parse("glyph area overflow".to_string()))?;
    let capacity = usize::try_from(area)
        .map_err(|error| CliError::Parse(format!("glyph area to usize failed: {error}")))?;
    let mut pixels = Vec::with_capacity(capacity);
    for y in 0..height {
        for x in 0..width {
            let pixel = image.get_pixel(glyph.x + x, glyph.y + y);
            pixels.push(u8::from(pixel[3] != 0));
        }
    }
    Ok(pixels)
}
