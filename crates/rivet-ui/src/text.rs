//! Text on the GPU: typography (font+layout+raster) → atlas → canvas
//! texture. The first use of a glyph rasterizes and uploads its region
//! (incremental); from then on everything is indexed (doc §6 of typography).

/// Destino de dibujo de texto: contexto GL + cola del frame.
pub struct DrawTarget<'a> {
    /// Contexto gráfico (sube texturas del atlas).
    pub canvas: &'a mut GraphicsContext,
    /// Cola de comandos del frame.
    pub queue: &'a mut rivet_canvas::RenderQueue,
}

/// Trabajo de dibujo de una cadena.
#[derive(Clone, Copy)]
pub struct DrawJob<'a> {
    /// Layout ya medido.
    pub layout: &'a TextLayout,
    /// Esquina superior izquierda X.
    pub x: f32,
    /// Esquina superior izquierda Y.
    pub y: f32,
    /// Tamaño de fuente (el del layout).
    pub size: f32,
    /// Tinte (color recto de estilo).
    pub color: Color,
}

use std::collections::HashMap;

use rivet_canvas::GraphicsContext;
use rivet_style::Color;
use rivet_typography::{Font, GlyphAtlas, GlyphCache, RasterizedGlyph, TextLayout};

/// Tamaño de página del atlas de glifos (px por lado).
const ATLAS_PAGE: u32 = 512;

/// Renderizador de texto: una fuente por tamaño, caché LRU y atlas.
pub struct TextRenderer {
    font: rivet_typography::FontDb,
    fonts: HashMap<u32, Font>,
    cache: GlyphCache,
    atlas: GlyphAtlas,
    /// Textura de canvas por página del atlas.
    textures: Vec<Option<rivet_canvas::TextureId>>,
}

impl TextRenderer {
    /// Renderizador con la fuente empaquetada (Inter, OFL) — autónomo.
    #[must_use]
    pub fn new() -> Self {
        let mut font = rivet_typography::FontDb::new();
        let _ = font.load_bundled();
        Self::with_font_db(font)
    }

    /// Con una `FontDb` ya configurada (p. ej. con fuentes del sistema).
    #[must_use]
    pub fn with_font_db(font: rivet_typography::FontDb) -> Self {
        Self {
            font,
            fonts: HashMap::new(),
            cache: GlyphCache::new(4096),
            atlas: GlyphAtlas::new(ATLAS_PAGE, 1),
            textures: Vec::new(),
        }
    }

    fn font_at(&mut self, size: f32) -> &Font {
        let key = size.to_bits();
        self.fonts.entry(key).or_insert_with(|| {
            let id = self.font.sans_serif().unwrap();
            self.font.font(id, size).expect("fuente válida")
        })
    }

    /// Layout de una cadena al tamaño pedido.
    pub fn layout(&mut self, text: &str, size: f32) -> TextLayout {
        let font = self.font_at(size).clone();
        font.layout(text)
    }

    /// Alto de línea al tamaño pedido.
    #[must_use]
    pub fn line_height(&mut self, size: f32) -> f32 {
        self.font_at(size).metrics().line_height()
    }

    /// Dibuja `layout` con esquina superior izquierda en `(x, y)`.
    pub fn draw(&mut self, target: &mut DrawTarget<'_>, job: DrawJob<'_>) {
        let DrawTarget { canvas, queue } = target;
        let DrawJob {
            layout,
            x,
            y,
            size,
            color,
        } = job;
        let font = self.font_at(size).clone();
        let baseline = y + font.metrics().ascent;
        for glyph in &layout.glyphs {
            let Some(rasterized) = self.cache.get_or_raster(
                font.primary().data(),
                font.primary().index(),
                font.primary().face(),
                size,
                glyph.glyph,
                glyph.subpixel,
            ) else {
                continue;
            };
            let Some(rect) = self.atlas_alloc(canvas, &rasterized) else {
                continue;
            };
            let (page, uv) = rect;
            let Some(texture) = self.textures.get(page).copied().flatten() else {
                continue;
            };
            // Cobertura × tinte (premultiplicado en canvas).
            let tint = rivet_canvas::Color::rgba(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            );
            queue.textured_rect(
                0,
                rivet_canvas::Rect::new(
                    x + glyph.x + rasterized.left,
                    baseline - rasterized.top,
                    rasterized.width as f32,
                    rasterized.height as f32,
                ),
                texture,
                uv,
                tint,
                &rivet_canvas::Transform::identity(),
            );
        }
    }

    /// Aloja el glifo en el atlas y garantiza que su página está subida.
    /// Devuelve (página, uv normalizado).
    fn atlas_alloc(
        &mut self,
        canvas: &mut GraphicsContext,
        glyph: &RasterizedGlyph,
    ) -> Option<(usize, rivet_canvas::Rect)> {
        let rect = self.atlas.alloc(glyph);
        let page = rect.page;
        while self.textures.len() <= page {
            self.textures.push(None);
        }
        let needed = rect.w * rect.h > 0;
        if !needed {
            return None;
        }
        match self.textures.get(page).copied().flatten() {
            None => {
                // Página nueva: subida completa.
                let rgba = self.atlas.page_rgba(page)?;
                let texture = canvas.create_texture(ATLAS_PAGE, ATLAS_PAGE, &rgba).ok()?;
                self.textures[page] = Some(texture);
            }
            Some(texture) => {
                // Página existente: solo la región del glifo (incremental).
                let page_data = &self.atlas.pages().get(page)?.data;
                let stride = ATLAS_PAGE as usize;
                let mut sub = Vec::with_capacity((rect.w * rect.h * 4) as usize);
                for row in 0..rect.h as usize {
                    let base = (rect.y as usize + row) * stride + rect.x as usize;
                    for &coverage in &page_data[base..base + rect.w as usize] {
                        sub.extend_from_slice(&[coverage, coverage, coverage, coverage]);
                    }
                }
                let _ = canvas.update_texture(
                    texture,
                    rivet_canvas::TextureRegion {
                        x: rect.x,
                        y: rect.y,
                        w: rect.w,
                        h: rect.h,
                    },
                    &sub,
                );
            }
        }
        let norm = |v: u32| v as f32 / ATLAS_PAGE as f32;
        Some((
            page,
            rivet_canvas::Rect::new(norm(rect.x), norm(rect.y), norm(rect.w), norm(rect.h)),
        ))
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}
