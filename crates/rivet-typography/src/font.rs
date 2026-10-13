//! [`Font`]: cara + tamaño + cadena de fallback; layout simple de texto.

use ttf_parser::GlyphId;

use crate::face::FontFace;
use crate::kern::Kerning;
use crate::metrics::{glyph_advance_px, metrics_of, FontMetrics};
use crate::raster::RasterizedGlyph;

/// Glifo posicionado dentro de un layout.
#[derive(Debug, Clone, Copy)]
pub struct PlacedGlyph {
    /// ID del glifo en su fuente.
    pub glyph: GlyphId,
    /// Índice de la cara que lo contiene (0 = primaria).
    pub face: u8,
    /// Posición X entera del lápiz (borde izquierdo) en píxeles.
    pub x: f32,
    /// Posición Y de la línea base en píxeles.
    pub y: f32,
    /// Avance horizontal aplicado (px).
    pub advance: f32,
    /// Bin subpíxel horizontal (0..4 → offset 0, ¼, ½, ¾ px).
    pub subpixel: u8,
}

/// Cuantiza una posición X a píxel entero + bin subpíxel de ¼ px.
pub(crate) fn quantize_subpixel(x: f32) -> (f32, u8) {
    let frac = x - x.floor();
    let bin = (frac * 4.0).round();
    if bin >= 4.0 {
        (x.floor() + 1.0, 0)
    } else {
        (x.floor(), bin as u8)
    }
}

/// Texto convertido a glifos posicionados (shaping simple LTR, doc §5).
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// Glifos en orden de dibujo.
    pub glyphs: Vec<PlacedGlyph>,
    /// Ancho total en píxeles.
    pub width: f32,
}

/// Fuente lista para medir y rastrerizar: cara primaria + fallbacks.
#[derive(Debug, Clone)]
pub struct Font {
    faces: Vec<FontFace>,
    size_px: f32,
    kern: Vec<Kerning>,
}

impl Font {
    /// Crea una fuente de una sola cara.
    pub(crate) fn new(face: FontFace, size_px: f32) -> Self {
        Self::with_fallbacks(face, Vec::new(), size_px)
    }

    /// Crea una fuente con cadena de fallback (la primaria primero).
    ///
    /// Los caracteres ausentes en la cara primaria se buscan en los
    /// fallbacks en orden (doc §3: "cadena de fallback configurable").
    pub fn with_fallbacks(primary: FontFace, fallbacks: Vec<FontFace>, size_px: f32) -> Self {
        assert!(size_px > 0.0, "Font: el tamaño debe ser > 0");
        let mut faces = vec![primary];
        faces.extend(fallbacks);
        let kern = faces.iter().map(Kerning::new).collect();
        Self {
            faces,
            size_px,
            kern,
        }
    }

    /// Añade una cara de fallback al final de la cadena.
    pub fn push_fallback(&mut self, face: FontFace) {
        self.kern.push(Kerning::new(&face));
        self.faces.push(face);
    }

    /// Tamaño en píxeles.
    #[must_use]
    pub fn size_px(&self) -> f32 {
        self.size_px
    }

    /// Métricas de la cara primaria al tamaño actual.
    #[must_use]
    pub fn metrics(&self) -> FontMetrics {
        metrics_of(self.primary().face(), self.size_px)
    }

    /// Cara primaria.
    #[must_use]
    pub fn primary(&self) -> &FontFace {
        &self.faces[0]
    }

    /// Cara por índice de la cadena.
    #[must_use]
    pub fn face(&self, index: u8) -> Option<&FontFace> {
        self.faces.get(index as usize)
    }

    /// Layout simple de una línea (LTR): cmap + avance + kern.
    ///
    /// Limitaciones v1 documentadas (§5): sin bidi, sin GSUB/GPOS; los
    /// caracteres sin glifo se toman de los fallbacks o se omiten.
    pub fn layout(&self, text: &str) -> TextLayout {
        let mut glyphs = Vec::with_capacity(text.len());
        let mut x = 0.0f32;
        let mut previous: Option<(u8, GlyphId)> = None;
        for ch in text.chars() {
            // .notdef (glyph 0) si nada de la cadena lo cubre.
            let mut chosen_face = 0u8;
            let mut glyph = GlyphId(0);
            let mut found = false;
            for (i, face) in self.faces.iter().enumerate() {
                if let Some(g) = face.face().glyph_index(ch) {
                    glyph = g;
                    chosen_face = i as u8;
                    found = true;
                    break;
                }
            }
            if !found && self.primary().face().glyph_index(' ').is_some() {
                // Sin glifo en toda la cadena: usar el espacio de la primaria
                // como sustituto para conservar el ritmo del texto.
                glyph = self.primary().face().glyph_index(' ').unwrap_or(GlyphId(0));
                chosen_face = 0;
            }
            // Kerning con el glifo anterior (misma cara): GPOS primero,
            // tabla kern legada como respaldo.
            if let Some((prev_face, prev)) = previous {
                if prev_face == chosen_face {
                    let ttf_face = self.faces[chosen_face as usize].face();
                    let units = self.kern[chosen_face as usize].get(ttf_face, prev, glyph);
                    x += units as f32 * self.px_per_unit();
                }
            }
            let (pen_x, bin) = quantize_subpixel(x);
            glyphs.push(PlacedGlyph {
                glyph,
                face: chosen_face,
                x: pen_x,
                y: 0.0,
                advance: 0.0,
                subpixel: bin,
            });
            let advance =
                glyph_advance_px(self.faces[chosen_face as usize].face(), self.size_px, glyph);
            glyphs.last_mut().expect("recién empujado").advance = advance;
            x += advance;
            previous = Some((chosen_face, glyph));
        }
        TextLayout { glyphs, width: x }
    }

    /// Rastreriza un glifo del layout desde su cara (sin caché),
    /// aplicando su bin subpíxel.
    #[must_use]
    pub fn rasterize(&self, placed: PlacedGlyph) -> Option<RasterizedGlyph> {
        let face = self.faces.get(placed.face as usize)?;
        crate::raster::rasterize(
            face.face(),
            placed.glyph,
            self.size_px,
            placed.subpixel as f32 * 0.25,
        )
    }

    fn px_per_unit(&self) -> f32 {
        let upem = self.primary().face().units_per_em().max(1) as f32;
        self.size_px / upem
    }
}
