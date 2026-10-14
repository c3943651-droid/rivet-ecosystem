//! Rivet Typography: de TrueType/OpenType a glifos rasterizados + métricas.
//!
//! Descubrimiento con `fontdb` (sin fontconfig, ADR-05), parseo con
//! `ttf-parser` (`cmap` 4/12, `glyf`/CFF), rasterizado propio con cobertura
//! analítica (De Casteljau + scanline), kerning básico (`kern` formato 0),
//! caché LRU y atlas de glifos para `rivet-canvas`.
//!
//! Limitaciones v1 (doc §5): shaping simple LTR; bidi/GSUB/GPOS completos,
//! hinting y fuentes variables quedan para v1.x/v2.

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
