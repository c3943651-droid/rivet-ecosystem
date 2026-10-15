//! `Application`: init/run/quit cycle that connects Wayland (window), GL
//! offscreen (canvas) and the UI tree (doc §10 of rivet-ui, §2 of rivet).
//!
//! v1 flow (`wl_shm`, ADR-02): window events → UI → `RenderQueue` →
//! GL to FBO → `read_pixels` → `present_rgba` (copy to the shared
//! buffer). The compositor's frame callback schedules redraws.

use rivet_canvas::{GraphicsContext, RenderQueue};
use rivet_window::{KeyEvent, Wayland, Window, WindowEvent, WindowOptions};

use crate::event::{EventDispatcher, UiInput};
use crate::layout::layout_tree;
use crate::render::render_tree;
use crate::widget::WidgetKind;
use crate::{Result, Ui, UiError};

/// Backend GL headless: mantiene vivo el contexto EGL de la app.
struct GlBackend {
    #[allow(dead_code)]
    egl: khronos_egl::DynamicInstance<khronos_egl::EGL1_4>,
    #[allow(dead_code)]
    display: khronos_egl::Display,
    #[allow(dead_code)]
    context: khronos_egl::Context,
    #[allow(dead_code)]
    surface: khronos_egl::Surface,
}

/// Estado por ventana: surface, UI propia, cola y recursos de render.
struct WindowCtx {
    /// Handle de la ventana Wayland.
    window: Window,
    /// Árbol de widgets de esta ventana (multiventana v0.3: una UI por
    /// ventana, con foco y estado independientes).
    ui: Ui,
    queue: RenderQueue,
    /// Render target (FBO + textura) de esta ventana.
    target: Option<(rivet_canvas::glow::Framebuffer, rivet_canvas::TextureId)>,
    target_size: (u32, u32),
    /// Redibujar en cuanto el compositor lo permita.
    needs_redraw: bool,
    /// Presentamos y esperamos el frame callback (re-arma el ciclo).
    frame_pending: bool,
    /// Tamaño lógico del último configure.
    logical: (u32, u32),
    /// Búfer de píxeles reutilizable (`read_pixels_into` + flip in place).
    pixel_buf: Vec<u8>,
    /// Fila temporal para voltear `pixel_buf` sin alocar (v0.2).
    flip_scratch: Vec<u8>,
}

/// Índice de una ventana dentro de la `Application`.
pub type WindowIndex = usize;

/// Aplicación multiventana (v0.3): N ventanas, cada una con su `Ui`.
///
/// ```ignore
/// let mut app = Application::new("Demo", 800, 600)?;
/// // ventana 0: app.ui_mut() (compatibilidad con una ventana)
/// let secundaria = app.create_window(WindowOptions::new(300, 200).title("Ajustes"))?;
/// // construir la UI de la secundaria: app.ui_mut_of(secundaria)
/// app.run()?;
/// ```
///
/// El bucle termina cuando cualquier UI pide [`Ui::quit`] o se cierra la
/// última ventana.
pub struct Application {
    event_loop: rivet_base::event_loop::EventLoop,
    wayland: Wayland,
    gl: GraphicsContext,
    _backend: Option<GlBackend>,
    windows: Vec<WindowCtx>,
    /// Deadline de un repintado diferido (animaciones/autoguardado).
    redraw_at: Option<std::time::Instant>,
}

impl Application {
    /// Crea la app: conecta Wayland, abre la ventana primaria y prepara GL.
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Result<Self> {
        let mut wayland = Wayland::connect().map_err(|e| UiError::Subsystem(e.to_string()))?;
        let mut event_loop = rivet_base::event_loop::EventLoop::new(None, 64)
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        let (gl, backend) =
            create_headless_gl().map_err(|e| UiError::Subsystem(format!("EGL: {e}")))?;
        wayland
            .register_in(&mut event_loop)
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        let mut app = Self {
            event_loop,
            wayland,
            gl,
            _backend: Some(backend),
            windows: Vec::new(),
            redraw_at: None,
        };
        app.push_window(WindowOptions::new(width.max(1), height.max(1)).title(title))?;
        // El commit inicial debe llegar al compositor antes de esperar en
        // epoll (si no, el configure nunca llega y el bucle bloquea).
        app.wayland
            .flush()
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        Ok(app)
    }

