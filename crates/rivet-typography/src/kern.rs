//! Kerning horizontal: tabla `kern` (formato 0) + subset `GPOS` PairPos.
//!
//! `docs/rivet-typography.md` §5: v1 usa avance + kerning básico. Aquí hay
//! dos fuentes:
//! - `kern` (formato 0): parseada a mano (ttf-parser no la expone).
//! - `GPOS` PairPos (formatos 1 y 2): la **spec** la parsea ttf-parser
//!   (LayoutTable/PairAdjustment); la orquestación (feature `kern` →
//!   lookups → consulta de par) es nuestra.

use std::collections::HashMap;

use ttf_parser::GlyphId;

use crate::face::FontFace;
use crate::utils::{read_i16, read_u16};

/// Pares de kerning horizontales (formato 0) indexados por (izq, der).
#[derive(Debug, Clone, Default)]
pub(crate) struct KernPairs {
    map: HashMap<(u16, u16), i16>,
}

impl KernPairs {
    /// Parsea todas las subtablas horizontales de formato 0 de `kern`.
    pub(crate) fn parse(table: Option<&[u8]>) -> Self {
        let mut map = HashMap::new();
        let Some(t) = table else { return Self { map } };
        if t.len() < 4 {
            return Self { map };
        }
        // Cabecera "legacy": version u16, nTables u16.
        let mut off = 4usize;
        let n_tables = read_u16(t, 2) as usize;
        for _ in 0..n_tables {
            if off + 6 > t.len() {
                break;
            }
            let length = read_u16(t, off + 2) as usize;
            let coverage = read_u16(t, off + 4);
            // bit 0: horizontal; bits 8-15: formato (formato 0 aceptado).
            let horizontal = coverage & 0x0001 != 0;
            let format = coverage >> 8;
            if horizontal && format == 0 && off + 14 <= t.len() {
                let n_pairs = read_u16(t, off + 6) as usize;
                let mut p = off + 14;
                for _ in 0..n_pairs {
                    if p + 6 > t.len() || p >= off + length.max(14) {
                        break;
                    }
                    let left = read_u16(t, p);
                    let right = read_u16(t, p + 2);
                    let value = read_i16(t, p + 4);
                    map.insert((left, right), value);
                    p += 6;
                }
            }
            // length puede ser 0 en fuentes rotas: avanza al menos la cabecera.
            off += length.max(6).max(if off + 6 <= t.len() { 6 } else { 0 });
            if length == 0 {
                break; // evita bucles infinitos con tablas corruptas
            }
        }
        Self { map }
    }

    /// Kerning horizontal entre dos glifos en unidades de fuente.
    pub(crate) fn get(&self, left: GlyphId, right: GlyphId) -> i16 {
        self.map.get(&(left.0, right.0)).copied().unwrap_or(0)
    }
}

/// Kerning de una cara: consulta `GPOS` (moderno) primero y cae a `kern`.
#[derive(Debug, Clone, Default)]
pub(crate) struct Kerning {
    /// Pares de la tabla `kern` legada (si existe).
    pairs: KernPairs,
}

impl Kerning {
    pub(crate) fn new(face: &FontFace) -> Self {
        Self {
            pairs: KernPairs::parse(face.face().raw_face().table(KERN_TAG)),
        }
    }

    /// Kerning horizontal entre dos glifos, en unidades de fuente.
    ///
    /// `GPOS` (feature `kern`, PairPos formatos 1/2) tiene prioridad; si
    /// la fuente no lo declara, se consulta la tabla `kern` legada.
    pub(crate) fn get(&self, face: &ttf_parser::Face<'_>, left: GlyphId, right: GlyphId) -> i16 {
        if let Some(value) = gpos_kern(face, left, right) {
            return value;
        }
        self.pairs.get(left, right)
    }
}

const KERN_TAG: ttf_parser::Tag = ttf_parser::Tag::from_bytes(b"kern");

/// Kerning horizontal vía `GPOS` feature `kern` (subset PairPos).
///
/// Devuelve `None` si la fuente no declara `kern` en GPOS (no
/// confundible con "el par no existe", que devuelve `Some(0)`).
pub(crate) fn gpos_kern(face: &ttf_parser::Face<'_>, left: GlyphId, right: GlyphId) -> Option<i16> {
    let tables = face.tables();
    let gpos = tables.gpos?;
    let feature = gpos.features.find(KERN_TAG)?;
    let mut total = 0i16;
    for i in 0..feature.lookup_indices.len() {
        let lookup_index = feature.lookup_indices.get(i)?;
        let lookup = gpos.lookups.get(lookup_index)?;
        for subtable_index in 0..lookup.subtables.len() {
            let subtable = lookup
                .subtables
                .get::<ttf_parser::gpos::PositioningSubtable<'_>>(subtable_index)?;
            let ttf_parser::gpos::PositioningSubtable::Pair(pair) = subtable else {
                continue;
            };
            match pair {
                ttf_parser::gpos::PairAdjustment::Format1 { coverage, sets } => {
                    if let Some(coverage_index) = coverage.get(left) {
                        if let Some(set) = sets.get(coverage_index) {
                            if let Some((first, _second)) = set.get(right) {
                                total += first.x_advance;
                            }
                        }
                    }
                }
                ttf_parser::gpos::PairAdjustment::Format2 {
                    coverage,
                    classes,
                    matrix,
                } => {
                    if coverage.get(left).is_some() {
                        let class1 = classes.0.get(left);
                        let class2 = classes.1.get(right);
                        if let Some((first, _second)) = matrix.get((class1, class2)) {
                            total += first.x_advance;
                        }
                    }
                }
            }
        }
    }
    Some(total)
}
