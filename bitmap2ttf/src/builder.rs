use crate::error::Error;
use crate::glyph::{BitmapGlyph, FontConfig};
use crate::rects::collect_pixel_rects;
use kurbo::BezPath;
use std::time::{SystemTime, UNIX_EPOCH};
use write_fonts::tables::cmap::Cmap;
use write_fonts::tables::gasp::{Gasp, GaspRange, GaspRangeBehavior};
use write_fonts::tables::glyf::{Bbox, GlyfLocaBuilder, Glyph, SimpleGlyph};
use write_fonts::tables::head::{Flags, Head, MacStyle};
use write_fonts::tables::hhea::Hhea;
use write_fonts::tables::hmtx::Hmtx;
use write_fonts::tables::loca::LocaFormat;
use write_fonts::tables::maxp::Maxp;
use write_fonts::tables::name::{Name, NameRecord};
use write_fonts::tables::os2::{Os2, SelectionFlags};
use write_fonts::tables::post::Post;
use write_fonts::tables::vmtx::LongMetric;
use write_fonts::types::{FWord, Fixed, GlyphId, LongDateTime, NameId, Tag, UfWord};
use write_fonts::{FontBuilder, OffsetMarker};

const EMPTY_DSIG: [u8; 8] = [0, 0, 0, 1, 0, 0, 0, 0];

macro_rules! add_table {
    ($builder:expr, $table:expr) => {
        $builder
            .add_table($table)
            .map_err(|e| Error::Conversion(e.to_string()))
    };
}

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

#[derive(Default)]
struct GlyphBuildData {
    glyph_metrics: Vec<(u32, i16)>,
    glyph_names: Vec<String>,
    cmap_entries: Vec<(u16, u16)>,
    cmap_mappings: Vec<(char, GlyphId)>,
    glyph_bboxes: Vec<(i16, i16, i16, i16)>,
    max_points: u16,
    max_contours: u16,
}

#[derive(Clone, Copy)]
struct HorizontalMetricsSummary {
    head_bbox: (i16, i16, i16, i16),
    advance_width_max: u16,
    min_lsb: i16,
    min_rsb: i16,
    x_max_extent: i16,
}

struct FontGlobalMetrics {
    units_per_em: u16,
    ascender: i16,
    descender: i16,
    now_mac_epoch: i64,
    head_flags: Flags,
    summary: HorizontalMetricsSummary,
}

type BitmapGlyphBuild = (Glyph, (i16, i16, i16, i16), u16);

fn init_glyph_data(
    line_height: u32,
    scale: u32,
    glyf_builder: &mut GlyfLocaBuilder,
) -> Result<GlyphBuildData, Error> {
    let mut data = GlyphBuildData::default();
    let units_per_em = line_height.saturating_mul(scale);
    let notdef = make_notdef_glyph(units_per_em, scale)?;
    glyf_builder
        .add_glyph(&notdef)
        .map_err(|e| Error::Conversion(e.to_string()))?;
    data.glyph_metrics.push((units_per_em, 0));
    data.glyph_names.push(".notdef".to_string());
    data.glyph_bboxes.push((
        notdef.bbox.x_min,
        notdef.bbox.y_min,
        notdef.bbox.x_max,
        notdef.bbox.y_max,
    ));
    Ok(data)
}

