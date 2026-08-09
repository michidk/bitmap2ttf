use super::metrics::FontGlobalMetrics;
use super::names::build_name_records;
use super::{Error, GlyphBuildData, i16_to_u16_non_negative, u32_to_i16, u32_to_u16, u64_to_i16};
use write_fonts::FontBuilder;
use write_fonts::tables::cmap::Cmap;
use write_fonts::tables::gasp::{Gasp, GaspRange, GaspRangeBehavior};
use write_fonts::tables::head::{Head, MacStyle};
use write_fonts::tables::hhea::Hhea;
use write_fonts::tables::hmtx::Hmtx;
use write_fonts::tables::maxp::Maxp;
use write_fonts::tables::name::Name;
use write_fonts::tables::os2::{Os2, SelectionFlags};
use write_fonts::tables::post::Post;
use write_fonts::tables::vmtx::LongMetric;
use write_fonts::types::{FWord, Fixed, LongDateTime, Tag, UfWord};

pub(super) struct PrimaryTableConfig<'a> {
    pub(super) family_name: &'a str,
    pub(super) line_height: u32,
    pub(super) scale: u32,
    pub(super) num_glyphs: u16,
    pub(super) index_to_loc_format: i16,
}

pub(super) fn add_primary_tables(
    builder: &mut FontBuilder,
    glyph_data: &GlyphBuildData,
    metrics: &FontGlobalMetrics,
    config: &PrimaryTableConfig,
) -> Result<(), Error> {
    add_head_table(builder, metrics, config)?;
    add_table!(builder, &Name::new(build_name_records(config.family_name)))?;
    add_table!(
        builder,
        &build_os2_table(glyph_data, config.line_height, metrics, config.scale)?
    )?;
    add_gasp_table(builder)?;
    add_maxp_table(builder, glyph_data, config.num_glyphs)
}

fn add_head_table(
    builder: &mut FontBuilder,
    metrics: &FontGlobalMetrics,
    config: &PrimaryTableConfig,
) -> Result<(), Error> {
    let bbox = metrics.summary.head_bbox;
    let head = Head::new(
        Fixed::from_f64(1.0),
        0,
        metrics.head_flags,
        metrics.units_per_em,
        LongDateTime::new(metrics.now_mac_epoch),
        LongDateTime::new(metrics.now_mac_epoch),
        bbox.0,
        bbox.1,
        bbox.2,
        bbox.3,
        MacStyle::empty(),
        8,
        config.index_to_loc_format,
    );
    add_table!(builder, &head)?;
    Ok(())
}

fn add_gasp_table(builder: &mut FontBuilder) -> Result<(), Error> {
    add_table!(
        builder,
        &Gasp {
            version: 1,
            num_ranges: 1,
            gasp_ranges: vec![GaspRange {
                range_max_ppem: u16::MAX,
                range_gasp_behavior: GaspRangeBehavior::GASP_DOGRAY
                    | GaspRangeBehavior::GASP_SYMMETRIC_SMOOTHING,
            }],
        }
    )?;
    Ok(())
}

fn add_maxp_table(
    builder: &mut FontBuilder,
    glyph_data: &GlyphBuildData,
    num_glyphs: u16,
) -> Result<(), Error> {
    add_table!(
        builder,
        &Maxp {
            num_glyphs,
            max_points: Some(glyph_data.max_points.max(4)),
            max_contours: Some(glyph_data.max_contours.max(1)),
            max_composite_points: Some(0),
            max_composite_contours: Some(0),
            max_zones: Some(2),
            max_twilight_points: Some(0),
            max_storage: Some(0),
            max_function_defs: Some(0),
            max_instruction_defs: Some(0),
            max_stack_elements: Some(0),
            max_size_of_instructions: Some(0),
            max_component_elements: Some(0),
            max_component_depth: Some(0),
        }
    )?;
    Ok(())
}

