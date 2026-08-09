use super::glyphs::{GlyphBbox, GlyphBuildData};
use super::{Error, i32_to_i16, u32_to_i32, u32_to_u16, u64_to_i64};
use std::time::{SystemTime, UNIX_EPOCH};
use write_fonts::tables::head::Flags;

#[derive(Clone, Copy)]
pub(super) struct HorizontalMetricsSummary {
    pub(super) head_bbox: GlyphBbox,
    pub(super) advance_width_max: u16,
    pub(super) min_lsb: i16,
    pub(super) min_rsb: i16,
    pub(super) x_max_extent: i16,
}

pub(super) struct FontGlobalMetrics {
    pub(super) units_per_em: u16,
    pub(super) ascender: i16,
    pub(super) descender: i16,
    pub(super) now_mac_epoch: i64,
    pub(super) head_flags: Flags,
    pub(super) summary: HorizontalMetricsSummary,
}

pub(super) fn prepare_global_metrics(
    line_height: u32,
    baseline: i16,
    scale: u32,
    glyph_data: &GlyphBuildData,
) -> Result<FontGlobalMetrics, Error> {
    let units_per_em = u32_to_u16(line_height.saturating_mul(scale), "units_per_em to u16")?;
    let line_height_i32 = u32_to_i32(line_height, "line height to i32")?;
    let scale_i32 = u32_to_i32(scale, "scale to i32")?;
    let ascender = i32_to_i16(
        (line_height_i32 - i32::from(baseline)).saturating_mul(scale_i32),
        "ascender to i16",
    )?;
    let descender = i32_to_i16(
        -i32::from(baseline).saturating_mul(scale_i32),
        "descender to i16",
    )?;
    let summary =
        summarize_horizontal_metrics(&glyph_data.glyph_metrics, &glyph_data.glyph_bboxes)?;
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::Conversion(error.to_string()))?
        .as_secs();
    let now_mac_epoch =
        u64_to_i64(now_unix, "unix timestamp to i64")?.saturating_add(2_082_844_800);
    Ok(FontGlobalMetrics {
        units_per_em,
        ascender,
        descender,
        now_mac_epoch,
        head_flags: head_flags(glyph_data),
        summary,
    })
}

fn head_flags(glyph_data: &GlyphBuildData) -> Flags {
    let mut flags = Flags::BASELINE_AT_Y_0;
    let all_lsb_match_xmin = glyph_data
        .glyph_metrics
        .iter()
        .zip(&glyph_data.glyph_bboxes)
        .all(|((_, lsb), (x_min, _, _, _))| *lsb == *x_min);
    if all_lsb_match_xmin {
        flags.insert(Flags::LSB_AT_X_0);
    }
    flags
}

#[allow(clippy::similar_names)]
fn summarize_horizontal_metrics(
    glyph_metrics: &[(u32, i16)],
    glyph_bboxes: &[GlyphBbox],
) -> Result<HorizontalMetricsSummary, Error> {
    let mut bbox = (0, 0, 0, 0);
    let mut bbox_initialized = false;
    let mut advance_width_max = 0;
    let mut min_lsb = i16::MAX;
    let mut min_rsb = i16::MAX;
    let mut x_max_extent = i16::MIN;
    for ((advance_width, lsb), glyph_bbox) in glyph_metrics.iter().zip(glyph_bboxes) {
        let advance = u32_to_u16(
            (*advance_width).min(u32::from(u16::MAX)),
            "advance width to u16",
        )?;
        advance_width_max = advance_width_max.max(advance);
        let width = i32::from(glyph_bbox.2) - i32::from(glyph_bbox.0);
        let right_bearing = i32::from(advance) - (i32::from(*lsb) + width);
        let right_bearing = right_bearing.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        min_lsb = min_lsb.min(*lsb);
        min_rsb = min_rsb.min(i32_to_i16(right_bearing, "right side bearing to i16")?);
        let extent = (i32::from(*lsb) + width).clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        x_max_extent = x_max_extent.max(i32_to_i16(extent, "x max extent to i16")?);
        if *glyph_bbox != (0, 0, 0, 0) || !bbox_initialized {
            bbox = include_bbox(bbox, *glyph_bbox, bbox_initialized);
            bbox_initialized = true;
        }
    }
    Ok(HorizontalMetricsSummary {
        head_bbox: bbox,
        advance_width_max,
        min_lsb,
        min_rsb,
        x_max_extent,
    })
}

fn include_bbox(aggregate: GlyphBbox, bbox: GlyphBbox, initialized: bool) -> GlyphBbox {
    if !initialized {
        return bbox;
    }
    (
        aggregate.0.min(bbox.0),
        aggregate.1.min(bbox.1),
        aggregate.2.max(bbox.2),
        aggregate.3.max(bbox.3),
    )
}