#[allow(clippy::similar_names)]
fn build_bitmap_glyph(
    glyph: &BitmapGlyph,
    line_height: u32,
    scale: u32,
) -> Result<BitmapGlyphBuild, Error> {
    let width = u32::from(glyph.width);
    let height = u32::from(glyph.height);
    let xoff = i32::from(glyph.offset_x);
    let yoff = i32::from(glyph.offset_y);
    let line_height_i32 = u32_to_i32(line_height, "line height to i32")?;
    let scale_f64 = f64::from(scale);

    let mut path = BezPath::new();
    let mut contours = 0_u16;

    if width > 0 && height > 0 {
        for (rx, ry, rw, rh) in collect_pixel_rects(width, height, &glyph.pixels, true) {
            let rect_x_i32 = u32_to_i32(rx, "glyph rx to i32")?;
            let rect_w_i32 = u32_to_i32(rw, "glyph rw to i32")?;
            let rect_y_i32 = u32_to_i32(ry, "glyph ry to i32")?;
            let rect_h_i32 = u32_to_i32(rh, "glyph rh to i32")?;

            let left = f64::from(xoff + rect_x_i32) * scale_f64;
            let right = f64::from(xoff + rect_x_i32 + rect_w_i32) * scale_f64;

            let y_top_px = line_height_i32 - yoff - rect_y_i32;
            let y_bottom_px = y_top_px - rect_h_i32;
            let top = f64::from(y_top_px) * scale_f64;
            let bottom = f64::from(y_bottom_px) * scale_f64;

            push_rect(&mut path, left, bottom, right, top);
            contours = contours.saturating_add(1);
        }
    }

    if contours == 0 {
        return Ok((Glyph::Simple(SimpleGlyph::default()), (0, 0, 0, 0), 0));
    }

    let mut simple = SimpleGlyph::from_bezpath(&path)
        .map_err(|_| Error::Conversion("failed to build glyph from path".to_string()))?;
    if simple.bbox == Bbox::default() {
        simple.recompute_bounding_box();
    }

    let point_sum: usize = simple
        .contours
        .iter()
        .map(write_fonts::tables::glyf::Contour::len)
        .sum();
    let point_count = usize_to_u16(point_sum, "glyph point count to u16")?;

    let bbox = (
        simple.bbox.x_min,
        simple.bbox.y_min,
        simple.bbox.x_max,
        simple.bbox.y_max,
    );
    Ok((Glyph::Simple(simple), bbox, point_count))
}

fn append_bitmap_glyphs(
    glyphs: &[BitmapGlyph],
    line_height: u32,
    scale: u32,
    glyf_builder: &mut GlyfLocaBuilder,
    data: &mut GlyphBuildData,
) -> Result<(), Error> {
    let scale_i32 = u32_to_i32(scale, "scale to i32")?;

    for glyph in glyphs {
        if glyph.codepoint > u32::from(u16::MAX) {
            continue;
        }

        let glyph_id = usize_to_u16(data.glyph_names.len(), "glyph id to u16")?;
        data.cmap_entries
            .push((u32_to_u16(glyph.codepoint, "codepoint to u16")?, glyph_id));

        if glyph.codepoint != 0
            && let Some(ch) = char::from_u32(glyph.codepoint)
        {
            data.cmap_mappings
                .push((ch, GlyphId::new(u32::from(glyph_id))));
        }
        data.glyph_names.push(format!("u{:04X}", glyph.codepoint));

        let (ttf_glyph, bbox, point_count) = build_bitmap_glyph(glyph, line_height, scale)?;
        let contour_count = match &ttf_glyph {
            Glyph::Simple(simple) => usize_to_u16(simple.contours.len(), "contour count to u16")?,
            Glyph::Composite(_) | Glyph::Empty => 0,
        };

        data.glyph_bboxes.push(bbox);
        data.max_points = data.max_points.max(point_count);
        data.max_contours = data.max_contours.max(contour_count);

        glyf_builder
            .add_glyph(&ttf_glyph)
            .map_err(|e| Error::Conversion(e.to_string()))?;

        let default_advance = glyph.width.saturating_add(1);
        let advance_px = glyph.advance_width.unwrap_or(default_advance);
        let x_advance = u32::from(advance_px).saturating_mul(scale);
        let lsb = i32_to_i16(
            i32::from(glyph.offset_x).saturating_mul(scale_i32),
            "left side bearing to i16",
        )?;
        data.glyph_metrics.push((x_advance.max(scale), lsb));
    }

    Ok(())
}

