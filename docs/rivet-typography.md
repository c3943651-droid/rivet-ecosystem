# rivet-typography — Definitive documentation

Rivet's **fonts and text** subsystem.

- **Version:** 2.0 (definitive)
- **Date:** 2026-10-11
- **Layer:** 2.5 (text support over canvas)
- **Depends on:** `rivet-base`; consumed by `rivet-canvas` / `rivet-ui`.
- **Umbrella document:** `../RIVET.md`

---

## 1. Purpose

Load fonts, **rasterize glyphs**, and **compose/"shape"** text so that `rivet-canvas` can draw it as a texture. It solves: font discovery, TrueType/OpenType parsing, character→glyph mapping (`cmap`), outline extraction, rasterization to mask/atlas, text measurement, and caching.

## 2. Components

| Module | Responsibility |
|---|---|
| `loader` | Load fonts (file, memory), validate, packaged fallback. |
| `parser` | TTF/OTF tables (`head`, `hhea`, `hmtx`, `cmap`, `glyf`, `CFF `, `kern`, `GPOS`...). |
| `cmap` | Character → glyph ID (formats 4/12...). |
| `outline` | Outlines (`glyf`/CFF) → polygons. |
| `rasterizer` | Outlines → mask (analytical or `stb`-like). |
| `metrics` | Advance, *advance width*, *kerning*, *ascent*/`descent`, *line height*. |
| `cache` | LRU glyph cache keyed by (font, size, subpixel). |
| `texture` | Glyph atlas → texture for canvas. |
| `font` | `Font` object (font + size + features), chained fallback. |

## 3. Font discovery (ADR-05)

- **Discovery with `fontdb`** (**pure Rust** crate): scans standard directories and the system's embedded fonts.
- **`fontconfig` (C) is NOT used**: that dependency is avoided.
- **Packaged fallback:** at least one fallback font is embedded in the binary to guarantee readable text without depending on the system. Configurable fallback chain.
- **Zero GUI C.**

## 4. Parsing and rasterization

- Table parsing with a Rust crate (`ttf-parser` or a pure-Rust equivalent).
- `cmap`: support formats 4 and 12 (BMP and full Unicode).
- `glyf` outlines (TrueType) and `CFF` (PostScript OpenType).
- Rasterization with **analytical coverage** (antialiasing), mask caching.
- **Light hinting** (v0.3): below 20px the outlines are grid-fitted before coverage — the two sides of each stem (near-vertical straight edges) are snapped together to integer columns preserving their width, and near-horizontal edges are snapped to integer rows. This keeps ~1px stems crisp instead of split across two columns. Full TrueType *hinting bytecode* remains out of scope.

## 5. Shaping and bidi (scope)

- **v1:** **simple** shaping — advance width + basic table `kern`; sufficient for Latin and many cases.
- **Complex cases** (Arabic, Indic, contextual ligatures via full `GSUB`/`GPOS`) and **bidi (RTL)** are **out of v1** (v1.x/v2 horizon, evaluating a Rust shaping engine such as `rustybuzz`).
- It is accepted that complex script rendering in v1 is not correct; this is documented as a limitation.

## 6. Cache and atlas

- **LRU cache** keyed by (font_id, pixel_size, subpixel_bin).
- **Glyph atlas** packed into one or more textures; incremental upload to GPU.
- **VRAM:** on the order of **KB per glyph** (e.g. ~32px), not MB. Total atlas size is measured.
- Canvas's `FrameStats` exposes rasterized glyphs per page.

## 7. Public API (indicative)

```rust
pub struct FontDb;                       // discovery and loading
pub struct Font;                         // font + size + features
pub struct TextLayout;                   // text → positioned glyphs
pub struct Glyph { id, advance, bearing, atlas_rect }

impl FontDb { pub fn load_system(&mut self); pub fn load_file(&mut self, p: &Path) -> Result<FontId>; }
impl Font { pub fn layout(&self, text: &str) -> TextLayout; pub fn metrics(&self) -> FontMetrics; }
```

## 8. Dependencies

- **Pure Rust:** `fontdb` (discovery), `ttf-parser` (parsing), custom or Rust-crate rasterizer; `unicode` for basic segmentation.
- **System C:** none (no `fontconfig`, no `freetype` in v1).
- Packaged fallback font (compatible license).

## 9. Scope

**v1:** `fontdb`, TrueType/OpenType, `cmap` 4/12, `glyf`/`CFF`, antialiased rasterization, metrics + `kern`, simple shaping (LTR), LRU cache, atlas, packaged fallback.

**v1.x/v2:** complex shaping and bidi (`rustybuzz`), hinting, variable fonts, contextual ligatures, vertical text.

## 10. Decisions and corrections applied

- Fonts with **`fontdb`** and **without `fontconfig`** (ADR-05).
- Complex shaping **explicitly out** of v1 (do not claim full support).
- VRAM per glyph in **KB**.
- No GUI C dependencies.

## 11. Metrics (targets, to be measured — `../RIVET.md` §12)

Font load/parse time, rasterization ms per glyph, cache hit ratio, RAM per loaded font, atlas VRAM, (subjective) quality at small sizes. Method: `criterion` + atlas telemetry.

- **v0.3 (pixel-exact text):** the render pipeline is proven 1:1 (screen pixels == CPU coverage, verified at 14/48px). Subpixel positioning (¼px bins) is disabled below 20px (`SUBPIXEL_MIN_SIZE`): fractional pens split stroke edges across two columns (perceived "double stroke"); small text uses integer glyph positions. Below the same threshold the outlines get **light hinting** (`grid_fit`): stem edges are snapped to integer columns preserving width (sharpness of `l`/`i`/`t` rises from ~0.6 to ~0.9 on a Σc²/Σc metric), horizontal edges to integer rows. Coverage is gamma-corrected (1/2.2) so thin strokes don't wash out on dark backgrounds. Glyph quads are grid-snapped (`x.round()`, `baseline.round()`).
