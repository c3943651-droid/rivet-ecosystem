//! Caché LRU de imágenes por (clave, tamaño, escala, tinte).
//!
//! Doc §6: evita re-decodificar/re-rasterizar; los SVG se rasterizan al
//! tamaño pedido y se cachean bajo esa clave exacta.

use std::collections::HashMap;

use crate::data::ImageData;

/// Entrada con marca temporal lógica para el desalojo LRU.
struct Entry {
    image: ImageData,
    tick: u64,
}

/// Clave de caché: (identificador, tamaño, escala, tinte opcional).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Identificador libre (ruta, nombre de icono, URL…).
    pub key: String,
    /// Tamaño lógico en píxeles.
    pub size: u32,
    /// Escala HiDPI.
    pub scale: u32,
    /// Tinte aplicado (iconos simbólicos); `None` sin recolor.
    pub tint: Option<[u8; 4]>,
}

impl CacheKey {
    /// Clave simple para bitmaps decodificados por ruta.
    #[must_use]
    pub fn new(key: impl Into<String>, size: u32, scale: u32) -> Self {
        Self {
            key: key.into(),
            size,
            scale,
            tint: None,
        }
    }

    /// Clave con tinte (simbólicos).
    #[must_use]
    pub fn tinted(key: impl Into<String>, size: u32, scale: u32, tint: [u8; 4]) -> Self {
        Self {
            key: key.into(),
            size,
            scale,
            tint: Some(tint),
        }
    }
}

/// Caché con capacidad limitada y estadísticas de acierto.
pub struct ImageCache {
    map: HashMap<CacheKey, Entry>,
    tick: u64,
    capacity: usize,
    hits: u64,
    misses: u64,
}

impl ImageCache {
    /// Caché con capacidad máxima de imágenes.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::new(),
            tick: 0,
            capacity: capacity.max(1),
            hits: 0,
            misses: 0,
        }
    }

    /// Busca una entrada, actualizando su marca LRU.
    #[must_use]
    pub fn get(&mut self, key: &CacheKey) -> Option<ImageData> {
        self.tick = self.tick.wrapping_add(1);
        match self.map.get_mut(key) {
            Some(entry) => {
                entry.tick = self.tick;
                self.hits += 1;
                Some(entry.image.clone())
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// Inserta (o reemplaza) una entrada, desalojando al LRU si hace falta.
    pub fn put(&mut self, key: CacheKey, image: ImageData) {
        self.tick = self.tick.wrapping_add(1);
        if self.map.len() >= self.capacity && !self.map.contains_key(&key) {
            if let Some(oldest) = self
                .map
                .iter()
                .min_by_key(|(_, e)| e.tick)
                .map(|(k, _)| k.clone())
            {
                self.map.remove(&oldest);
            }
        }
        self.map.insert(
            key,
            Entry {
                image,
                tick: self.tick,
            },
        );
    }

    /// Consulta o inserta con el cargador dado (patrón get-or-insert).
    #[must_use]
    pub fn get_or_insert_with(
        &mut self,
        key: CacheKey,
        loader: impl FnOnce() -> ImageData,
    ) -> ImageData {
        if let Some(image) = self.get(&key) {
            return image;
        }
        let image = loader();
        self.put(key, image.clone());
        image
    }

    /// Entradas almacenadas.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// `true` si está vacía.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Aciertos acumulados.
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Fallos acumulados.
    #[must_use]
    pub fn misses(&self) -> u64 {
        self.misses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lru_y_estadisticas() {
        let mut cache = ImageCache::new(2);
        let a = CacheKey::new("a", 16, 1);
        let b = CacheKey::new("b", 16, 1);
        let c = CacheKey::new("c", 16, 1);
        cache.put(a.clone(), ImageData::solid(1, 1, [1, 2, 3, 255]));
        cache.put(b.clone(), ImageData::solid(1, 1, [4, 5, 6, 255]));
        assert!(cache.get(&a).is_some()); // `a` vuelve a ser reciente
        assert!(cache.get(&b).is_some());
        cache.put(c, ImageData::solid(1, 1, [7, 8, 9, 255])); // desaloja... a o b
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.hits(), 2);
        assert_eq!(cache.misses(), 0);
        assert!(cache.get(&CacheKey::new("z", 9, 1)).is_none());
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn get_or_insert() {
        let mut cache = ImageCache::new(4);
        let key = CacheKey::new("icono", 32, 1);
        let first =
            cache.get_or_insert_with(key.clone(), || ImageData::solid(2, 2, [9, 9, 9, 255]));
        let second = cache.get_or_insert_with(key, || ImageData::solid(9, 9, [1, 1, 1, 255]));
        assert_eq!(first, second, "segunda llamada reutiliza");
        assert_eq!(cache.hits(), 1);
    }
}