#[allow(clippy::similar_names)]
fn summarize_horizontal_metrics(
    glyph_metrics: &[(u32, i16)],
    glyph_bboxes: &[(i16, i16, i16, i16)],
) -> Result<HorizontalMetricsSummary, Error> {
    let mut bbox_x_min = 0_i16;
    let mut bbox_y_min = 0_i16;
    let mut bbox_x_max = 0_i16;
    let mut bbox_y_max = 0_i16;
    let mut bbox_initialized = false;

    let mut advance_width_max = 0_u16;
    let mut min_left_side_bearing = i16::MAX;
    let mut min_right_side_bearing = i16::MAX;
    let mut x_max_extent = i16::MIN;

    for ((advance_width, lsb), (x_min, y_min, x_max, y_max)) in
        glyph_metrics.iter().zip(glyph_bboxes)
    {
        let aw_u16 = u32_to_u16(
            (*advance_width).min(u32::from(u16::MAX)),
            "advance width to u16",
        )?;
        advance_width_max = advance_width_max.max(aw_u16);

        let glyph_width = i32::from(*x_max) - i32::from(*x_min);
        let rsb_i32 = i32::from(aw_u16) - (i32::from(*lsb) + glyph_width);
        let rsb_clamped = rsb_i32.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        let rsb = i32_to_i16(rsb_clamped, "right side bearing to i16")?;

        min_left_side_bearing = min_left_side_bearing.min(*lsb);
        min_right_side_bearing = min_right_side_bearing.min(rsb);

        let extent_i32 = i32::from(*lsb) + glyph_width;
        let extent_clamped = extent_i32.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        let extent = i32_to_i16(extent_clamped, "x max extent to i16")?;
        x_max_extent = x_max_extent.max(extent);

        if (*x_min == 0 && *y_min == 0 && *x_max == 0 && *y_max == 0) && bbox_initialized {
            continue;
        }

        if bbox_initialized {
            bbox_x_min = bbox_x_min.min(*x_min);
            bbox_y_min = bbox_y_min.min(*y_min);
            bbox_x_max = bbox_x_max.max(*x_max);
            bbox_y_max = bbox_y_max.max(*y_max);
        } else {
            bbox_x_min = *x_min;
            bbox_y_min = *y_min;
            bbox_x_max = *x_max;
            bbox_y_max = *y_max;
            bbox_initialized = true;
        }
    }

    Ok(HorizontalMetricsSummary {
        head_bbox: (bbox_x_min, bbox_y_min, bbox_x_max, bbox_y_max),
        advance_width_max,
        min_lsb: min_left_side_bearing,
        min_rsb: min_right_side_bearing,
        x_max_extent,
    })
}

fn build_name_records(family_name: &str) -> Vec<NameRecord> {
    let postscript_name = sanitize_postscript_name(&format!("{family_name}-Regular"));

    let mut records = Vec::new();

    let mac_entries: &[(u16, &str)] = &[(1, family_name), (2, "Regular")];
    for &(name_id, value) in mac_entries {
        records.push(NameRecord {
            platform_id: 1,
            encoding_id: 0,
            language_id: 0,
            name_id: NameId::from(name_id),
            string: OffsetMarker::new(value.to_string()),
        });
    }

    let win_entries: Vec<(u16, String)> = vec![
        (1, family_name.to_string()),
        (2, "Regular".to_string()),
        (3, format!("{postscript_name};Version 1.0")),
        (4, format!("{family_name} Regular")),
        (5, "Version 1.0".to_string()),
        (6, postscript_name),
    ];
    for (name_id, value) in win_entries {
        records.push(NameRecord {
            platform_id: 3,
            encoding_id: 1,
            language_id: 0x0409,
            name_id: NameId::from(name_id),
            string: OffsetMarker::new(value),
        });
    }

    records
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

    let sum: u64 = glyph_metrics.iter().map(|(aw, _)| u64::from(*aw)).sum();
    let divisor = u64::try_from(glyph_metrics.len())
        .map_err(|e| Error::Conversion(format!("glyph metric count to u64 failed: {e}")))?;
    let avg = if divisor == 0 { 0 } else { sum / divisor };
    let i16_max_u64 = u64::try_from(i16::MAX)
        .map_err(|e| Error::Conversion(format!("i16 max to u64 failed: {e}")))?;
    u64_to_i16(avg.min(i16_max_u64), "average advance to i16")
}

fn build_hmtx_metrics(glyph_metrics: &[(u32, i16)]) -> Result<Vec<LongMetric>, Error> {
    glyph_metrics
        .iter()
        .map(|(advance_width, lsb)| {
            let aw_u16 = u32_to_u16(
                (*advance_width).min(u32::from(u16::MAX)),
                "hmtx advance width to u16",
            )?;
            Ok(LongMetric::new(aw_u16, *lsb))
        })
        .collect()
}

