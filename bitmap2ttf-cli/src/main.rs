use bitmap2ttf::{BitmapGlyph, FontConfig, build_ttf};
use clap::Parser;
use image::ImageReader;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Parser, Debug)]
#[command(name = "bitmap2ttf")]
#[command(about = "Convert bitmap font descriptors to TrueType")]
struct Args {
    #[arg(help = "Input descriptor (.fnt BMFont text or .json PNG+JSON)")]
    input: PathBuf,
    #[arg(short, long, help = "Output TrueType file path (.ttf)")]
    output: PathBuf,
    #[arg(long, help = "Override output family name")]
    family_name: Option<String>,
    #[arg(long, help = "Override font line height in pixels")]
    line_height: Option<u16>,
    #[arg(
        long,
        default_value_t = 64,
        help = "Coordinate scale multiplier for TTF units"
    )]
    scale: u32,
}

#[derive(Debug, Clone)]
struct BmfontInfo {
    line_height: u16,
    pages: HashMap<u16, String>,
    chars: Vec<GlyphRect>,
}

#[derive(Debug, Clone)]
struct GlyphRect {
    id: u32,
    x: u32,
    y: u32,
    width: u16,
    height: u16,
    offset_x: i16,
    offset_y: i16,
    advance_width: u16,
    page: u16,
}

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

#[derive(Debug, Clone)]
struct LoadedDescriptor {
    line_height: u16,
    glyphs: Vec<BitmapGlyph>,
}

#[derive(Error, Debug)]
enum CliError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image decode error: {0}")]
    Image(#[from] image::ImageError),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Descriptor parse error: {0}")]
    Parse(String),
    #[error("TTF conversion error: {0}")]
    Convert(String),
}

fn parse_key_values(input: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut idx = 0_usize;
    let bytes = input.as_bytes();

    while idx < bytes.len() {
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }

        let key_start = idx;
        while idx < bytes.len() && bytes[idx] != b'=' && !bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= bytes.len() || bytes[idx] != b'=' {
            while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() {
                idx += 1;
            }
            continue;
        }
        let key = &input[key_start..idx];
        idx += 1;

        if idx >= bytes.len() {
            map.insert(key.to_string(), String::new());
            break;
        }

        let value = if bytes[idx] == b'"' {
            idx += 1;
            let value_start = idx;
            while idx < bytes.len() && bytes[idx] != b'"' {
                idx += 1;
            }
            let v = &input[value_start..idx.min(bytes.len())];
            if idx < bytes.len() && bytes[idx] == b'"' {
                idx += 1;
            }
            v.to_string()
        } else {
            let value_start = idx;
            while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() {
                idx += 1;
            }
            input[value_start..idx].to_string()
        };

        map.insert(key.to_string(), value);
    }

    map
}

fn parse_u16(map: &HashMap<String, String>, key: &str) -> Result<u16, CliError> {
    let raw = map
        .get(key)
        .ok_or_else(|| CliError::Parse(format!("missing key '{key}'")))?;
    raw.parse::<u16>()
        .map_err(|e| CliError::Parse(format!("invalid u16 for '{key}': {e}")))
}

fn parse_u32(map: &HashMap<String, String>, key: &str) -> Result<u32, CliError> {
    let raw = map
        .get(key)
        .ok_or_else(|| CliError::Parse(format!("missing key '{key}'")))?;
    raw.parse::<u32>()
        .map_err(|e| CliError::Parse(format!("invalid u32 for '{key}': {e}")))
}

fn parse_i16(map: &HashMap<String, String>, key: &str) -> Result<i16, CliError> {
    let raw = map
        .get(key)
        .ok_or_else(|| CliError::Parse(format!("missing key '{key}'")))?;
    raw.parse::<i16>()
        .map_err(|e| CliError::Parse(format!("invalid i16 for '{key}': {e}")))
}

