# rivet-typography — Definitive documentation

Rivet's **fonts and text** subsystem.

- **Version:** 2.0 (definitive)
- **Date:** 2026-10-11
- **Layer:** 2.5 (text support over canvas)
- **Depends on:** `rivet-foundation`; consumed by `rivet-canvas` / `rivet-ui`.
- **Replaces:** Pango + FriBidi + HarfBuzz (partially, see §7).
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
- **Known limitation:** complex TrueType *hinting* (`bytecode`) is out of the initial scope; reduced sharpness at small sizes is accepted and compensated with subpixel rasterization/high PPEM.

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

## 11. Metrics (targets, to be measured — `../RIVET.md` §14)

Font load/parse time, rasterization ms per glyph, cache hit ratio, RAM per loaded font, atlas VRAM, (subjective) quality at small sizes. Method: `criterion` + atlas telemetry.
