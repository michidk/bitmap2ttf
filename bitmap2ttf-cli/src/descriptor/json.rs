use super::{GlyphRect, LoadedDescriptor, glyph_rects_to_bitmap_glyphs, load_page_images};
use crate::CliError;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct JsonDescriptor {
    line_height: u16,
    pages: Option<Vec<JsonPage>>,
    image: Option<String>,
    glyphs: Vec<JsonGlyph>,
}

#[derive(Debug, Deserialize)]
struct JsonPage {
    id: u16,
    file: String,
}

#[derive(Debug, Deserialize)]
struct JsonGlyph {
    codepoint: u32,
    x: u32,
    y: u32,
    width: u16,
    height: u16,
    #[serde(alias = "xoffset")]
    offset_x: i16,
    #[serde(alias = "yoffset")]
    offset_y: i16,
    #[serde(alias = "xadvance")]
    advance_width: Option<u16>,
    #[serde(default)]
    page: u16,
}

pub(super) fn load(input_path: &Path) -> Result<LoadedDescriptor, CliError> {
    let text = std::fs::read_to_string(input_path)?;
    let descriptor: JsonDescriptor = serde_json::from_str(&text)?;
    if descriptor.glyphs.is_empty() {
        return Err(CliError::Parse(
            "JSON descriptor has no glyph entries".to_string(),
        ));
    }
    let pages = descriptor_pages(&descriptor)?;
    let images = load_page_images(input_path, &pages)?;
    let glyphs = json_glyphs_to_rects(&descriptor);
    Ok(LoadedDescriptor {
        line_height: descriptor.line_height,
        glyphs: glyph_rects_to_bitmap_glyphs(&glyphs, &images)?,
    })
}

fn descriptor_pages(descriptor: &JsonDescriptor) -> Result<HashMap<u16, String>, CliError> {
    if let Some(pages) = &descriptor.pages {
        if pages.is_empty() {
            return Err(CliError::Parse(
                "JSON descriptor pages array is empty".to_string(),
            ));
        }
        return Ok(pages
            .iter()
            .map(|page| (page.id, page.file.clone()))
            .collect());
    }
    if let Some(image) = &descriptor.image {
        return Ok(HashMap::from([(0, image.clone())]));
    }
    Err(CliError::Parse(
        "JSON descriptor must define either 'pages' or 'image'".to_string(),
    ))
}

fn json_glyphs_to_rects(descriptor: &JsonDescriptor) -> Vec<GlyphRect> {
    descriptor
        .glyphs
        .iter()
        .map(|glyph| GlyphRect {
            id: glyph.codepoint,
            x: glyph.x,
            y: glyph.y,
            width: glyph.width,
            height: glyph.height,
            offset_x: glyph.offset_x,
            offset_y: glyph.offset_y,
            advance_width: glyph.advance_width.unwrap_or(glyph.width.saturating_add(1)),
            page: glyph.page,
        })
        .collect()
}
