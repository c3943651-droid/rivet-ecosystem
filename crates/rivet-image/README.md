# rivet-image

Image decoding layer of the [Rivet](https://github.com/c3943651-droid/rivet-ecosystem)
GUI toolkit: raster and vector formats in, premultiplied RGBA out —
ready for direct texture upload by `rivet-canvas`.

## What it provides

- **Decoding**: PNG, JPEG, and WebP (via the `image` crate), SVG (via
  `resvg`), and XPM for legacy themes. One entry point:
  `decode(bytes) -> ImageData`.
- **Freedesktop icon themes** (`IconTheme`): lookup by name and size
  across the active theme, with the standard fallback chain.
- **Missing-placeholder** (`missing_icon`): a dashed-box fallback so
  the UI never renders an empty hole.
- **Transforms**: resize, crop, recolor, and `composite_over`, with
  linear filters.
- **Cache** (`ImageCache`): a bounded LRU keyed by `CacheKey` for
  decoded icons/images.

## Usage

```rust
use rivet_image::{decode, missing_icon, ImageData, IconTheme};

// From bytes (PNG/JPEG/WebP/SVG/XPM are auto-detected):
let img: ImageData = decode(include_bytes!("avatar.png"))?;
assert_eq!((img.width, img.height), (64, 64));
// img.pixels is premultiplied RGBA8, len == width * height * 4.

// System icon lookup (freedesktop theme of the session):
let theme = IconTheme::default();
if let Ok(img) = theme.lookup("edit-copy", 32, 1) {
    // ImageData, premultiplied and ready to upload
}

// Always draw something:
let placeholder = missing_icon(32, 1);
```

## Notes

- Depends only on `rivet-base`. The heavy lifting is delegated to
  mature pure-Rust crates (`image`, `resvg`); the crate's own code is
  the theme resolution, XPM, transforms, and cache.
- Consumed by `rivet-canvas` (texture upload) and `rivet-ui` (the
  `Image` widget).

## License

MIT OR Apache-2.0.
