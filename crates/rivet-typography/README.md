# rivet-typography

Typography engine of the [Rivet](https://github.com/c3943651-droid/rivet-ecosystem)
GUI toolkit: font database, glyph rasterization with subpixel
positioning, shaping of Latin scripts, kerning, and a GPU-ready glyph
atlas.

## What it provides

- **Font database** (`FontDb`): system font discovery (via `fontdb`),
  explicit file loading, and a bundled [Inter](https://rsms.me/inter/)
  Regular (SIL OFL) so text works with zero system fonts.
- **Font model** (`Font`/`FontFace`): per-size metrics, `layout()` with
  kerning and subpixel advance, fallback chains across faces.
- **Rasterizer**: analytic De Casteljau curve flattening with a
  scanline coverage accumulator — no FreeType, no C.
- **Glyph cache** (`GlyphCache`) **and atlas** (`GlyphAtlas`): 512²
  pages with 1 px padding, ready for texture upload by `rivet-canvas`.

## Usage

```rust
use rivet_typography::FontDb;

let mut db = FontDb::new();
db.load_system();                      // system fonts
let id = db.load_bundled()?;            // or the bundled Inter

let font = db.font(id, 16.0)?;
let layout = font.layout("Hello, Rivet");
println!("advance: {:.2} px", layout.width);
```

For rendering, pair it with `rivet-canvas`: rasterize at the atlas,
upload the page once, then draw textured quads per glyph. `rivet-ui`'s
`TextRenderer` packages that loop.

## Notes

- Depends only on `rivet-base`. Text shaping is Latin-script in v1;
  full shaping (BiDi, complex scripts) is future work.
- The glyph rasterizer is benchmarked at 145.7/189.7/274.5 µs per glyph
  at 16/32/64 px (see
  [`docs/RIVET.md` §12](../../docs/RIVET.md)).
- The bundled font and its OFL license ship inside the crate.

## License

MIT OR Apache-2.0 for the code; Inter Regular under the SIL Open Font
License 1.1 (`assets/OFL-LICENSE.txt`).
