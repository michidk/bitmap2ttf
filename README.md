# bitmap2ttf

[![bitmap2ttf crate](https://img.shields.io/crates/v/bitmap2ttf)](https://crates.io/crates/bitmap2ttf)
[![bitmap2ttf-cli crate](https://img.shields.io/crates/v/bitmap2ttf-cli)](https://crates.io/crates/bitmap2ttf-cli)
[![MIT License](https://img.shields.io/crates/l/bitmap2ttf)](https://choosealicense.com/licenses/mit/)
[![Continuous integration](https://github.com/michidk/bitmap2ttf/actions/workflows/ci.yml/badge.svg)](https://github.com/michidk/bitmap2ttf/actions/workflows/ci.yml)

Convert bitmap font glyphs into TrueType (.ttf) vector fonts.

Takes pixel-grid glyph data and produces valid TrueType fonts with traced vector outlines — no hinting, just clean bitmap-to-outline conversion. Useful for retro game preservation, pixel font creation, and embedding bitmap fonts in applications that require vector formats.

## Features

- Generic input: any source that provides per-glyph pixel grids + metrics
- Run-length rect merging for minimal contour counts
- Complete TTF table set: head, hhea, hmtx, maxp, cmap, name, OS/2, post, glyf/loca, gasp, DSIG
- Fallible integer conversions throughout (no panics on overflow)
- BMFont (.fnt + .png atlas) and PNG+JSON CLI input support

## Installation

### Library

Install [bitmap2ttf using cargo](https://crates.io/crates/bitmap2ttf):

```toml
[dependencies]
bitmap2ttf = "0.1"
```

### CLI

Install [bitmap2ttf-cli using cargo](https://crates.io/crates/bitmap2ttf-cli):

```bash
cargo install bitmap2ttf-cli
```

## Usage

### Library

```rust
use bitmap2ttf::{BitmapGlyph, FontConfig, build_ttf};

let glyphs = vec![
    BitmapGlyph {
        codepoint: 0x41, // 'A'
        width: 8,
        height: 8,
        offset_x: 0,
        offset_y: 0,
        advance_width: None,
        pixels: vec![/* 8x8 pixel data, 0=transparent, nonzero=filled */],
    },
];

let config = FontConfig {
    family_name: "MyPixelFont".to_string(),
    ..Default::default()
};

let ttf_bytes = build_ttf(&glyphs, &config)?;
std::fs::write("output.ttf", ttf_bytes)?;
```

### Integration in existing importers

Existing bitmap font importers can map their parsed glyph structures directly to `BitmapGlyph` with no extra format conversion:

```rust
use bitmap2ttf::{BitmapGlyph, FontConfig, build_ttf};

let glyphs: Vec<BitmapGlyph> = parsed_glyphs
    .iter()
    .map(|g| BitmapGlyph {
        codepoint: g.codepoint,
        width: g.width,
        height: g.height,
        offset_x: g.offset_x,
        offset_y: g.offset_y,
        advance_width: g.advance_width,
        pixels: g.pixels.clone(),
    })
    .collect();

let config = FontConfig {
    family_name: "MyBitmapFont".to_string(),
    line_height: 16,
    scale: 64,
};

let ttf = build_ttf(&glyphs, &config)?;
```

### CLI

Convert a BMFont (.fnt + .png atlas) to TrueType:

```bash
bitmap2ttf input.fnt -o output.ttf
```

Convert a PNG+JSON descriptor to TrueType:

```bash
bitmap2ttf font.json -o output.ttf
```

The CLI reads either:

- BMFont text descriptors (`.fnt`) with PNG atlas page references
- JSON descriptors (`.json`) with either `image` (single page) or `pages` (multi-page)

JSON descriptor shape:

```json
{
  "line_height": 16,
  "image": "atlas.png",
  "glyphs": [
    {
      "codepoint": 65,
      "x": 0,
      "y": 0,
      "width": 8,
      "height": 8,
      "offset_x": 0,
      "offset_y": 0,
      "advance_width": 9,
      "page": 0
    }
  ]
}
```

## How It Works

1. Each glyph's pixel grid is scanned for filled pixels
2. Adjacent filled pixels are merged into minimal rectangles (run-length rect merging)
3. Rectangles are converted to vector outlines (straight-edge contours)
4. Coordinates are transformed from bitmap space (Y-down) to TrueType space (Y-up) and scaled
5. All required TTF tables are constructed and assembled into a valid font file

The output fonts are bitmap-traced vector outlines without hinting. They render correctly at the original pixel size and can be installed system-wide, used in game engines (Unity, Godot, etc.), and previewed at [fontdrop.info](https://fontdrop.info).

## Repository Layout

```
bitmap2ttf/        # library crate (core conversion logic)
bitmap2ttf-cli/    # CLI binary (BMFont input → TTF output)
```

## License

MIT
