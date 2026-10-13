//! Caché LRU de glifos rastrerizados.
//!
//! `docs/rivet-typography.md` §6: clave (cara, tamaño, glifo, bin subpíxel)
//! con desalojo del menos usado recientemente. v1 usa bin subpíxel 0
//! (posiciones enteras); el campo existe para v1.x.

use std::collections::HashMap;

use ttf_parser::GlyphId;

use crate::raster::{rasterize, RasterizedGlyph};

/// Clave de caché.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    /// Hash identificador de la cara (contraparte de fuente).
    face: FaceKey,
    /// Tamaño en píxeles.
    size_px: u32,
    /// Glifo.
    glyph: u16,
    /// Bin subpíxel horizontal (v1: siempre 0).
    subpixel: u8,
}

/// Identificador de cara para la caché (deduplica por bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FaceKey(pub(crate) u64);

/// Generador de claves de cara a partir de los bytes.
pub(crate) fn face_key(data: &[u8], index: u32) -> FaceKey {
    // FNV-1a rápido: solo para deduplicar, no criptográfico.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data.iter().take(4096) {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    FaceKey(h ^ (index as u64).rotate_left(56))
}

/// Caché con desalojo LRU por marca de tiempo lógica.
pub struct GlyphCache {
    map: HashMap<CacheKey, (RasterizedGlyph, u64)>,
    tick: u64,
    capacity: usize,
    stats_hits: u64,
    stats_misses: u64,
}

impl GlyphCache {
    /// Caché con capacidad máxima de glifos.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            tick: 0,
            capacity: capacity.max(1),
            stats_hits: 0,
            stats_misses: 0,
        }
    }

    /// Obtiene (o rastreriza bajo demanda) un glifo.
    pub fn get_or_raster(
        &mut self,
        face_data: &[u8],
        face_index: u32,
        face: &ttf_parser::Face<'_>,
        size_px: f32,
        glyph: GlyphId,
        subpixel: u8,
    ) -> Option<RasterizedGlyph> {
        let key = CacheKey {
            face: face_key(face_data, face_index),
            size_px: size_px.to_bits(),
            glyph: glyph.0,
            subpixel,
        };
        self.tick = self.tick.wrapping_add(1);
        if let Some(entry) = self.map.get_mut(&key) {
            entry.1 = self.tick;
            self.stats_hits += 1;
            return Some(entry.0.clone());
        }
        self.stats_misses += 1;
        let rasterized = rasterize(face, glyph, size_px)?;
        if self.map.len() >= self.capacity {
            self.evict_oldest();
        }
        self.map.insert(key, (rasterized.clone(), self.tick));
        Some(rasterized)
    }

    /// Aciertos de caché acumulados.
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.stats_hits
    }

    /// Fallos acumulados.
    #[must_use]
    pub fn misses(&self) -> u64 {
        self.stats_misses
    }

    /// Glifos en caché.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// `true` si está vacía.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest) = self
            .map
            .iter()
            .min_by_key(|(_, (_, tick))| *tick)
            .map(|(k, _)| *k)
        {
            self.map.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_desaloja_al_mas_viejo() {
        let mut cache = GlyphCache::new(2);
        let key = |n: u16| CacheKey {
            face: FaceKey(1),
            size_px: 32f32.to_bits(),
            glyph: n,
            subpixel: 0,
        };
        let mut order = 0u64;
        let mut touch = |cache: &mut GlyphCache, k: CacheKey| {
            order += 1;
            cache.tick = order;
            cache.map.insert(k, (dummy(), order));
        };
        touch(&mut cache, key(1));
        touch(&mut cache, key(2));
        // Uso el 1 de nuevo (más reciente).
        touch(&mut cache, key(1));
        // Insertar un tercero debe desalojar al 2.
        cache.evict_oldest();
        assert!(cache.map.contains_key(&key(1)));
        assert!(!cache.map.contains_key(&key(2)));
    }

    fn dummy() -> RasterizedGlyph {
        RasterizedGlyph {
            width: 1,
            height: 1,
            left: 0.0,
            top: 0.0,
            coverage: vec![255],
        }
    }
}
