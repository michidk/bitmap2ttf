mod builder;
mod error;
mod glyph;
mod rects;

pub use builder::build_ttf;
pub use error::Error;
pub use glyph::{BitmapGlyph, FontConfig};
pub use rects::collect_pixel_rects;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_glyph() -> BitmapGlyph {
        BitmapGlyph {
            codepoint: 65,
            width: 2,
            height: 2,
            offset_x: 0,
            offset_y: 0,
            advance_width: None,
            pixels: vec![1, 1, 1, 1],
        }
    }

    #[test]
    fn merge_rects_reduces_contour_count() {
        let pixels = vec![1, 1, 1, 1, 1, 1];
        let per_pixel = collect_pixel_rects(3, 2, &pixels, false);
        let merged = collect_pixel_rects(3, 2, &pixels, true);
        assert_eq!(per_pixel.len(), 6);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], (0, 0, 3, 2));
    }

    #[test]
    fn default_profile_generates_parseable_ttf() {
        let bytes = build_ttf(&[sample_glyph()], &FontConfig::default())
            .expect("ttf generation should succeed");

        let face = ttf_parser::Face::parse(&bytes, 0).expect("ttf should parse");
        assert!(face.number_of_glyphs() >= 2);
        assert!(face.units_per_em() > 0);
    }

    #[test]
    fn empty_glyphs_returns_error() {
        let error = build_ttf(&[], &FontConfig::default())
            .expect_err("empty glyph input should return error");
        assert!(matches!(error, Error::NoGlyphs));
    }
}
