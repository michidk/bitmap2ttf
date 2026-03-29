use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Conversion error: {0}")]
    Conversion(String),
    #[error("No mappable glyphs provided")]
    NoGlyphs,
}
