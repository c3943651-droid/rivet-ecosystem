//! Rivet Image: image decoding, SVG rasterization and icon resolution by
//! freedesktop theme.
//!
//! PNG/JPEG/WebP with the `image` crate, SVG with `resvg` (all pure Rust;
//! no GdkPixbuf or `librsvg`). Premultiplied RGBA8 output ready for
//! canvas texturing (doc §3/§9).

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod cache;
pub mod data;
pub mod decode;
pub mod error;
pub mod icon;
pub mod missing;
pub mod svg;
pub mod theme;
pub mod transform;
pub mod xpm;

pub use cache::{CacheKey, ImageCache};
pub use data::ImageData;
pub use decode::{decode, decode_file, encode_png, Format};
pub use error::{ImageError, Result};
pub use icon::IconTheme;
pub use missing::missing_icon;
pub use svg::render as render_svg;
pub use transform::{composite_over, crop, recolor, resize, Filter};
pub use xpm::decode_xpm;
