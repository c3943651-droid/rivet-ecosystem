//! Recursos FBO para el blur gaussiano separable (ping-pong).
//!
//! `docs/rivet-canvas.md` §3: en v1 no hay compute shaders; el blur se hace
//! con dos FBO a **media resolución** y dos pases (horizontal + vertical),
//! que es factible a 60 FPS (a medir, §14).

use glow::HasContext;

use crate::error::{CanvasError, Result};

/// Par de FBOs + texturas RGBA8 a media resolución.
pub(crate) struct BlurPipeline {
    fbo_a: glow::Framebuffer,
    tex_a: glow::Texture,
    fbo_b: glow::Framebuffer,
    tex_b: glow::Texture,
    /// Ancho de los FBOs (media resolución).
    w: i32,
    /// Alto de los FBOs (media resolución).
    h: i32,
}

impl BlurPipeline {
    /// Crea el par ping-pong a media resolución del target indicado.
    pub(crate) fn new(gl: &glow::Context, target_w: i32, target_h: i32) -> Result<Self> {
        let w = (target_w / 2).max(1);
        let h = (target_h / 2).max(1);
        let (fbo_a, tex_a) = create_fbo(gl, w, h)?;
        let (fbo_b, tex_b) = create_fbo(gl, w, h)?;
        Ok(Self {
            fbo_a,
            tex_a,
            fbo_b,
            tex_b,
            w,
            h,
        })
    }

    /// Tamaño actual de los FBOs.
    pub(crate) fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    pub(crate) fn fbo_a(&self) -> glow::Framebuffer {
        self.fbo_a
    }

    pub(crate) fn fbo_b(&self) -> glow::Framebuffer {
        self.fbo_b
    }

    pub(crate) fn tex_a(&self) -> glow::Texture {
        self.tex_a
    }

    pub(crate) fn tex_b(&self) -> glow::Texture {
        self.tex_b
    }

    /// Libera los recursos GL.
    pub(crate) fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo_a);
            gl.delete_texture(self.tex_a);
            gl.delete_framebuffer(self.fbo_b);
            gl.delete_texture(self.tex_b);
        }
    }
}

fn create_fbo(gl: &glow::Context, w: i32, h: i32) -> Result<(glow::Framebuffer, glow::Texture)> {
    let texture =
        unsafe { gl.create_texture() }.map_err(|e| CanvasError::TextureCreation(e.to_string()))?;
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
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(vec![0u8; (w * h * 4) as usize].as_slice())),
        );
    }
    let fbo = unsafe { gl.create_framebuffer() }
        .map_err(|e| CanvasError::TextureCreation(e.to_string()))?;
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(texture),
            0,
        );
        // Sin depth/stencil: solo color para efectos 2D.
    }
    Ok((fbo, texture))
}
