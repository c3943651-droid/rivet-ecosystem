//! Atlas de glifos: empaqueta máscaras en páginas con *shelf packing*.
//!
//! `docs/rivet-typography.md` §6: una o más texturas, subida incremental a
//! GPU. Cada página es un bitmap de cobertura (1 byte/píxel) que canvas
//! sube como textura RGBA premultiplicada blanca.

use crate::raster::RasterizedGlyph;

/// Rectángulo de un glifo dentro del atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasRect {
    /// Página del atlas.
    pub page: usize,
    /// Esquina X en píxeles.
    pub x: u32,
    /// Esquina Y en píxeles.
    pub y: u32,
    /// Ancho.
    pub w: u32,
    /// Alto.
    pub h: u32,
}

/// Una página del atlas (bitmap de cobertura).
#[derive(Debug, Clone)]
pub struct AtlasPage {
    /// Lado de la página en píxeles (cuadrada).
    pub size: u32,
    /// Cobertura 1 byte/píxel, fila-major.
    pub data: Vec<u8>,
    /// Empaquetado por estanterías (*shelves*): cursor X de la fila actual.
    pub(crate) cursor_x: u32,
    /// Línea base Y de la estantería actual.
    pub(crate) cursor_y: u32,
    /// Altura máxima de la estantería en curso.
    pub(crate) shelf_height: u32,
}

impl AtlasPage {
    fn new(size: u32) -> Self {
        Self {
            size,
            data: vec![0; (size * size) as usize],
            cursor_x: 0,
            cursor_y: 0,
            shelf_height: 0,
        }
    }

    /// Reserva `w×h` (+padding alrededor) devolviendo la esquina útil.
    fn alloc(&mut self, w: u32, h: u32, padding: u32) -> Option<(u32, u32)> {
        let padded_w = w + padding * 2;
        let padded_h = h + padding * 2;
        if padded_w > self.size || padded_h > self.size {
            return None; // no cabe ni en una página vacía
        }
        if self.cursor_x + padded_w > self.size {
            // Estantería nueva: bajar a la siguiente franja libre.
            self.cursor_y += self.shelf_height;
            self.cursor_x = 0;
            self.shelf_height = 0;
        }
        if self.cursor_y + padded_h > self.size {
            return None; // página llena
        }
        let (x, y) = (self.cursor_x + padding, self.cursor_y + padding);
        self.cursor_x += padded_w;
        self.shelf_height = self.shelf_height.max(padded_h);
        Some((x, y))
    }
}

/// Atlas multi-página de glifos.
#[derive(Debug)]
pub struct GlyphAtlas {
    pages: Vec<AtlasPage>,
    page_size: u32,
    padding: u32,
}

impl GlyphAtlas {
    /// Atlas con páginas de `page_size × page_size` y padding entre glifos
    /// (evita sangrado al muestrear bilineal).
    #[must_use]
    pub fn new(page_size: u32, padding: u32) -> Self {
        Self {
            pages: Vec::new(),
            page_size,
            padding,
        }
    }

    /// Inserta un glifo rastrerizado y devuelve su rectánguno.
    pub fn alloc(&mut self, glyph: &RasterizedGlyph) -> AtlasRect {
        let w = glyph.width as u32;
        let h = glyph.height as u32;
        // Busca espacio en páginas existentes.
        for (page_index, page) in self.pages.iter_mut().enumerate() {
            if let Some((x, y)) = page.alloc(w, h, self.padding) {
                blit(page, x, y, glyph);
                return AtlasRect {
                    page: page_index,
                    x,
                    y,
                    w,
                    h,
                };
            }
        }
        // Página nueva.
        let mut page = AtlasPage::new(self.page_size);
        let (x, y) = page
            .alloc(w, h, self.padding)
            .expect("glifo más grande que la página");
        blit(&mut page, x, y, glyph);
        self.pages.push(page);
        AtlasRect {
            page: self.pages.len() - 1,
            x,
            y,
            w,
            h,
        }
    }

    /// Páginas actuales.
    pub fn pages(&self) -> &[AtlasPage] {
        &self.pages
    }

    /// Bytes de una página convertidos a RGBA8 premultiplicado blanco
    /// (r=g=b=a=coverage) para subir con `rivet-canvas`.
    pub fn page_rgba(&self, page: usize) -> Option<Vec<u8>> {
        let page = self.pages.get(page)?;
        let mut rgba = Vec::with_capacity(page.data.len() * 4);
        for &c in &page.data {
            rgba.extend_from_slice(&[c, c, c, c]);
        }
        Some(rgba)
    }

    /// Número de páginas.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// VRAM total estimada en bytes (una vez subida como RGBA).
    #[must_use]
    pub fn vram_estimate(&self) -> usize {
        self.pages
            .iter()
            .map(|p| (p.size * p.size * 4) as usize)
            .sum()
    }
}

fn blit(page: &mut AtlasPage, x: u32, y: u32, glyph: &RasterizedGlyph) {
    for row in 0..glyph.height {
        let dst = (y as usize + row) * page.size as usize + x as usize;
        let src = row * glyph.width;
        page.data[dst..dst + glyph.width].copy_from_slice(&glyph.coverage[src..src + glyph.width]);
    }
}
