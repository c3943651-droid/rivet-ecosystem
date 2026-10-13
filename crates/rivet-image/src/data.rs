//! [`ImageData`]: píxeles RGBA8 **premultiplicados**, listos para textura.
//!
//! Doc §3/§9: salida coherente con canvas (blending `ONE,
//! ONE_MINUS_SRC_ALPHA`). Los decodificadores entregan alfa recta; la
//! conversión ocurre en `decode`/`svg`.

use crate::transform;

/// Imagen en memoria: RGBA8 premultiplicado, fila-major desde arriba.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageData {
    /// Ancho en píxeles.
    pub width: u32,
    /// Alto en píxeles.
    pub height: u32,
    /// Píxeles premultiplicados; longitud `width * height * 4`.
    pub pixels: Vec<u8>,
}

impl ImageData {
    /// Construye desde píxeles ya premultiplicados.
    ///
    /// # Panics
    ///
    /// Si `pixels` no tiene `width * height * 4` bytes.
    #[must_use]
    pub fn from_premultiplied(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        assert_eq!(
            pixels.len(),
            (width as usize) * (height as usize) * 4,
            "ImageData: tamaño de píxeles incorrecto"
        );
        Self {
            width,
            height,
            pixels,
        }
    }

    /// Dimensiones `(ancho, alto)`.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Construye desde píxeles con alfa **recta** (salida típica de
    /// decodificadores), premultiplicando.
    #[must_use]
    pub fn from_straight(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        let mut px = pixels;
        for chunk in px.chunks_exact_mut(4) {
            let a = chunk[3] as u32;
            chunk[0] = (chunk[0] as u32 * a / 255) as u8;
            chunk[1] = (chunk[1] as u32 * a / 255) as u8;
            chunk[2] = (chunk[2] as u32 * a / 255) as u8;
        }
        Self::from_premultiplied(width, height, px)
    }

    /// Crea una imagen opaca de color plano.
    #[must_use]
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let n = (width as usize) * (height as usize);
        let mut pixels = Vec::with_capacity(n * 4);
        for _ in 0..n {
            pixels.extend_from_slice(&rgba);
        }
        Self::from_premultiplied(width, height, pixels)
    }

    /// Alto.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Píxeles premultiplicados.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Escala a un tamaño nuevo con el filtro indicado.
    #[must_use]
    pub fn resize(&self, width: u32, height: u32, filter: transform::Filter) -> Self {
        transform::resize(self, width, height, filter)
    }

    /// Recorta una subregión (recortada a los límites).
    #[must_use]
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Self {
        transform::crop(self, x, y, width, height)
    }

    /// Compone esta imagen **encima** de otra del mismo tamaño (operator over).
    #[must_use]
    pub fn composite_over(&self, below: &ImageData) -> Self {
        transform::composite_over(self, below)
    }

    /// Recolorea (iconos simbólicos): RGB por el tinte, conservando alfa.
    /// `tint` es alfa recta.
    #[must_use]
    pub fn recolor(&self, tint: [u8; 4]) -> Self {
        transform::recolor(self, tint)
    }
}
