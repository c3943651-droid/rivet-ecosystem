//! Parseo de la tabla `kern` (formato 0) para kerning básico.
//!
//! `docs/rivet-typography.md` §5: v1 usa avance + `kern`; `GPOS` completo
//! queda para v1.x/v2. ttf-parser no expone `kern`, así que se parsea a mano
//! desde los bytes crudos de la tabla.

use std::collections::HashMap;

use ttf_parser::GlyphId;

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