fn prepare_global_metrics(
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
        .map_err(|e| Error::Conversion(e.to_string()))?
        .as_secs();
    let now_mac_epoch =
        u64_to_i64(now_unix, "unix timestamp to i64")?.saturating_add(2_082_844_800);

    let mut head_flags = Flags::empty();
    head_flags.insert(Flags::BASELINE_AT_Y_0);
    let all_lsb_match_xmin = glyph_data
        .glyph_metrics
        .iter()
        .zip(glyph_data.glyph_bboxes.iter())
        .all(|((_, lsb), (x_min, _, _, _))| *lsb == *x_min);
    if all_lsb_match_xmin {
        head_flags.insert(Flags::LSB_AT_X_0);
    }

    Ok(FontGlobalMetrics {
        units_per_em,
        ascender,
        descender,
        now_mac_epoch,
        head_flags,
        summary,
    })
}

fn build_os2_table(
    glyph_data: &GlyphBuildData,
    line_height: u32,
    metrics: &FontGlobalMetrics,
    scale: u32,
) -> Result<Os2, Error> {
    let avg_advance = average_advance(&glyph_data.glyph_metrics, line_height, scale)?;
    Ok(Os2 {
        x_avg_char_width: avg_advance,
        us_weight_class: 400,
        us_width_class: 5,
        fs_type: 0,
        y_subscript_x_size: u32_to_i16(
            line_height.saturating_mul(scale) / 2,
            "y_subscript_x_size to i16",
        )?,
        y_subscript_y_size: u32_to_i16(
            line_height.saturating_mul(scale) / 2,
            "y_subscript_y_size to i16",
        )?,
        y_subscript_x_offset: 0,
        y_subscript_y_offset: u32_to_i16(
            line_height.saturating_mul(scale) / 4,
            "y_subscript_y_offset to i16",
        )?,
        y_superscript_x_size: u32_to_i16(
            line_height.saturating_mul(scale) / 2,
            "y_superscript_x_size to i16",
        )?,
        y_superscript_y_size: u32_to_i16(
            line_height.saturating_mul(scale) / 2,
            "y_superscript_y_size to i16",
        )?,
        y_superscript_x_offset: 0,
        y_superscript_y_offset: u32_to_i16(
            line_height.saturating_mul(scale) / 2,
            "y_superscript_y_offset to i16",
        )?,
        y_strikeout_size: u32_to_i16(scale, "y_strikeout_size to i16")?,
        y_strikeout_position: u32_to_i16(
            line_height.saturating_mul(scale) / 2,
            "y_strikeout_position to i16",
        )?,
        s_family_class: 0,
        panose_10: [0; 10],
        ul_unicode_range_1: 1,
        ul_unicode_range_2: 0,
        ul_unicode_range_3: 0,
        ul_unicode_range_4: 0,
        ach_vend_id: Tag::new(b"B2TF"),
        fs_selection: SelectionFlags::REGULAR,
        us_first_char_index: glyph_data.cmap_entries.first().map_or(32, |(c, _)| *c),
        us_last_char_index: glyph_data.cmap_entries.last().map_or(126, |(c, _)| *c),
        s_typo_ascender: metrics.ascender,
        s_typo_descender: metrics.descender,
        s_typo_line_gap: 0,
        us_win_ascent: i16_to_u16_non_negative(
            metrics.ascender.max(metrics.summary.head_bbox.3).max(1),
            "us_win_ascent to u16",
        )?,
        us_win_descent: metrics
            .descender
            .unsigned_abs()
            .max(metrics.summary.head_bbox.1.unsigned_abs())
            .max(1),
        ul_code_page_range_1: Some(1),
        ul_code_page_range_2: Some(0),
        sx_height: Some(metrics.ascender * 7 / 10),
        s_cap_height: Some(metrics.ascender),
        us_default_char: Some(0),
        us_break_char: Some(32),
        us_max_context: Some(0),
        us_lower_optical_point_size: None,
        us_upper_optical_point_size: None,
    })
}

struct PrimaryTableConfig<'a> {
    family_name: &'a str,
    line_height: u32,
    scale: u32,
    num_glyphs: u16,
    index_to_loc_format: i16,
}