fn parse_bmfont_text(contents: &str) -> Result<BmfontInfo, CliError> {
    let mut line_height: Option<u16> = None;
    let mut pages: HashMap<u16, String> = HashMap::new();
    let mut chars = Vec::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let tag = parts.next().unwrap_or_default();
        let kv = parts.next().unwrap_or_default();
        let map = parse_key_values(kv);

        match tag {
            "common" => {
                line_height = Some(parse_u16(&map, "lineHeight")?);
            }
            "page" => {
                let id = parse_u16(&map, "id")?;
                let file = map.get("file").ok_or_else(|| {
                    CliError::Parse("missing key 'file' on page line".to_string())
                })?;
                pages.insert(id, file.to_string());
            }
            "char" => {
                let ch = GlyphRect {
                    id: parse_u32(&map, "id")?,
                    x: parse_u32(&map, "x")?,
                    y: parse_u32(&map, "y")?,
                    width: parse_u16(&map, "width")?,
                    height: parse_u16(&map, "height")?,
                    offset_x: parse_i16(&map, "xoffset")?,
                    offset_y: parse_i16(&map, "yoffset")?,
                    advance_width: parse_u16(&map, "xadvance")?,
                    page: parse_u16(&map, "page")?,
                };
                chars.push(ch);
            }
            _ => {}
        }
    }

    let line_height = line_height
        .ok_or_else(|| CliError::Parse("missing 'common' line with lineHeight".to_string()))?;
    if pages.is_empty() {
        return Err(CliError::Parse("no BMFont pages found".to_string()));
    }
    if chars.is_empty() {
        return Err(CliError::Parse("no BMFont char entries found".to_string()));
    }

    Ok(BmfontInfo {
        line_height,
        pages,
        chars,
    })
}

fn extract_glyph_pixels(
    image: &image::RgbaImage,
    x: u32,
    y: u32,
    width: u16,
    height: u16,
) -> Result<Vec<u8>, CliError> {
    let w = u32::from(width);
    let h = u32::from(height);
    let x_end = x
        .checked_add(w)
        .ok_or_else(|| CliError::Parse("glyph x range overflow".to_string()))?;
    let y_end = y
        .checked_add(h)
        .ok_or_else(|| CliError::Parse("glyph y range overflow".to_string()))?;

    if x_end > image.width() || y_end > image.height() {
        return Err(CliError::Parse(format!(
            "glyph rectangle out of bounds: x={x}, y={y}, width={width}, height={height}, image={}x{}",
            image.width(),
            image.height()
        )));
    }

    let len_u32 = w
        .checked_mul(h)
        .ok_or_else(|| CliError::Parse("glyph area overflow".to_string()))?;
    let len = usize::try_from(len_u32)
        .map_err(|e| CliError::Parse(format!("glyph area to usize failed: {e}")))?;

    let mut pixels = Vec::with_capacity(len);
    for gy in 0..h {
        for gx in 0..w {
            let p = image.get_pixel(x + gx, y + gy);
            pixels.push(if p[3] == 0 { 0 } else { 1 });
        }
    }
    Ok(pixels)
}

fn load_page_images(
    input_path: &Path,
    pages: &HashMap<u16, String>,
) -> Result<HashMap<u16, image::RgbaImage>, CliError> {
    let base_dir = input_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut page_images: HashMap<u16, image::RgbaImage> = HashMap::new();
    for (&id, filename) in pages {
        let page_path = base_dir.join(filename);
        let image = ImageReader::open(&page_path)?.decode()?.to_rgba8();
        page_images.insert(id, image);
    }
    Ok(page_images)
}

