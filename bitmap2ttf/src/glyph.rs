#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitmapGlyph {
    pub codepoint: u32,
    pub width: u16,
    pub height: u16,
    pub offset_x: i16,
    pub offset_y: i16,
    pub advance_width: Option<u16>,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontConfig {
    pub family_name: String,
    pub line_height: u16,
    pub scale: u32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family_name: "BitmapFont".to_string(),
            line_height: 16,
            scale: 64,
        }
    }
}