    fn push_window(&mut self, opts: WindowOptions) -> Result<WindowIndex> {
        let window = self
            .wayland
            .create_window(opts)
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        let logical = (
            window.logical_size().0.max(1),
            window.logical_size().1.max(1),
        );
        self.windows.push(WindowCtx {
            window,
            ui: Ui::new_dark(),
            queue: RenderQueue::new(),
            target: None,
            target_size: (0, 0),
            needs_redraw: true,
            frame_pending: false,
            logical,
            pixel_buf: Vec::new(),
            flip_scratch: Vec::new(),
        });
        Ok(self.windows.len() - 1)
    }

    /// Abre una ventana adicional con su propia UI (multiventana v0.3).
    ///
    /// Devuelve el índice para usar con [`Application::ui_mut_of`] y
    /// [`Application::close_window`].
    pub fn create_window(&mut self, opts: WindowOptions) -> Result<WindowIndex> {
        let index = self.push_window(opts)?;
        self.wayland
            .flush()
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        Ok(index)
    }

    /// Cierra una ventana por índice (idempotente).
    ///
    /// Cerrar la última ventana hace que `run` termine en el próximo ciclo.
    pub fn close_window(&mut self, index: WindowIndex) -> Result<()> {
        let Some(ctx) = self.windows.get(index) else {
            return Ok(());
        };
        let id = ctx.window.id();
        self.windows.remove(index);
        self.wayland
            .destroy_window(id)
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        Ok(())
    }

    /// Número de ventanas abiertas.
    #[must_use]
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// UI de la ventana primaria (compatibilidad con una sola ventana).
    #[must_use]
    pub fn ui(&self) -> &Ui {
        &self.windows[0].ui
    }

    /// UI mutable de la ventana primaria.
    pub fn ui_mut(&mut self) -> &mut Ui {
        &mut self.windows[0].ui
    }

    /// UI mutable de la ventana `index` (multiventana v0.3).
    pub fn ui_mut_of(&mut self, index: WindowIndex) -> &mut Ui {
        &mut self.windows[index].ui
    }

    /// Compila un tema y lo aplica a todas las ventanas.
    pub fn set_theme(&mut self, dsl: &str, theme: &str) {
        for ctx in &mut self.windows {
            if let Ok(table) = rivet_style::compile(dsl, theme) {
                ctx.ui.style = table;
            }
            ctx.needs_redraw = true;
        }
    }

    /// Programa un repintado diferido (`None` = ya) para animaciones o
    /// tareas periódicas (autoguardado).
    ///
    /// El bucle despierta a tiempo (`run_once` con timeout) aunque no
    /// llegen eventos del compositor.
    pub fn schedule_redraw_after(&mut self, delay: std::time::Duration) {
        let deadline = std::time::Instant::now() + delay;
        // El plazo más temprano gana (animaciones en cadena).
        self.redraw_at = Some(self.redraw_at.map_or(deadline, |d| d.min(deadline)));
    }

    /// Bombea una vez: espera eventos (bloqueante) y los enruta a la UI.
    ///
    /// Con un repintado diferido activo, espera como máximo hasta su
    /// deadline en vez de bloquear indefinidamente.
    pub fn pump_events(&mut self) -> Result<()> {
        let timeout_ms = self.redraw_at.map(|deadline| {
            deadline
                .saturating_duration_since(std::time::Instant::now())
                .as_millis()
                .min(u32::MAX as u128) as u64
        });
        self.event_loop
            .run_once(timeout_ms)
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        self.route_pending_events()?;
        // Deadline vencido: repintar aunque no haya eventos.
        if self
            .redraw_at
            .is_some_and(|deadline| deadline <= std::time::Instant::now())
        {
            self.redraw_at = None;
            for ctx in &mut self.windows {
                ctx.needs_redraw = true;
            }
        }
        Ok(())
    }

    /// Presenta un frame ya renderizado (público para tests/ejemplos).
    pub fn present_frame(&mut self) -> Result<()> {
        self.draw_frame(0)
    }

