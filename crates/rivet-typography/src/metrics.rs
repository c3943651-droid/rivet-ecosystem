//! Métricas tipográficas escaladas a píxeles.
//!
//! Fuente de datos: tablas `head`/`hhea`/`hmtx` vía ttf-parser.

use ttf_parser::GlyphId;

/// Métricas de una fuente a un tamaño concreto (píxeles, Y hacia abajo).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    /// Tamaño solicitado en píxeles.
    pub size_px: f32,
    /// Unidades por em de la fuente (típicamente 1000 o 2048).
    pub units_per_em: u16,
    /// Ascenso por encima de la línea base (positivo).
    pub ascent: f32,
    /// Descenso bajo la línea base (positivo hacia abajo).
    pub descent: f32,
    /// Separación recomendada entre líneas.
    pub line_gap: f32,
}

impl FontMetrics {
    /// Alto de línea recomendado (ascent + descent + gap).
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.ascent + self.descent + self.line_gap
    }

    /// Escala unidades-de-fuente → píxeles para el tamaño dado.
    #[must_use]
    pub fn px_per_unit(&self) -> f32 {
        self.size_px / self.units_per_em.max(1) as f32
    }
}

/// Avance horizontal de un glifo en píxeles.
pub(crate) fn glyph_advance_px(face: &ttf_parser::Face<'_>, size_px: f32, glyph: GlyphId) -> f32 {
    let upem = face.units_per_em().max(1) as f32;
    let units = face.glyph_hor_advance(glyph).unwrap_or(0) as f32;
    units * size_px / upem
}

/// Métricas de la cara escaladas.
pub(crate) fn metrics_of(face: &ttf_parser::Face<'_>, size_px: f32) -> FontMetrics {
    let upem = face.units_per_em().max(1) as f32;
    let k = size_px / upem;
    FontMetrics {
        size_px,
        units_per_em: face.units_per_em().max(1),
        // ttf-parser: descender es negativo (bajo la línea base).
        ascent: face.ascender() as f32 * k,
        descent: -(face.descender() as f32) * k,
        line_gap: face.line_gap() as f32 * k,
    }
}