fn add_primary_tables(
    builder: &mut FontBuilder,
    glyph_data: &GlyphBuildData,
    metrics: &FontGlobalMetrics,
    config: &PrimaryTableConfig,
) -> Result<(), Error> {
    let head = Head::new(
        Fixed::from_f64(1.0),
        0,
        metrics.head_flags,
        metrics.units_per_em,
        LongDateTime::new(metrics.now_mac_epoch),
        LongDateTime::new(metrics.now_mac_epoch),
        metrics.summary.head_bbox.0,
        metrics.summary.head_bbox.1,
        metrics.summary.head_bbox.2,
        metrics.summary.head_bbox.3,
        MacStyle::empty(),
        8,
        config.index_to_loc_format,
    );
    add_table!(builder, &head)?;
    add_table!(builder, &Name::new(build_name_records(config.family_name)))?;
    add_table!(
        builder,
        &build_os2_table(glyph_data, config.line_height, metrics, config.scale)?
    )?;
    add_table!(
        builder,
        &Gasp {
            version: 1,
            num_ranges: 1,
            gasp_ranges: vec![GaspRange {
                range_max_ppem: 0xFFFF,
                range_gasp_behavior: GaspRangeBehavior::GASP_DOGRAY
                    | GaspRangeBehavior::GASP_SYMMETRIC_SMOOTHING,
            }],
        }
    )?;
    add_table!(
        builder,
        &Maxp {
            num_glyphs: config.num_glyphs,
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

fn add_layout_tables(
    builder: &mut FontBuilder,
    glyph_data: &GlyphBuildData,
    metrics: &FontGlobalMetrics,
    ascender: i16,
    descender: i16,
    num_glyphs: u16,
    scale: u32,
) -> Result<(), Error> {
    let glyph_name_refs: Vec<&str> = glyph_data.glyph_names.iter().map(String::as_str).collect();
    let mut post = Post::new_v2(glyph_name_refs);
    post.underline_position = FWord::new(-u32_to_i16(scale, "underline_position scale to i16")?);
    post.underline_thickness = FWord::new(u32_to_i16(scale / 2, "underline_thickness to i16")?);
    post.is_fixed_pitch = 0;
    add_table!(builder, &post)?;

    let cmap = Cmap::from_mappings(glyph_data.cmap_mappings.clone())
        .map_err(|e| Error::Conversion(format!("failed to build cmap: {e}")))?;
    add_table!(builder, &cmap)?;

    let hhea = Hhea::new(
        FWord::new(ascender),
        FWord::new(descender),
        FWord::new(0),
        UfWord::new(metrics.summary.advance_width_max),
        FWord::new(metrics.summary.min_lsb),
        FWord::new(metrics.summary.min_rsb),
        FWord::new(metrics.summary.x_max_extent),
        1,
        0,
        0,
        num_glyphs,
    );
    add_table!(builder, &hhea)?;
    add_table!(
        builder,
        &Hmtx::new(build_hmtx_metrics(&glyph_data.glyph_metrics)?, vec![])
    )?;
    Ok(())
}

fn push_rect(path: &mut BezPath, x0: f64, y0: f64, x1: f64, y1: f64) {
    path.move_to((x0, y0));
    path.line_to((x1, y0));
    path.line_to((x1, y1));
    path.line_to((x0, y1));
    path.close_path();
}

fn make_notdef_glyph(units: u32, scale: u32) -> Result<SimpleGlyph, Error> {
    let mut path = BezPath::new();
    let side = f64::from(units.max(scale));
    push_rect(&mut path, 0.0, 0.0, side, side);
    SimpleGlyph::from_bezpath(&path)
        .map_err(|_| Error::Conversion("failed to build .notdef glyph".to_string()))
}

fn sanitize_postscript_name(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        }
        if out.len() >= 63 {
            break;
        }
    }

    if out.is_empty() {
        "BitmapFont-Regular".to_string()
    } else {
        out
    }
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
    add_layout_tables(
        &mut builder,
        &glyph_data,
        &metrics,
        metrics.ascender,
        metrics.descender,
        num_glyphs,
        scale,
    )?;

    add_table!(builder, &glyf)?;
    add_table!(builder, &loca)?;
    builder.add_raw(Tag::new(b"DSIG"), EMPTY_DSIG.to_vec());

    Ok(builder.build())
}
