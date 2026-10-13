//! Gestión de texturas RGBA con slab de IDs y contabilidad de VRAM.

use std::collections::HashMap;

use glow::HasContext;

use rivet_foundation::slab::Slab;

use crate::error::{CanvasError, Result};

/// Identificador de textura (ID del slab del [`TextureManager`]).
pub type TextureId = u64;

/// Subregión de una textura (en píxeles, origen arriba-izquierda).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureRegion {
    /// Esquina X.
    pub x: u32,
    /// Esquina Y.
    pub y: u32,
    /// Ancho.
    pub w: u32,
    /// Alto.
    pub h: u32,
}

struct TextureEntry {
    texture: glow::Texture,
    width: u32,
    height: u32,
    vram: usize,
}

/// Fuente de píxeles CPU exportable a textura (imágenes, atlas de glifos…).
pub trait TextureSource {
    /// Dimensiones en píxeles.
    fn size(&self) -> (u32, u32);
    /// Píxeles RGBA8 premultiplicados; longitud `w*h*4`.
    fn rgba(&self) -> &[u8];
}

/// Almacén de texturas vivas del contexto gráfico.
///
/// Usa el slab con generaciones de `rivet-foundation`: un `TextureId` de una
/// textura eliminada nunca apunta a una nueva por accidente.
pub struct TextureManager {
    slab: Slab<TextureEntry>,
    bound: HashMap<u32, TextureId>, // unidad -> última textura fijada
    vram_total: usize,
    uploads_this_frame: usize,
}

impl TextureManager {
    /// Manager vacío.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slab: Slab::new(),
            bound: HashMap::new(),
            vram_total: 0,
            uploads_this_frame: 0,
        }
    }

    /// Sube una textura RGBA8 completa y devuelve su ID.
    pub fn create(
        &mut self,
        gl: &glow::Context,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<TextureId> {
        assert!(
            rgba.len() == (width as usize) * (height as usize) * 4,
            "create: rgba debe tener w*h*4 bytes"
        );
        let texture = unsafe { gl.create_texture() }
            .map_err(|e| CanvasError::TextureCreation(e.to_string()))?;
        // SAFETY de GL gestionada por glow: los IDs son válidos tras create.
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as _,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as _,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as _,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as _,
            );
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as _,
                width as _,
                height as _,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(rgba)),
            );
        }
        let vram = rgba.len();
        let id = self.slab.insert(TextureEntry {
            texture,
            width,
            height,
            vram,
        });
        self.vram_total += vram;
        self.uploads_this_frame += 1;
        Ok(id)
    }

    /// Crea desde cualquier fuente [`TextureSource`].
    pub fn create_from(
        &mut self,
        gl: &glow::Context,
        source: &dyn TextureSource,
    ) -> Result<TextureId> {
        let (w, h) = source.size();
        self.create(gl, w, h, source.rgba())
    }

    /// Actualiza una subregión con `glTexSubImage2D`.
    pub fn update_sub(
        &mut self,
        gl: &glow::Context,
        id: TextureId,
        region: TextureRegion,
        rgba: &[u8],
    ) -> Result<()> {
        let entry = self
            .slab
            .get_mut(id)
            .ok_or(CanvasError::InvalidTexture(id))?;
        assert!(
            rgba.len() == (region.w as usize) * (region.h as usize) * 4,
            "update_sub: rgba debe tener w*h*4 bytes"
        );
        debug_assert!(region.x + region.w <= entry.width && region.y + region.h <= entry.height);
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(entry.texture));
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                region.x as _,
                region.y as _,
                region.w as _,
                region.h as _,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(rgba)),
            );
        }
        self.uploads_this_frame += 1;
        Ok(())
    }

    /// Fija la textura en una unidad de textura (evita `glBind` redundante).
    pub fn bind(&mut self, gl: &glow::Context, id: TextureId, unit: u32) -> Result<()> {
        if self.bound.get(&unit) == Some(&id) {
            return Ok(());
        }
        let entry = self.slab.get(id).ok_or(CanvasError::InvalidTexture(id))?;
        unsafe {
            gl.active_texture(glow::TEXTURE0 + unit);
            gl.bind_texture(glow::TEXTURE_2D, Some(entry.texture));
        }
        self.bound.insert(unit, id);
        Ok(())
    }

    /// Elimina una textura (libera VRAM). Idempotente con IDs obsoletos.
    pub fn remove(&mut self, gl: &glow::Context, id: TextureId) {
        if let Some(entry) = self.slab.remove(id) {
            unsafe { gl.delete_texture(entry.texture) };
            self.vram_total = self.vram_total.saturating_sub(entry.vram);
        }
        self.bound.retain(|_, &mut bound| bound != id);
    }

    /// Handle GL nativo de una textura viva (uso interno del renderer).
    pub(crate) fn native_handle(&self, id: TextureId) -> Option<glow::Texture> {
        self.slab.get(id).map(|e| e.texture)
    }

    /// Dimensiones de una textura viva.
    #[must_use]
    pub fn size(&self, id: TextureId) -> Option<(u32, u32)> {
        self.slab.get(id).map(|e| (e.width, e.height))
    }

    /// VRAM total estimada en bytes.
    #[must_use]
    pub fn vram_total(&self) -> usize {
        self.vram_total
    }

    /// Texturas vivas.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slab.len()
    }

    /// `true` si no hay texturas vivas.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slab.is_empty()
    }

    pub(crate) fn take_frame_uploads(&mut self) -> usize {
        std::mem::take(&mut self.uploads_this_frame)
    }

    pub(crate) fn reset_bindings(&mut self) {
        self.bound.clear();
    }
}

impl Default for TextureManager {
    fn default() -> Self {
        Self::new()
    }
}