    /// Presenta un frame de la ventana `index` (tests/ejemplos).
    pub fn present_frame_of(&mut self, index: WindowIndex) -> Result<()> {
        self.draw_frame(index)
    }

    /// Últimos píxeles presentados de la ventana `index` (RGBA top-down,
    /// tamaño `buffer_size_of`): diagnóstico y tests de regresión.
    #[must_use]
    pub fn presented_frame_rgba(&self, index: WindowIndex) -> &[u8] {
        &self.windows[index].pixel_buf
    }

    /// Tamaño físico del buffer de la ventana `index`.
    #[must_use]
    pub fn buffer_size_of(&self, index: WindowIndex) -> (u32, u32) {
        self.windows[index].window.buffer_size()
    }

    /// `true` cuando el compositor configuró la ventana primaria.
    #[must_use]
    pub fn configured(&self) -> bool {
        self.windows
            .first()
            .is_some_and(|ctx| ctx.window.configured())
    }

    /// `true` cuando el compositor configuró la ventana `index`.
    #[must_use]
    pub fn configured_of(&self, index: WindowIndex) -> bool {
        self.windows
            .get(index)
            .is_some_and(|ctx| ctx.window.configured())
    }

    fn index_of(&self, id: rivet_window::WindowId) -> Option<WindowIndex> {
        self.windows.iter().position(|ctx| ctx.window.id() == id)
    }

    fn route_pending_events(&mut self) -> Result<()> {
        let events = self.wayland.take_events();
        for event in events {
            // La ventana afectada (0 como fallback de compatibilidad).
            let index = match &event {
                WindowEvent::Configure { window, .. }
                | WindowEvent::Close { window }
                | WindowEvent::Frame { window }
                | WindowEvent::PointerMove { window, .. }
                | WindowEvent::PointerEnter { window, .. }
                | WindowEvent::PointerLeave { window }
                | WindowEvent::PointerButton { window, .. }
                | WindowEvent::Scroll { window, .. }
                | WindowEvent::Key { window, .. }
                | WindowEvent::TouchDown { window, .. }
                | WindowEvent::TouchMotion { window, .. }
                | WindowEvent::TouchUp { window, .. }
                | WindowEvent::TouchCancel { window } => *window,
                _ => 0,
            };
            let Some(index) = self.index_of(index) else {
                continue; // evento de una ventana ya cerrada
            };
            match event {
                WindowEvent::Configure { width, height, .. } => {
                    let ctx = &mut self.windows[index];
                    ctx.logical = (width.max(1), height.max(1));
                    ctx.needs_redraw = true;
                }
                WindowEvent::Close { .. } => {
                    // Cerrar la ventana; sin ventanas, terminar la app.
                    if self.windows.len() == 1 {
                        self.windows.clear();
                        return Ok(());
                    }
                    self.close_window(index)?;
                }
                WindowEvent::Frame { .. } => {
                    // El compositor presentó: re-armar solo si toca.
                    let redraw = self.windows[index].needs_redraw;
                    self.windows[index].frame_pending = false;
                    if redraw {
                        self.draw_frame(index)?;
                    }
                }
                WindowEvent::PointerMove { x, y, .. } | WindowEvent::PointerEnter { x, y, .. } => {
                    let ctx = &mut self.windows[index];
                    // Layout en píxeles físicos: escalar coordenadas
                    // lógicas del puntero antes del hit-test.
                    let s = ctx.window.scale();
                    EventDispatcher::dispatch(
                        &mut ctx.ui,
                        UiInput::PointerMove {
                            x: x as f32 * s,
                            y: y as f32 * s,
                        },
                    );
                }
                WindowEvent::PointerButton { pressed, .. } => {
                    let ctx = &mut self.windows[index];
                    // last_pointer ya vive en coordenadas físicas (Move).
                    let (x, y) = ctx.ui.last_pointer().unwrap_or((0.0, 0.0));
                    let input = if pressed {
                        UiInput::PointerDown { x, y }
                    } else {
                        UiInput::PointerUp { x, y }
                    };
                    EventDispatcher::dispatch(&mut ctx.ui, input);
                }
                WindowEvent::Key { key, .. } => {
                    // Portapapeles Ctrl+C/V/X sobre el TextInput con foco.
                    self.handle_clipboard_keys(index, &key)?;
                    // El resto de teclas (texto, especiales, atajos)
                    // las resuelve el despachador: hook global → foco.
                    let ctx = &mut self.windows[index];
                    EventDispatcher::dispatch(
                        &mut ctx.ui,
                        UiInput::Key {
                            keysym: key.keysym,
                            text: key.utf8.clone(),
                            modifiers: key.modifiers,
                            pressed: key.pressed,
                        },
                    );
                }
                WindowEvent::Scroll { dx, dy, .. } => {
                    let ctx = &mut self.windows[index];
                    // El evento de window no trae posición: usar la
                    // última conocida del puntero (físicas).
                    let (x, y) = ctx.ui.last_pointer().unwrap_or((0.0, 0.0));
                    EventDispatcher::dispatch(
                        &mut ctx.ui,
                        UiInput::Scroll {
                            x,
                            y,
                            dx: dx as f32,
                            dy: dy as f32,
                        },
                    );
                }
                _ => {}
            }
            if self.windows.is_empty() || self.quit_requested() {
                return Ok(());
            }
        }
        // Render dirigido por suciedad (v0.2): solo repintar si algún
        // widget cambió (hover/foco/texto/estado), no por cada evento.
        for ctx in &mut self.windows {
            if ctx.ui.is_dirty() {
                ctx.needs_redraw = true;
            }
        }
        Ok(())
    }

