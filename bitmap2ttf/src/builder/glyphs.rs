use super::{Error, i32_to_i16, u32_to_i32, u32_to_u16, usize_to_u16};
use crate::glyph::BitmapGlyph;
use crate::rects::collect_pixel_rects;
use kurbo::BezPath;
use write_fonts::tables::glyf::{Bbox, GlyfLocaBuilder, Glyph, SimpleGlyph};
use write_fonts::types::GlyphId;

pub(super) type GlyphBbox = (i16, i16, i16, i16);
type BitmapGlyphBuild = (Glyph, GlyphBbox, u16);

#[derive(Default)]
pub(super) struct GlyphBuildData {
    pub(super) glyph_metrics: Vec<(u32, i16)>,
    pub(super) glyph_names: Vec<String>,
    pub(super) cmap_entries: Vec<(u16, u16)>,
    pub(super) cmap_mappings: Vec<(char, GlyphId)>,
    pub(super) glyph_bboxes: Vec<GlyphBbox>,
    pub(super) max_points: u16,
    pub(super) max_contours: u16,
}

pub(super) fn init_glyph_data(
    line_height: u32,
    scale: u32,
    glyf_builder: &mut GlyfLocaBuilder,
) -> Result<GlyphBuildData, Error> {
    let mut data = GlyphBuildData::default();
    let units_per_em = line_height.saturating_mul(scale);
    let notdef = make_notdef_glyph(units_per_em, scale)?;
    glyf_builder
        .add_glyph(&notdef)
        .map_err(|error| Error::Conversion(error.to_string()))?;
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

pub(super) fn append_bitmap_glyphs(
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
            && let Some(character) = char::from_u32(glyph.codepoint)
        {
            data.cmap_mappings
                .push((character, GlyphId::new(u32::from(glyph_id))));
        }
        data.glyph_names.push(format!("u{:04X}", glyph.codepoint));
        append_bitmap_glyph(glyph, line_height, scale, glyf_builder, data, scale_i32)?;
    }
    Ok(())
}

fn append_bitmap_glyph(
    glyph: &BitmapGlyph,
    line_height: u32,
    scale: u32,
    glyf_builder: &mut GlyfLocaBuilder,
    data: &mut GlyphBuildData,
    scale_i32: i32,
) -> Result<(), Error> {
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
        .map_err(|error| Error::Conversion(error.to_string()))?;
    let advance = glyph.advance_width.unwrap_or(glyph.width.saturating_add(1));
    let x_advance = u32::from(advance).saturating_mul(scale);
    let lsb = i32_to_i16(
        i32::from(glyph.offset_x).saturating_mul(scale_i32),
        "left side bearing to i16",
    )?;
    data.glyph_metrics.push((x_advance.max(scale), lsb));
    Ok(())
}

#[allow(clippy::similar_names)]
fn build_bitmap_glyph(
    glyph: &BitmapGlyph,
    line_height: u32,
    scale: u32,
) -> Result<BitmapGlyphBuild, Error> {
    let dimensions = (u32::from(glyph.width), u32::from(glyph.height));
    let offsets = (i32::from(glyph.offset_x), i32::from(glyph.offset_y));
    let mut path = BezPath::new();
    let contours = append_glyph_rects(
        &mut path,
        glyph,
        dimensions,
        offsets,
        u32_to_i32(line_height, "line height to i32")?,
        f64::from(scale),
    )?;
    if contours == 0 {
        return Ok((Glyph::Simple(SimpleGlyph::default()), (0, 0, 0, 0), 0));
    }
    let mut simple = SimpleGlyph::from_bezpath(&path)
        .map_err(|_| Error::Conversion("failed to build glyph from path".to_string()))?;
    if simple.bbox == Bbox::default() {
        simple.recompute_bounding_box();
    }
    let points = simple
        .contours
        .iter()
        .map(write_fonts::tables::glyf::Contour::len)
        .sum();
    let point_count = usize_to_u16(points, "glyph point count to u16")?;
    let bbox = (
        simple.bbox.x_min,
        simple.bbox.y_min,
        simple.bbox.x_max,
        simple.bbox.y_max,
    );
    Ok((Glyph::Simple(simple), bbox, point_count))
}

fn append_glyph_rects(
    path: &mut BezPath,
    glyph: &BitmapGlyph,
    dimensions: (u32, u32),
    offsets: (i32, i32),
    line_height: i32,
    scale: f64,
) -> Result<u16, Error> {
    let (width, height) = dimensions;
    let (x_offset, y_offset) = offsets;
    let mut contours = 0_u16;
    for (x, y, width, height) in collect_pixel_rects(width, height, &glyph.pixels, true) {
        let x = u32_to_i32(x, "glyph x to i32")?;
        let y = u32_to_i32(y, "glyph y to i32")?;
        let width = u32_to_i32(width, "glyph width to i32")?;
        let height = u32_to_i32(height, "glyph height to i32")?;
        let top = line_height - y_offset - y;
        push_rect(
            path,
            f64::from(x_offset + x) * scale,
            f64::from(top - height) * scale,
            f64::from(x_offset + x + width) * scale,
            f64::from(top) * scale,
        );
        contours = contours.saturating_add(1);
    }
    Ok(contours)
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