fn glyph_rects_to_bitmap_glyphs(
    chars: &[GlyphRect],
    page_images: &HashMap<u16, image::RgbaImage>,
) -> Result<Vec<BitmapGlyph>, CliError> {
    let mut glyphs = Vec::with_capacity(chars.len());
    for ch in chars {
        let page = page_images
            .get(&ch.page)
            .ok_or_else(|| CliError::Parse(format!("missing image for page {}", ch.page)))?;
        let pixels = extract_glyph_pixels(page, ch.x, ch.y, ch.width, ch.height)?;
        glyphs.push(BitmapGlyph {
            codepoint: ch.id,
            width: ch.width,
            height: ch.height,
            offset_x: ch.offset_x,
            offset_y: ch.offset_y,
            advance_width: Some(ch.advance_width),
            pixels,
        });
    }
    Ok(glyphs)
}

fn load_bmfont_descriptor(input_path: &Path) -> Result<LoadedDescriptor, CliError> {
    let bmfont_text = std::fs::read_to_string(input_path)?;
    let info = parse_bmfont_text(&bmfont_text)?;
    let page_images = load_page_images(input_path, &info.pages)?;
    let glyphs = glyph_rects_to_bitmap_glyphs(&info.chars, &page_images)?;
    Ok(LoadedDescriptor {
        line_height: info.line_height,
        glyphs,
    })
}

fn descriptor_pages(json: &JsonDescriptor) -> Result<HashMap<u16, String>, CliError> {
    if let Some(pages) = &json.pages {
        if pages.is_empty() {
            return Err(CliError::Parse(
                "JSON descriptor pages array is empty".to_string(),
            ));
        }
        return Ok(pages.iter().map(|p| (p.id, p.file.clone())).collect());
    }

    if let Some(image) = &json.image {
        let mut map = HashMap::new();
        map.insert(0, image.clone());
        return Ok(map);
    }

    Err(CliError::Parse(
        "JSON descriptor must define either 'pages' or 'image'".to_string(),
    ))
}

fn json_glyphs_to_rects(json: &JsonDescriptor) -> Vec<GlyphRect> {
    json.glyphs
        .iter()
        .map(|g| GlyphRect {
            id: g.codepoint,
            x: g.x,
            y: g.y,
            width: g.width,
            height: g.height,
            offset_x: g.offset_x,
            offset_y: g.offset_y,
            advance_width: g.advance_width.unwrap_or(g.width.saturating_add(1)),
            page: g.page,
        })
        .collect()
}

fn load_json_descriptor(input_path: &Path) -> Result<LoadedDescriptor, CliError> {
    let text = std::fs::read_to_string(input_path)?;
    let json: JsonDescriptor = serde_json::from_str(&text)?;

    if json.glyphs.is_empty() {
        return Err(CliError::Parse(
            "JSON descriptor has no glyph entries".to_string(),
        ));
    }

    let pages = descriptor_pages(&json)?;
    let glyph_rects = json_glyphs_to_rects(&json);
    let page_images = load_page_images(input_path, &pages)?;
    let glyphs = glyph_rects_to_bitmap_glyphs(&glyph_rects, &page_images)?;
    Ok(LoadedDescriptor {
        line_height: json.line_height,
        glyphs,
    })
}

fn load_descriptor(input_path: &Path) -> Result<LoadedDescriptor, CliError> {
    let ext = input_path
        .extension()
        .and_then(|v| v.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    match ext.as_str() {
        "fnt" => load_bmfont_descriptor(input_path),
        "json" => load_json_descriptor(input_path),
        _ => Err(CliError::Parse(
            "unsupported descriptor extension; use .fnt or .json".to_string(),
        )),
    }
}

fn run(args: Args) -> Result<(), CliError> {
    let loaded = load_descriptor(&args.input)?;

    let family_name = args
        .family_name
        .or_else(|| {
            args.input
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "BitmapFont".to_string());

    let config = FontConfig {
        family_name,
        line_height: args.line_height.unwrap_or(loaded.line_height),
        scale: args.scale,
    };

    let ttf = build_ttf(&loaded.glyphs, &config).map_err(|e| CliError::Convert(e.to_string()))?;

    if let Some(parent) = args.output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(args.output, ttf)?;
    Ok(())
}

fn main() {
    let args = Args::parse();
    if let Err(error) = run(args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
