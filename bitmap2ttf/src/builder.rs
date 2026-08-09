use crate::error::Error;
use crate::glyph::{BitmapGlyph, FontConfig};
use write_fonts::FontBuilder;
use write_fonts::tables::glyf::GlyfLocaBuilder;
use write_fonts::tables::loca::LocaFormat;
use write_fonts::types::Tag;

const EMPTY_DSIG: [u8; 8] = [0, 0, 0, 1, 0, 0, 0, 0];

macro_rules! add_table {
    ($builder:expr, $table:expr) => {
        $builder
            .add_table($table)
            .map_err(|e| Error::Conversion(e.to_string()))
    };
}

mod glyphs;
mod metrics;
mod names;
mod tables;

use glyphs::{GlyphBuildData, append_bitmap_glyphs, init_glyph_data};
use metrics::prepare_global_metrics;
use tables::{PrimaryTableConfig, add_layout_tables, add_primary_tables};

fn usize_to_u16(value: usize, context: &str) -> Result<u16, Error> {
    u16::try_from(value).map_err(|e| Error::Conversion(format!("{context}: {e}")))
}

fn u32_to_u16(value: u32, context: &str) -> Result<u16, Error> {
    u16::try_from(value).map_err(|e| Error::Conversion(format!("{context}: {e}")))
}

fn u32_to_i32(value: u32, context: &str) -> Result<i32, Error> {
    i32::try_from(value).map_err(|e| Error::Conversion(format!("{context}: {e}")))
}

fn u64_to_i64(value: u64, context: &str) -> Result<i64, Error> {
    i64::try_from(value).map_err(|e| Error::Conversion(format!("{context}: {e}")))
}

fn i32_to_i16(value: i32, context: &str) -> Result<i16, Error> {
    i16::try_from(value).map_err(|e| Error::Conversion(format!("{context}: {e}")))
}

fn u32_to_i16(value: u32, context: &str) -> Result<i16, Error> {
    let as_i32 = u32_to_i32(value, context)?;
    i32_to_i16(as_i32, context)
}

fn u64_to_i16(value: u64, context: &str) -> Result<i16, Error> {
    i16::try_from(value).map_err(|e| Error::Conversion(format!("{context}: {e}")))
}

fn i16_to_u16_non_negative(value: i16, context: &str) -> Result<u16, Error> {
    u16::try_from(value).map_err(|e| Error::Conversion(format!("{context}: {e}")))
}

fn baseline_from_line_height(line_height: u32) -> Result<i16, Error> {
    let line_height_i32 = u32_to_i32(line_height, "line height to i32")?;
    i32_to_i16((line_height_i32 / 5).max(1), "baseline to i16")
}

pub fn build_ttf(glyphs: &[BitmapGlyph], config: &FontConfig) -> Result<Vec<u8>, Error> {
    let line_height = u32::from(config.line_height.max(1));
    let scale = config.scale.max(1);
    let baseline = baseline_from_line_height(line_height)?;

    let mut builder = FontBuilder::new();
    let mut glyf_builder = GlyfLocaBuilder::new();

    let mut glyph_data = init_glyph_data(line_height, scale, &mut glyf_builder)?;
    append_bitmap_glyphs(
        glyphs,
        line_height,
        scale,
        &mut glyf_builder,
        &mut glyph_data,
    )?;

    if glyph_data.cmap_entries.is_empty() {
        return Err(Error::NoGlyphs);
    }

    glyph_data.cmap_entries.sort_by_key(|(cp, _)| *cp);

    let metrics = prepare_global_metrics(line_height, baseline, scale, &glyph_data)?;

    let (glyf, loca, loca_format) = glyf_builder.build();
    let index_to_loc_format = match loca_format {
        LocaFormat::Short => 0,
        LocaFormat::Long => 1,
    };

    let num_glyphs = usize_to_u16(glyph_data.glyph_names.len(), "num_glyphs to u16")?;
    let primary_config = PrimaryTableConfig {
        family_name: &config.family_name,
        line_height,
        scale,
        num_glyphs,
        index_to_loc_format,
    };
    add_primary_tables(&mut builder, &glyph_data, &metrics, &primary_config)?;
    add_layout_tables(&mut builder, &glyph_data, &metrics, num_glyphs, scale)?;

    add_table!(builder, &glyf)?;
    add_table!(builder, &loca)?;
    builder.add_raw(Tag::new(b"DSIG"), EMPTY_DSIG.to_vec());

    Ok(builder.build())
}