    /// `true` si alguna UI pidió terminar.
    #[must_use]
    fn quit_requested(&self) -> bool {
        self.windows.iter().any(|ctx| ctx.ui.quit_requested())
    }

    /// Ctrl+C/V/X sobre el `TextInput` con foco (portapapeles del sistema).
    ///
    /// Sin selección de texto en v0.2: copia/corta el contenido completo.
    fn handle_clipboard_keys(&mut self, index: WindowIndex, key: &KeyEvent) -> Result<()> {
        let ui = &self.windows[index].ui;
        if !key.pressed
            || !key.modifiers.ctrl
            || key.modifiers.alt
            || key.modifiers.logo
            || ui.focus.focused(&ui.registry).is_none()
        {
            return Ok(());
        }
        let Some(focused) = ui.focus.focused(&ui.registry) else {
            return Ok(());
        };
        let is_input = ui
            .registry
            .node(focused)
            .map(|node| node.kind == WidgetKind::TextInput)
            .unwrap_or(false);
        if !is_input {
            return Ok(());
        }
        match key.keysym {
            0x63 => {
                // Ctrl+C: copiar todo el contenido.
                if let Some(text) = ui.text_input_text(focused) {
                    let text = text.to_string();
                    self.wayland
                        .set_clipboard_text(&text)
                        .map_err(|e| UiError::Subsystem(e.to_string()))?;
                }
            }
            0x78 => {
                // Ctrl+X: copiar y vaciar.
                if let Some(text) = ui.text_input_text(focused) {
                    let text = text.to_string();
                    self.wayland
                        .set_clipboard_text(&text)
                        .map_err(|e| UiError::Subsystem(e.to_string()))?;
                    self.windows[index].ui.text_input_set_text(focused, "");
                }
            }
            0x76 => {
                // Ctrl+V: pegar en el cursor (lectura con tope de espera).
                if let Some(text) = self
                    .wayland
                    .clipboard_text_blocking(std::time::Duration::from_millis(50))
                {
                    self.windows[index]
                        .ui
                        .text_input_insert_text(focused, &text);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Bucle principal: epoll de foundation + eventos de window + render.
    ///
    /// Termina cuando un callback llama a [`Ui::quit`] en cualquier
    /// ventana o se cierra la última ventana.
    pub fn run(&mut self) -> Result<()> {
        while !self.quit_requested() && !self.windows.is_empty() {
            self.pump_events()?;
            if self.quit_requested() || self.windows.is_empty() {
                break;
            }
            // Presentar cuando hay cambios sin frame callback pendiente
            // (configure/entrada): el commit re-arma el ciclo del compositor.
            for index in 0..self.windows.len() {
                let should_draw = {
                    let ctx = &self.windows[index];
                    ctx.needs_redraw && ctx.window.configured() && !ctx.frame_pending
                };
                if should_draw {
                    self.draw_frame(index)?;
                }
            }
        }
        Ok(())
    }

    fn draw_frame(&mut self, index: WindowIndex) -> Result<()> {
        let Application { gl, windows, .. } = self;
        let ctx = &mut windows[index];
        let root = ctx.ui.registry.root().ok_or_else(|| {
            UiError::InvalidTree("sin widget raíz: fija registry.set_root".into())
        })?;
        let (bw, bh) = ctx.window.buffer_size();
        // Layout con el tamaño lógico escalado (HiDPI fraccional o entero).
        let scale = ctx.window.scale().max(1.0);
        let (lw, lh) = (ctx.logical.0 as f32 * scale, ctx.logical.1 as f32 * scale);
        layout_tree(
            &mut ctx.ui.registry,
            &mut ctx.ui.text,
            &ctx.ui.style,
            root,
            lw,
            lh,
        )
        .map_err(|e| UiError::Subsystem(e.to_string()))?;
        ctx.queue.clear();
        ctx.queue
            .clear_color(Some(rivet_canvas::Color::opaque(0.06, 0.06, 0.07)));
        render_tree(
            &mut ctx.ui.registry,
            &mut ctx.ui.text,
            &ctx.ui.style,
            gl,
            &mut ctx.queue,
            root,
        )
        .map_err(|e| UiError::Subsystem(e.to_string()))?;
        // Render target por ventana (recreado al cambiar de tamaño).
        let (w, h) = (bw.max(1), bh.max(1));
        if ctx.target_size != (w, h) || ctx.target.is_none() {
            if let Some((fbo, texture)) = ctx.target.take() {
                gl.destroy_render_target(fbo, texture);
            }
            ctx.target = Some(
                gl.create_render_target(w, h)
                    .map_err(|e| UiError::Subsystem(e.to_string()))?,
            );
            // CRÍTICO: viewport GL + proyección ortográfica al tamaño del
            // target. Sin esto el contexto queda a 64×64 (pbuffer inicial)
            // y la escena completa se comprime en esa esquina (v0.3-fix).
            gl.resize(w, h);
            ctx.target_size = (w, h);
        }
        // GL al FBO y copia (volteada) al buffer wl_shm.
        let fbo = ctx.target.expect("target creado").0;
        gl.set_target(Some(fbo));
        let _stats = gl.render(&ctx.queue);
        // Búfer reutilizable: read directo + volteo in place (v0.2; antes
        // se alocaba el frame completo dos veces por presentación).
        ctx.pixel_buf.resize(w as usize * h as usize * 4, 0);
        gl.read_pixels_into(0, 0, w, h, &mut ctx.pixel_buf);
        flip_rows_in_place(&mut ctx.pixel_buf, w as usize, &mut ctx.flip_scratch);
        ctx.window
            .present_rgba(&ctx.pixel_buf)
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        ctx.needs_redraw = false;
        ctx.ui.clear_dirty(); // frame presentado: árbol limpio
        ctx.frame_pending = true; // el frame callback lo re-arma
        Ok(())
    }
}

/// Contexto GL headless vía EGL (pbuffer); devuelve el backend a mantener
/// vivo mientras dure el `GraphicsContext`.
fn create_headless_gl() -> std::result::Result<(GraphicsContext, GlBackend), String> {
    use khronos_egl::EGL1_4 as EGLApi;
    type Egl = khronos_egl::DynamicInstance<EGLApi>;
    let egl: Egl = unsafe { khronos_egl::DynamicInstance::<khronos_egl::EGL1_4>::load_required() }
        .map_err(|e| e.to_string())?;
    let display =
        unsafe { egl.get_display(khronos_egl::DEFAULT_DISPLAY) }.ok_or("eglGetDisplay falló")?;
    egl.initialize(display).map_err(|e| format!("{e:?}"))?;
    let config_attribs: &[khronos_egl::Int] = &[
        khronos_egl::RED_SIZE,
        8,
        khronos_egl::GREEN_SIZE,
        8,
        khronos_egl::BLUE_SIZE,
        8,
        khronos_egl::ALPHA_SIZE,
        8,
        khronos_egl::SURFACE_TYPE,
        khronos_egl::PBUFFER_BIT,
        khronos_egl::NONE,
    ];
    let mut configs = Vec::with_capacity(8);
    egl.choose_config(display, config_attribs, &mut configs)
        .map_err(|e| format!("eglChooseConfig: {e:?}"))?;
    let config = *configs.first().ok_or("sin configs EGL")?;
    egl.bind_api(khronos_egl::OPENGL_API)
        .map_err(|e| format!("{e:?}"))?;
    // ADR-01: contexto 3.3 Core vía EGL_CONTEXT_MAJOR/MINOR_VERSION.
    let context_attribs: &[khronos_egl::Int] = &[
        khronos_egl::CONTEXT_MAJOR_VERSION,
        3,
        khronos_egl::CONTEXT_MINOR_VERSION,
        3,
        khronos_egl::NONE,
    ];
    let context = egl
        .create_context(display, config, None, context_attribs)
        .map_err(|e| format!("eglCreateContext: {e:?}"))?;
    let surface_attribs: &[khronos_egl::Int] = &[
        khronos_egl::WIDTH,
        64,
        khronos_egl::HEIGHT,
        64,
        khronos_egl::NONE,
    ];
    let surface = egl
        .create_pbuffer_surface(display, config, surface_attribs)
        .map_err(|e| format!("eglCreatePbufferSurface: {e:?}"))?;
    egl.make_current(display, Some(surface), Some(surface), Some(context))
        .map_err(|e| format!("eglMakeCurrent: {e:?}"))?;

    // El loader solo se usa durante la creación: puede tomar prestado `egl`.
    let gl = GraphicsContext::from_loader(
        |name| {
            egl.get_proc_address(name)
                .map_or(std::ptr::null(), |f| f as *const core::ffi::c_void)
        },
        64,
        64,
    )
    .map_err(|e| e.to_string())?;
    Ok((
        gl,
        GlBackend {
            egl,
            display,
            context,
            surface,
        },
    ))
}

/// Voltea las filas del búfer **in place** (GL entrega bottom-up;
/// wl_shm espera top-down), con una fila temporal reutilizable.
fn flip_rows_in_place(rgba: &mut [u8], width: usize, scratch: &mut Vec<u8>) {
    let height = rgba.len() / (width * 4);
    let row = width * 4;
    scratch.resize(row, 0);
    for top in 0..height / 2 {
        let bottom = height - 1 - top;
        let a = top * row;
        let b = bottom * row;
        scratch.copy_from_slice(&rgba[a..a + row]);
        rgba.copy_within(b..b + row, a);
        rgba[b..b + row].copy_from_slice(scratch);
    }
}

#[cfg(test)]
mod tests {
    use super::flip_rows_in_place;

    #[test]
    fn flip_in_place_voltea_filas() {
        // 3 filas × 1 px RGBA: filas A (rojo), B (verde), C (azul).
        let mut buf = Vec::new();
        for row in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]] {
            buf.extend_from_slice(&row);
        }
        let mut scratch = Vec::new();
        flip_rows_in_place(&mut buf, 1, &mut scratch);
        assert_eq!(
            &buf,
            &[
                0, 0, 255, 255, // C arriba
                0, 255, 0, 255, // B
                255, 0, 0, 255, // A abajo
            ],
            "top-down tras el volteo"
        );
        // Idempotente tras doble volteo (par de filas pares).
        flip_rows_in_place(&mut buf, 1, &mut scratch);
        assert_eq!(&buf[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn flip_in_place_filas_pares() {
        let mut buf: Vec<u8> = (0u8..8 * 2 * 4).collect();
        let original_last_row: Vec<u8> = buf[8 * 4..].to_vec();
        let original_first_row: Vec<u8> = buf[..8 * 4].to_vec();
        let mut scratch = Vec::new();
        flip_rows_in_place(&mut buf, 8, &mut scratch);
        assert_eq!(&buf[..8 * 4], &original_last_row, "última fila pasa arriba");
        assert_eq!(&buf[8 * 4..], &original_first_row);
    }
}
