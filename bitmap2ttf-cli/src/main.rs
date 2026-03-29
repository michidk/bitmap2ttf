use bitmap2ttf::{BitmapGlyph, FontConfig, build_ttf};
use clap::Parser;
use image::ImageReader;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Parser, Debug)]
#[command(name = "bitmap2ttf")]
#[command(about = "Convert BMFont bitmap fonts to TrueType")]
struct Args {
    #[arg(help = "Input BMFont text descriptor (.fnt)")]
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
    chars: Vec<BmfontChar>,
}

#[derive(Debug, Clone)]
struct BmfontChar {
    id: u32,
    x: u32,
    y: u32,
    width: u16,
    height: u16,
    xoffset: i16,
    yoffset: i16,
    xadvance: u16,
    page: u16,
}

#[derive(Error, Debug)]
enum CliError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image decode error: {0}")]
    Image(#[from] image::ImageError),
    #[error("BMFont parse error: {0}")]
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
                let ch = BmfontChar {
                    id: parse_u32(&map, "id")?,
                    x: parse_u32(&map, "x")?,
                    y: parse_u32(&map, "y")?,
                    width: parse_u16(&map, "width")?,
                    height: parse_u16(&map, "height")?,
                    xoffset: parse_i16(&map, "xoffset")?,
                    yoffset: parse_i16(&map, "yoffset")?,
                    xadvance: parse_u16(&map, "xadvance")?,
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
            let alpha = p[3];
            pixels.push(if alpha == 0 { 0 } else { 1 });
        }
    }
    Ok(pixels)
}

fn bmfont_to_bitmap_glyphs(
    input_path: &Path,
    info: &BmfontInfo,
) -> Result<Vec<BitmapGlyph>, CliError> {
    let base_dir = input_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut page_images: HashMap<u16, image::RgbaImage> = HashMap::new();
    for (&id, filename) in &info.pages {
        let page_path = base_dir.join(filename);
        let image = ImageReader::open(&page_path)?.decode()?.to_rgba8();
        page_images.insert(id, image);
    }

    let mut glyphs = Vec::with_capacity(info.chars.len());
    for ch in &info.chars {
        let page = page_images
            .get(&ch.page)
            .ok_or_else(|| CliError::Parse(format!("missing image for page {}", ch.page)))?;
        let pixels = extract_glyph_pixels(page, ch.x, ch.y, ch.width, ch.height)?;
        glyphs.push(BitmapGlyph {
            codepoint: ch.id,
            width: ch.width,
            height: ch.height,
            offset_x: ch.xoffset,
            offset_y: ch.yoffset,
            advance_width: Some(ch.xadvance),
            pixels,
        });
    }
    Ok(glyphs)
}

fn run(args: Args) -> Result<(), CliError> {
    let bmfont_text = std::fs::read_to_string(&args.input)?;
    let info = parse_bmfont_text(&bmfont_text)?;
    let glyphs = bmfont_to_bitmap_glyphs(&args.input, &info)?;

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
        line_height: args.line_height.unwrap_or(info.line_height),
        scale: args.scale,
    };

    let ttf = build_ttf(&glyphs, &config).map_err(|e| CliError::Convert(e.to_string()))?;

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
