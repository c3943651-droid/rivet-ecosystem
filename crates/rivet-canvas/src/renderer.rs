//! `GraphicsContext`: orchestrates the frame (preparation → resources → sorting
//! → rendering → presentation) per `docs/rivet-canvas.md` §2.

use glow::HasContext;

use crate::blur::BlurPipeline;
use crate::color::Color;
use crate::error::Result;
use crate::geometry::{push_quad, Rect, Transform};
use crate::queue::{Batch, BlurCommand, RenderQueue, ShaderKind};
use crate::shader::ShaderManager;
use crate::stats::{FrameStats, FrameTimer};
use crate::texture::{TextureId, TextureManager, TextureRegion};

/// Formato de vértice: pos(2) uv(2) color(4) = 8 × f32 = 32 B.
const STRIDE: i32 = 8 * 4;

/// Contexto gráfico de una superficie.
///
/// No posee la superficie: recibe el framebuffer (por defecto el de
/// `rivet-window` o el que el dueño fije con [`GraphicsContext::set_target`])
/// e intercambia buffers vía callback de presentación.
pub struct GraphicsContext {
    gl: glow::Context,
    width: i32,
    height: i32,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ebo: glow::Buffer,
    shaders: ShaderManager,
    textures: TextureManager,
    blur: Option<BlurPipeline>,
    blur_size: (i32, i32),
    target: Option<glow::Framebuffer>,
    ortho: [f32; 16],
    current_program: Option<glow::Program>,
    scissor_on: bool,
    present_cb: Option<Box<dyn FnMut()>>,
}

impl GraphicsContext {
    /// Crea el contexto desde un cargador de funciones GL/EGL.
    ///
    /// `loader` resuelve símbolos GL (`eglGetProcAddress` en el flujo con
    /// `rivet-window`).
    pub fn from_loader<F>(loader: F, width: u32, height: u32) -> Result<Self>
    where
        F: FnMut(&str) -> *const core::ffi::c_void,
    {
        // SAFETY: el llamador garantiza que `loader` resuelve símbolos GL
        // válidos para el contexto GL/EGL vigente del hilo.
        let gl = unsafe { glow::Context::from_loader_function(loader) };
        Self::from_context(gl, width, height)
    }

