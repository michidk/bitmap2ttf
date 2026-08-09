use bitmap2ttf::{FontConfig, build_ttf};
use clap::Parser;
use std::path::PathBuf;
use thiserror::Error;

mod descriptor;

use descriptor::load_descriptor;

#[derive(Parser, Debug)]
#[command(name = "bitmap2ttf")]
#[command(about = "Convert bitmap font descriptors to TrueType")]
struct Args {
    #[arg(help = "Input descriptor (.fnt BMFont text or .json PNG+JSON)")]
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

#[derive(Error, Debug)]
enum CliError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image decode error: {0}")]
    Image(#[from] image::ImageError),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Descriptor parse error: {0}")]
    Parse(String),
    #[error("TTF conversion error: {0}")]
    Convert(String),
}

fn run(args: Args) -> Result<(), CliError> {
    let loaded = load_descriptor(&args.input)?;

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
        line_height: args.line_height.unwrap_or(loaded.line_height),
        scale: args.scale,
    };

    let ttf = build_ttf(&loaded.glyphs, &config).map_err(|e| CliError::Convert(e.to_string()))?;

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
