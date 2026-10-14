# rivet-image — Definitive documentation

Rivet's **images and icons** subsystem. **New crate** (did not exist in v1).

- **Version:** 2.0 (definitive)
- **Date:** 2026-10-11
- **Layer:** 2.5 (image support over canvas)
- **Depends on:** `rivet-foundation`; consumed by `rivet-canvas` / `rivet-ui`.
- **Umbrella document:** `../RIVET.md`

---

## 1. Purpose and justification

Applications need to **display images**: application icons, thumbnails, sprites, avatars, and scalable SVG. `rivet-image` provides this capability natively: it decodes formats, resolves **icons by theme**, and delivers data ready for texture upload.

## 2. Components

| Module | Responsibility |
|---|---|
| `decode` | Decode PNG / JPEG / WebP. |
| `svg` | Rasterize SVG at a given size/`scale`. |
| `icon` | Resolve icons by name from `freedesktop` themes (with fallback). |
| `theme` | Load/parse `index.theme` and search the *hicolor theme* paths. |
| `transform` | Scaling (nearest/bilinear), rotation, cropping, alpha compositing. |
| `cache` | Cache keyed by (key, size, scale). |
| `error` | `ImageError`. |

## 3. Decoding

- **PNG:** pure Rust crate (`png`).
- **JPEG:** `jpeg-decoder` (pure Rust) or `image` (Rust).
- **WebP:** `image`/`webp` (Rust).
- **SVG:** `resvg`/`usvg` (**pure Rust**) for rasterization to a bitmap; `librsvg` (C) is not used.
- **Output:** premultiplied RGBA8, ready for a canvas texture.

## 4. Icons and themes

- Resolve icons by **name** following the **`freedesktop` Icon Theme** specification:
  - Search in `$XDG_DATA_DIRS/icons`, `~/.local/share/icons`, `~/.icons`, `/usr/share/icons`.
  - Read `index.theme` (inherits from `hicolor`), sizes, and directories.
  - Search extensions by priority: `.svg` (preferred, scalable) → `.png` → `.xpm`.
- **Fallback:** a packaged "missing"/generic icon when nothing is found.
- Symbolic names (`-symbolic`) with recoloring by theme color where applicable.

## 5. Canvas integration

```rust
pub enum ImageData { Rgba { width, height, pixels: Vec<u8> }, /* ... */ }
pub struct IconTheme;
pub struct IconCache;

impl IconTheme {
    pub fn lookup(&self, name: &str, size: u32, scale: u32) -> Result<ImageData, ImageError>;
}
```

- `rivet-canvas` uploads `ImageData` to a `Texture` (`glTexSubImage2D`).
- It does **not** draw by itself; it produces pixels/resources.

## 6. Performance

- **Cache** keyed by (path/name, size, scale, tint color). Avoids re-decoding and re-rasterizing.
- Lazy/async: large decodes (thumbnails) can be deferred and notified; v1 may do this synchronously and measure.
- SVG is rasterized at the requested size (a giant SVG is not cached).

## 7. Dependencies

- **Pure Rust:** `png`, `jpeg-decoder`/`image`, `resvg`/`usvg`, color utilities.
- **System C:** none.
- No GUI C libraries; no `librsvg`.

## 8. Scope

**v1:** PNG/JPEG/WebP decode, SVG rasterize, `freedesktop` icon theme with fallback, scaling, cache.

**v1.x/v2:** extra formats (AVIF, animated GIF), XPM icons, advanced recoloring, GPU scaling, asynchronous decoding.

## 9. Decisions and corrections applied

- **New crate** justified by the seed applications (icons + images).
- All **pure Rust**; no `librsvg`/`gdk-pixbuf`.
- **Premultiplied RGBA** output, consistent with canvas.

## 10. Metrics (targets, to be measured — `../RIVET.md` §12)

PNG/JPEG/WebP decode ms, SVG rasterization ms per size, cache hit ratio and RAM, icon lookup latency. Method: `criterion`.