fn build_os2_table(
    glyph_data: &GlyphBuildData,
    line_height: u32,
    metrics: &FontGlobalMetrics,
    scale: u32,
) -> Result<Os2, Error> {
    let half_height = line_height.saturating_mul(scale) / 2;
    let bbox = metrics.summary.head_bbox;
    Ok(Os2 {
        x_avg_char_width: average_advance(&glyph_data.glyph_metrics, line_height, scale)?,
        y_subscript_x_size: u32_to_i16(half_height, "y_subscript_x_size to i16")?,
        y_subscript_y_size: u32_to_i16(half_height, "y_subscript_y_size to i16")?,
        y_subscript_y_offset: u32_to_i16(half_height / 2, "y_subscript_y_offset to i16")?,
        y_superscript_x_size: u32_to_i16(half_height, "y_superscript_x_size to i16")?,
        y_superscript_y_size: u32_to_i16(half_height, "y_superscript_y_size to i16")?,
        y_superscript_y_offset: u32_to_i16(half_height, "y_superscript_y_offset to i16")?,
        y_strikeout_size: u32_to_i16(scale, "y_strikeout_size to i16")?,
        y_strikeout_position: u32_to_i16(half_height, "y_strikeout_position to i16")?,
        ul_unicode_range_1: 1,
        ach_vend_id: Tag::new(b"B2TF"),
        fs_selection: SelectionFlags::REGULAR,
        us_first_char_index: glyph_data.cmap_entries.first().map_or(32, |(c, _)| *c),
        us_last_char_index: glyph_data.cmap_entries.last().map_or(126, |(c, _)| *c),
        s_typo_ascender: metrics.ascender,
        s_typo_descender: metrics.descender,
        us_win_ascent: i16_to_u16_non_negative(
            metrics.ascender.max(bbox.3).max(1),
            "us_win_ascent to u16",
        )?,
        us_win_descent: metrics
            .descender
            .unsigned_abs()
            .max(bbox.1.unsigned_abs())
            .max(1),
        ul_code_page_range_1: Some(1),
        ul_code_page_range_2: Some(0),
        sx_height: Some(metrics.ascender * 7 / 10),
        s_cap_height: Some(metrics.ascender),
        us_default_char: Some(0),
        us_break_char: Some(32),
        us_max_context: Some(0),
        ..Os2::default()
    })
}

fn average_advance(
    glyph_metrics: &[(u32, i16)],
    line_height: u32,
    scale: u32,
) -> Result<i16, Error> {
    if glyph_metrics.is_empty() {
        return u32_to_i16(
            line_height.saturating_mul(scale),
            "average advance fallback to i16",
        );
    }
    let sum: u64 = glyph_metrics
        .iter()
        .map(|(advance, _)| u64::from(*advance))
        .sum();
    let divisor = u64::try_from(glyph_metrics.len())
        .map_err(|error| Error::Conversion(format!("glyph metric count to u64 failed: {error}")))?;
    let max = u64::try_from(i16::MAX)
        .map_err(|error| Error::Conversion(format!("i16 max to u64 failed: {error}")))?;
    u64_to_i16((sum / divisor).min(max), "average advance to i16")
}

pub(super) fn add_layout_tables(
    builder: &mut FontBuilder,
    glyph_data: &GlyphBuildData,
    metrics: &FontGlobalMetrics,
    num_glyphs: u16,
    scale: u32,
) -> Result<(), Error> {
    let glyph_names: Vec<&str> = glyph_data.glyph_names.iter().map(String::as_str).collect();
    let mut post = Post::new_v2(glyph_names);
    post.underline_position = FWord::new(-u32_to_i16(scale, "underline position to i16")?);
    post.underline_thickness = FWord::new(u32_to_i16(scale / 2, "underline thickness to i16")?);
    add_table!(builder, &post)?;

    let cmap = Cmap::from_mappings(glyph_data.cmap_mappings.clone())
        .map_err(|error| Error::Conversion(format!("failed to build cmap: {error}")))?;
    add_table!(builder, &cmap)?;

    let summary = metrics.summary;
    add_table!(
        builder,
        &Hhea::new(
            FWord::new(metrics.ascender),
            FWord::new(metrics.descender),
            FWord::new(0),
            UfWord::new(summary.advance_width_max),
            FWord::new(summary.min_lsb),
            FWord::new(summary.min_rsb),
            FWord::new(summary.x_max_extent),
            1,
            0,
            0,
            num_glyphs,
        )
    )?;
    add_table!(
        builder,
        &Hmtx::new(build_hmtx_metrics(&glyph_data.glyph_metrics)?, vec![])
    )?;
    Ok(())
}

fn build_hmtx_metrics(glyph_metrics: &[(u32, i16)]) -> Result<Vec<LongMetric>, Error> {
    glyph_metrics
        .iter()
        .map(|(advance_width, lsb)| {
            let advance = u32_to_u16(
                (*advance_width).min(u32::from(u16::MAX)),
                "hmtx advance width to u16",
            )?;
            Ok(LongMetric::new(advance, *lsb))
        })
        .collect()
}