    /// Crea el contexto desde un `glow::Context` ya inicializado.
    pub fn from_context(gl: glow::Context, width: u32, height: u32) -> Result<Self> {
        let vao = unsafe { gl.create_vertex_array() }
            .map_err(|e| crate::error::CanvasError::ContextCreation(e.to_string()))?;
        let vbo = unsafe { gl.create_buffer() }
            .map_err(|e| crate::error::CanvasError::ContextCreation(e.to_string()))?;
        let ebo = unsafe { gl.create_buffer() }
            .map_err(|e| crate::error::CanvasError::ContextCreation(e.to_string()))?;
        unsafe {
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ebo));
            gl.enable_vertex_attrib_array(0); // a_pos
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, STRIDE, 0);
            gl.enable_vertex_attrib_array(1); // a_uv
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, STRIDE, 2 * 4);
            gl.enable_vertex_attrib_array(2); // a_color
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, STRIDE, 4 * 4);
            // Blending premultiplicado (docs §4: colores premultiplicados).
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        }
        let shaders = ShaderManager::new(&gl)?;
        let this = Self {
            gl,
            width: width as i32,
            height: height as i32,
            vao,
            vbo,
            ebo,
            shaders,
            textures: TextureManager::new(),
            blur: None,
            blur_size: (0, 0),
            target: None,
            ortho: ortho_matrix(width as i32, height as i32),
            current_program: None,
            scissor_on: false,
            present_cb: None,
        };
        Ok(this)
    }

    /// Fija el framebuffer de render (`None` = framebuffer por defecto/superficie).
    pub fn set_target(&mut self, fbo: Option<glow::Framebuffer>) {
        self.target = fbo;
        self.current_program = None;
    }

    /// Callback de presentación (intercambio de buffers del dueño de la
    /// superficie); lo llama [`GraphicsContext::present`].
    pub fn set_present_callback(&mut self, cb: Box<dyn FnMut()>) {
        self.present_cb = Some(cb);
    }

    /// Dimensiones del target.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    /// Cambia el tamaño del target (p. ej. tras un resize de ventana).
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width as i32;
        self.height = height as i32;
        self.ortho = ortho_matrix(self.width, self.height);
        // Forzar re-envío de la proyección: use_program la omite si el
        // programa no cambió, y tras un resize la matriz ya es otra.
        self.current_program = None;
        self.textures.reset_bindings();
    }

    /// Sube una textura RGBA8 y devuelve su ID.
    pub fn create_texture(&mut self, width: u32, height: u32, rgba: &[u8]) -> Result<TextureId> {
        self.textures.create(&self.gl, width, height, rgba)
    }

    /// Elimina una textura por ID.
    pub fn remove_texture(&mut self, id: TextureId) {
        self.textures.remove(&self.gl, id);
    }

    /// Crea un framebuffer offscreen con textura de color (`w×h`).
    ///
    /// Para render a target propio (lectura de píxeles, tests, wl_shm).
    /// Fija con [`GraphicsContext::set_target`].
    pub fn create_render_target(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<(glow::Framebuffer, TextureId)> {
        let texture = self.textures.create(
            &self.gl,
            width,
            height,
            &vec![0u8; (width * height * 4) as usize],
        )?;
        let fbo = unsafe { self.gl.create_framebuffer() }
            .map_err(|e| crate::error::CanvasError::ContextCreation(e.to_string()))?;
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            self.gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(self.textures.native_handle(texture).expect("textura viva")),
                0,
            );
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, self.target);
        }
        Ok((fbo, texture))
    }

    /// Destruye un render target creado con `create_render_target`.
    pub fn destroy_render_target(&mut self, fbo: glow::Framebuffer, texture: TextureId) {
        unsafe { self.gl.delete_framebuffer(fbo) };
        self.remove_texture(texture);
    }

    /// Actualiza una subregión de una textura (subida incremental).
    pub fn update_texture(
        &mut self,
        id: TextureId,
        region: TextureRegion,
        rgba: &[u8],
    ) -> Result<()> {
        self.textures.update_sub(&self.gl, id, region, rgba)
    }

    /// Referencia de solo lectura al gestor de texturas.
    #[must_use]
    pub fn textures(&self) -> &TextureManager {
        &self.textures
    }

    /// Renderiza la cola de un frame y devuelve la telemetría.
    ///
    /// Fases (§2): ordenación por capa → lotes → blurs interpolados por capa.
    pub fn render(&mut self, queue: &RenderQueue) -> FrameStats {
        let timer = FrameTimer::start();
        let mut stats = FrameStats::default();

        self.bind_target();
        unsafe {
            self.gl.viewport(0, 0, self.width, self.height);
            if let Some(color) = queue.background() {
                self.gl.clear_color(color.r, color.g, color.b, color.a);
                self.gl.clear(glow::COLOR_BUFFER_BIT);
            }
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            self.gl
                .bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.ebo));
        }

        // Ordenación: lotes estables por capa; blurs por capa interpolados.
        let mut order: Vec<usize> = (0..queue.batches().len()).collect();
        order.sort_by_key(|&i| queue.batches()[i].layer);
        let mut blurs: Vec<BlurCommand> = queue.blurs().to_vec();
        blurs.sort_by_key(|b| b.layer);

        let mut blur_i = 0;
        for &i in &order {
            let layer = queue.batches()[i].layer;
            while blur_i < blurs.len() && blurs[blur_i].layer <= layer {
                self.apply_blur(blurs[blur_i], &mut stats);
                blur_i += 1;
            }
            self.draw_batch(&queue.batches()[i], &mut stats);
        }
        while blur_i < blurs.len() {
            self.apply_blur(blurs[blur_i], &mut stats);
            blur_i += 1;
        }

        // Estado limpio para el próximo frame (clear sin scissor activo).
        if self.scissor_on {
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
            self.scissor_on = false;
        }

        stats.batches = queue.batches().len();
        stats.vertices = queue.vertex_count();
        stats.texture_uploads = self.textures.take_frame_uploads();
        timer.finish(&mut stats);
        stats
    }

    /// Ejecuta el intercambio de buffers (vía callback del dueño).
    pub fn present(&mut self) {
        if let Some(cb) = self.present_cb.as_mut() {
            cb();
        }
    }

    /// Lee un rectángulo de píxeles RGBA8 del target (coordenadas GL: origen
    /// abajo-izquierda). Útil para tests y capturas.
    #[must_use]
    pub fn read_pixels(&self, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        self.read_pixels_into(x, y, w, h, &mut buf);
        buf
    }

    /// Como [`GraphicsContext::read_pixels`], pero escribe en un búfer
    /// reutilizable del llamador (cero alocaciones por frame).
    ///
    /// `buf` debe medir exactamente `w × h × 4` bytes.
    pub fn read_pixels_into(&self, x: i32, y: i32, w: u32, h: u32, buf: &mut [u8]) {
        let len = (w as usize) * (h as usize) * 4;
        assert_eq!(
            buf.len(),
            len,
            "read_pixels_into: tamaño de búfer incorrecto"
        );
        unsafe {
            self.gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER, self.target);
            self.gl.read_pixels(
                x,
                y,
                w as _,
                h as _,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf[..len])),
            );
        }
    }

    fn bind_target(&mut self) {
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, self.target);
        }
    }

    fn use_program(&mut self, kind: ShaderKind) {
        let program = self.shaders.program(kind);
        if self.current_program != Some(program) {
            unsafe { self.gl.use_program(Some(program)) };
            self.current_program = Some(program);
            self.shaders.set_projection(&self.gl, kind, &self.ortho);
        }
    }

    fn draw_batch(&mut self, batch: &Batch, stats: &mut FrameStats) {
        // Recorte por lote (v0.2): scissor GL con Y invertida; un lote con
        // clip degenerado (intersección vacía) no se dibuja.
        match batch.clip {
            Some(clip) if clip.w > 0.0 && clip.h > 0.0 => {
                let y = self.height as f32 - (clip.y + clip.h);
                let (x, y) = (clip.x.max(0.0) as i32, y.max(0.0) as i32);
                let (w, h) = (clip.w as i32, clip.h as i32);
                unsafe {
                    self.gl.enable(glow::SCISSOR_TEST);
                    self.gl.scissor(x, y, w, h);
                }
                self.scissor_on = true;
            }
            Some(_) => {
                if self.scissor_on {
                    unsafe {
                        self.gl.disable(glow::SCISSOR_TEST);
                    }
                    self.scissor_on = false;
                }
                return; // clip vacío: nada visible de este lote
            }
            None => {
                if self.scissor_on {
                    unsafe {
                        self.gl.disable(glow::SCISSOR_TEST);
                    }
                    self.scissor_on = false;
                }
            }
        }
        self.use_program(batch.kind);
        if let Some(texture) = batch.texture {
            if self.textures.bind(&self.gl, texture, 0).is_ok() {
                self.shaders.set_sampler(&self.gl, batch.kind, 0);
            }
        }
        self.upload_geometry(&batch.vertices, &batch.indices);
        unsafe {
            self.gl.draw_elements(
                glow::TRIANGLES,
                batch.indices.len() as _,
                glow::UNSIGNED_INT,
                0,
            );
        }
        stats.draw_calls += 1;
    }

    fn upload_geometry(&mut self, vertices: &[f32], indices: &[u32]) {
        // SAFETY: reinterpretar &[f32]/&[u32] como bytes es válido (mismos
        // bytes, alineación de u8 es la más débil).
        let vbytes =
            unsafe { std::slice::from_raw_parts(vertices.as_ptr().cast(), vertices.len() * 4) };
        let ibytes =
            unsafe { std::slice::from_raw_parts(indices.as_ptr().cast(), indices.len() * 4) };
        unsafe {
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, vbytes, glow::STREAM_DRAW);
            self.gl
                .buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, ibytes, glow::STREAM_DRAW);
        }
    }

    fn apply_blur(&mut self, cmd: BlurCommand, stats: &mut FrameStats) {
        // (Re)crea el pipeline si no existe o cambió el tamaño del target.
        if self.blur.is_none() || self.blur_size != (self.width, self.height) {
            if let Some(old) = self.blur.take() {
                old.destroy(&self.gl);
            }
            self.blur = BlurPipeline::new(&self.gl, self.width, self.height).ok();
            self.blur_size = (self.width, self.height);
        }
        let Some(pipeline) = self.blur.as_ref() else {
            return;
        };
        let (hw, hh) = pipeline.size();
        // Copiamos los handles (todos Copy) para soltar el borrow de `blur`.
        let (fbo_a, tex_a, fbo_b, tex_b) = (
            pipeline.fbo_a(),
            pipeline.tex_a(),
            pipeline.fbo_b(),
            pipeline.tex_b(),
        );

        // 1) Copia la región a desenfocar al FBO A (media resolución).
        //    Conversión Y-abajo → Y-arriba de GL para el blit.
        let y0 = self.height - cmd.rect.bottom() as i32;
        let y1 = self.height - cmd.rect.y as i32;
        unsafe {
            self.gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER, self.target);
            self.gl
                .bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo_a));
            self.gl.blit_framebuffer(
                cmd.rect.x as i32,
                y0,
                cmd.rect.right() as i32,
                y1,
                0,
                0,
                hw,
                hh,
                glow::COLOR_BUFFER_BIT,
                glow::LINEAR,
            );
        }

        // 2) Ping-pong: horizontal (A→B) y vertical (B→A).
        let radius = cmd.radius;
        let ortho_half = ortho_matrix(hw, hh);
        let blur_program = match self.shaders.blur_program(&self.gl) {
            Ok(p) => p,
            Err(_) => return,
        };
        unsafe { self.gl.use_program(Some(blur_program)) };
        self.current_program = Some(blur_program);
        self.shaders
            .set_projection(&self.gl, ShaderKind::Blur, &ortho_half);
        self.shaders.set_sampler(&self.gl, ShaderKind::Blur, 0);
        self.blur_pass(fbo_b, tex_a, (radius / hw as f32, 0.0), hw, hh);
        self.blur_pass(fbo_a, tex_b, (0.0, radius / hh as f32), hw, hh);

        // 3) Compone el resultado en el target: quad en el rect original,
        //    con V invertida (el blit/FBO trabajan en Y-arriba).
        self.bind_target();
        unsafe { self.gl.viewport(0, 0, self.width, self.height) };
        let program = self.shaders.program(ShaderKind::Textured);
        unsafe { self.gl.use_program(Some(program)) };
        self.current_program = Some(program);
        self.shaders
            .set_projection(&self.gl, ShaderKind::Textured, &self.ortho);
        self.shaders.set_sampler(&self.gl, ShaderKind::Textured, 0);
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        push_quad(
            &mut vertices,
            &mut indices,
            cmd.rect,
            Rect::new(0.0, 1.0, 1.0, -1.0),
            Color::WHITE,
            &Transform::identity(),
        );
        unsafe {
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(tex_a));
        }
        self.upload_geometry(&vertices, &indices);
        unsafe {
            self.gl
                .draw_elements(glow::TRIANGLES, 6, glow::UNSIGNED_INT, 0);
        }
        stats.draw_calls += 1;
        stats.blurs += 1;
        // Los pases internos pisaron las texturas de la unidad 0.
        self.textures.reset_bindings();
    }

    fn blur_pass(
        &mut self,
        dst: glow::Framebuffer,
        src: glow::Texture,
        dir: (f32, f32),
        hw: i32,
        hh: i32,
    ) {
        self.shaders.set_blur_direction(&self.gl, dir);
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        push_quad(
            &mut vertices,
            &mut indices,
            Rect::new(0.0, 0.0, hw as f32, hh as f32),
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::WHITE,
            &Transform::identity(),
        );
        unsafe {
            self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(dst));
            self.gl.viewport(0, 0, hw, hh);
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(src));
        }
        self.upload_geometry(&vertices, &indices);
        unsafe {
            self.gl
                .draw_elements(glow::TRIANGLES, 6, glow::UNSIGNED_INT, 0);
        }
    }
}

impl Drop for GraphicsContext {
    fn drop(&mut self) {
        if let Some(blur) = self.blur.take() {
            blur.destroy(&self.gl);
        }
        let gl = &self.gl;
        // El VAO mantiene referencias; borrar buffers primero es válido.
        unsafe {
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.vbo);
            gl.delete_buffer(self.ebo);
        }
    }
}

/// Proyección ortográfica Y-invertida (pantalla Y-abajo), column-major.
fn ortho_matrix(width: i32, height: i32) -> [f32; 16] {
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    [
        2.0 / w,
        0.0,
        0.0,
        0.0, // columna 0
        0.0,
        -2.0 / h,
        0.0,
        0.0, // columna 1
        0.0,
        0.0,
        -1.0,
        0.0, // columna 2
        -1.0,
        1.0,
        0.0,
        1.0, // columna 3
    ]
}
