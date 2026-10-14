//! Rivet Typography: from TrueType/OpenType to rasterized glyphs + metrics.
//!
//! Discovery with `fontdb` (no fontconfig, ADR-05), parsing with
//! `ttf-parser` (`cmap` 4/12, `glyf`/CFF), own rasterization with analytic
//! coverage (De Casteljau + scanline), basic kerning (`kern` format 0),
//! LRU cache, and a glyph atlas for `rivet-canvas`.
//!
//! v1 limitations (doc §5): simple LTR shaping; full bidi/GSUB/GPOS,
//! hinting, and variable fonts are deferred to v1.x/v2.

#![forbid(unsafe_op_in_unsafe_fn)]

mod atlas;
pub mod bundled;
mod cache;
mod db;
mod error;
mod face;
mod font;
mod kern;
mod metrics;
mod raster;
mod utils;

pub use atlas::{AtlasPage, AtlasRect, GlyphAtlas};
pub use bundled::{BUNDLED_FONT, BUNDLED_FONT_LICENSE};
pub use cache::GlyphCache;
pub use db::FontDb;
pub use error::{FontId, Result, TypographyError};
pub use face::FontFace;
pub use font::{Font, PlacedGlyph, TextLayout};
pub use metrics::FontMetrics;
pub use raster::RasterizedGlyph;
