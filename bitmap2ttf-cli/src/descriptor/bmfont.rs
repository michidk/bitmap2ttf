use super::{GlyphRect, LoadedDescriptor, glyph_rects_to_bitmap_glyphs, load_page_images};
use crate::CliError;
use std::collections::HashMap;
use std::path::Path;

struct BmfontInfo {
    line_height: u16,
    pages: HashMap<u16, String>,
    glyphs: Vec<GlyphRect>,
}

#[derive(Default)]
struct BmfontAccumulator {
    line_height: Option<u16>,
    pages: HashMap<u16, String>,
    glyphs: Vec<GlyphRect>,
}

impl BmfontAccumulator {
    fn parse_line(&mut self, line: &str) -> Result<(), CliError> {
        let mut parts = line.splitn(2, char::is_whitespace);
        let tag = parts.next().unwrap_or_default();
        let values = parse_key_values(parts.next().unwrap_or_default());
        match tag {
            "common" => self.line_height = Some(parse_u16(&values, "lineHeight")?),
            "page" => {
                let id = parse_u16(&values, "id")?;
                let file = values.get("file").ok_or_else(|| {
                    CliError::Parse("missing key 'file' on page line".to_string())
                })?;
                self.pages.insert(id, file.clone());
            }
            "char" => self.glyphs.push(parse_glyph_rect(&values)?),
            _ => {}
        }
        Ok(())
    }

    fn finish(self) -> Result<BmfontInfo, CliError> {
        let line_height = self
            .line_height
            .ok_or_else(|| CliError::Parse("missing 'common' line with lineHeight".to_string()))?;
        if self.pages.is_empty() {
            return Err(CliError::Parse("no BMFont pages found".to_string()));
        }
        if self.glyphs.is_empty() {
            return Err(CliError::Parse("no BMFont char entries found".to_string()));
        }
        Ok(BmfontInfo {
            line_height,
            pages: self.pages,
            glyphs: self.glyphs,
        })
    }
}

pub(super) fn load(input_path: &Path) -> Result<LoadedDescriptor, CliError> {
    let text = std::fs::read_to_string(input_path)?;
    let info = parse(&text)?;
    let images = load_page_images(input_path, &info.pages)?;
    Ok(LoadedDescriptor {
        line_height: info.line_height,
        glyphs: glyph_rects_to_bitmap_glyphs(&info.glyphs, &images)?,
    })
}

fn parse(contents: &str) -> Result<BmfontInfo, CliError> {
    let mut accumulator = BmfontAccumulator::default();
    for line in contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        accumulator.parse_line(line)?;
    }
    accumulator.finish()
}

fn parse_glyph_rect(values: &HashMap<String, String>) -> Result<GlyphRect, CliError> {
    Ok(GlyphRect {
        id: parse_u32(values, "id")?,
        x: parse_u32(values, "x")?,
        y: parse_u32(values, "y")?,
        width: parse_u16(values, "width")?,
        height: parse_u16(values, "height")?,
        offset_x: parse_i16(values, "xoffset")?,
        offset_y: parse_i16(values, "yoffset")?,
        advance_width: parse_u16(values, "xadvance")?,
        page: parse_u16(values, "page")?,
    })
}

fn parse_key_values(input: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let mut index = 0;
    let bytes = input.as_bytes();
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let key_start = index;
        while index < bytes.len() && bytes[index] != b'=' && !bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            index = skip_token(bytes, index);
            continue;
        }
        let key = &input[key_start..index];
        index += 1;
        let (value, next) = parse_value(input, index);
        values.insert(key.to_string(), value);
        index = next;
    }
    values
}

fn parse_value(input: &str, start: usize) -> (String, usize) {
    let bytes = input.as_bytes();
    if start >= bytes.len() {
        return (String::new(), start);
    }
    let quoted = bytes[start] == b'"';
    let value_start = start + usize::from(quoted);
    let mut end = value_start;
    while end < bytes.len()
        && if quoted {
            bytes[end] != b'"'
        } else {
            !bytes[end].is_ascii_whitespace()
        }
    {
        end += 1;
    }
    let next = end + usize::from(quoted && end < bytes.len());
    (input[value_start..end].to_string(), next)
}

fn skip_token(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn parse_u16(values: &HashMap<String, String>, key: &str) -> Result<u16, CliError> {
    parse_number(values, key, "u16")
}

fn parse_u32(values: &HashMap<String, String>, key: &str) -> Result<u32, CliError> {
    parse_number(values, key, "u32")
}

fn parse_i16(values: &HashMap<String, String>, key: &str) -> Result<i16, CliError> {
    parse_number(values, key, "i16")
}

fn parse_number<T>(values: &HashMap<String, String>, key: &str, kind: &str) -> Result<T, CliError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let raw = values
        .get(key)
        .ok_or_else(|| CliError::Parse(format!("missing key '{key}'")))?;
    raw.parse()
        .map_err(|error| CliError::Parse(format!("invalid {kind} for '{key}': {error}")))
}
