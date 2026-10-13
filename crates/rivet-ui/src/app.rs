//! `Application`: ciclo init/run/quit que conecta Wayland (window), GL
//! offscreen (canvas) y el árbol de UI (doc §10 de rivet-ui, §2 de rivet).
//!
//! Flujo v1 (`wl_shm`, ADR-02): eventos de window → UI → `RenderQueue` →
//! GL al FBO → `read_pixels` → `present_rgba` (copia al buffer
//! compartido). El frame callback del compositor pauta el redibujado.

use rivet_canvas::{GraphicsContext, RenderQueue};
use rivet_window::{Wayland, Window, WindowEvent, WindowOptions};

use crate::event::{EventDispatcher, UiInput};
use crate::layout::layout_tree;
use crate::render::render_tree;
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

/// Aplicación con una ventana y una UI.
///
/// ```ignore
/// let mut app = Application::new("Demo", 800, 600)?;
/// // construir widgets con app.ui_mut() y fijar registry.set_root(...)
/// app.run()?;
/// ```
pub struct Application {
    event_loop: rivet_foundation::event_loop::EventLoop,
    wayland: Wayland,
    window: Window,
    gl: GraphicsContext,
    _backend: Option<GlBackend>,
    target: Option<(rivet_canvas::glow::Framebuffer, rivet_canvas::TextureId)>,
    target_size: (u32, u32),
    ui: Ui,
    queue: RenderQueue,
    /// Redibujar en cuanto el compositor lo permita.
    needs_redraw: bool,
    /// Presentamos y esperamos el frame callback (re-arma el ciclo).
    frame_pending: bool,
    /// Tamaño lógico del último configure.
    logical: (u32, u32),
}

impl Application {
    /// Crea la app: conecta Wayland, abre la ventana y prepara GL.
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Result<Self> {
        let mut wayland = Wayland::connect().map_err(|e| UiError::Subsystem(e.to_string()))?;
        let mut event_loop = rivet_foundation::event_loop::EventLoop::new(None, 64)
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        let window = wayland
            .create_window(WindowOptions::new(width.max(1), height.max(1)).title(title))
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        // El commit inicial debe llegar al compositor antes de esperar en
        // epoll (si no, el configure nunca llega y el bucle bloquea).
        wayland
            .flush()
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        let (gl, backend) =
            create_headless_gl().map_err(|e| UiError::Subsystem(format!("EGL: {e}")))?;
        wayland
            .register_in(&mut event_loop)
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        Ok(Self {
            event_loop,
            wayland,
            window,
            gl,
            _backend: Some(backend),
            target: None,
            target_size: (0, 0),
            ui: Ui::new_dark(),
            queue: RenderQueue::new(),
            needs_redraw: true,
            frame_pending: false,
            logical: (width.max(1), height.max(1)),
        })
    }

    /// UI de la app (para construir widgets y conectar callbacks).
    #[must_use]
    pub fn ui(&self) -> &Ui {
        &self.ui
    }

    /// UI mutable.
    pub fn ui_mut(&mut self) -> &mut Ui {
        &mut self.ui
    }

    /// Compila un tema distinto para la UI.
    pub fn set_theme(&mut self, dsl: &str, theme: &str) {
        if let Ok(table) = rivet_style::compile(dsl, theme) {
            self.ui.style = table;
            self.needs_redraw = true;
        }
    }

    /// Bombea una vez: espera eventos (bloqueante) y los enruta a la UI.
    pub fn pump_events(&mut self) -> Result<()> {
        {
            self.event_loop
                .run_once(None)
                .map_err(|e| UiError::Subsystem(e.to_string()))?;
        }
        self.route_pending_events()
    }

    /// Presenta un frame ya renderizado (público para tests/ejemplos).
    pub fn present_frame(&mut self) -> Result<()> {
        self.draw_frame()
    }

    /// `true` cuando el compositor configuró la ventana.
    #[must_use]
    pub fn configured(&self) -> bool {
        self.window.configured()
    }

    fn route_pending_events(&mut self) -> Result<()> {
        {
            let mut close_requested = false;
            for event in self.wayland.take_events() {
                match event {
                    WindowEvent::Configure { width, height, .. } => {
                        self.logical = (width.max(1), height.max(1));
                        self.needs_redraw = true;
                    }
                    WindowEvent::Close { .. } => close_requested = true,
                    WindowEvent::Frame { .. } => {
                        // El compositor presentó: re-armar solo si toca.
                        self.frame_pending = false;
                        if self.needs_redraw {
                            self.draw_frame()?;
                        }
                    }
                    WindowEvent::PointerMove { x, y, .. }
                    | WindowEvent::PointerEnter { x, y, .. } => {
                        EventDispatcher::dispatch(
                            &mut self.ui,
                            UiInput::PointerMove {
                                x: x as f32,
                                y: y as f32,
                            },
                        );
                        self.needs_redraw = true;
                    }
                    WindowEvent::PointerButton { pressed, .. } => {
                        let (x, y) = self.ui.last_pointer().unwrap_or((0.0, 0.0));
                        let input = if pressed {
                            UiInput::PointerDown { x, y }
                        } else {
                            UiInput::PointerUp { x, y }
                        };
                        EventDispatcher::dispatch(&mut self.ui, input);
                        self.needs_redraw = true;
                    }
                    WindowEvent::Key { key, .. } => {
                        if key.pressed {
                            if key.keysym == 0xFF09 {
                                // Tab: avanzar foco.
                                EventDispatcher::dispatch(&mut self.ui, UiInput::Tab);
                            } else if !key.utf8.is_empty() {
                                EventDispatcher::dispatch(
                                    &mut self.ui,
                                    UiInput::KeyText {
                                        text: key.utf8.clone(),
                                        keysym: key.keysym,
                                        pressed: true,
                                    },
                                );
                            }
                        }
                        self.needs_redraw = true;
                    }
                    _ => {}
                }
                if self.ui.quit_requested() {
                    break;
                }
            }
            if close_requested {
                self.ui.quit();
            }
        }
        Ok(())
    }

    /// Bucle principal: epoll de foundation + eventos de window + render.
    ///
    /// Termina cuando un callback llama a [`Ui::quit`] o cierran la ventana.
    pub fn run(&mut self) -> Result<()> {
        while !self.ui.quit_requested() {
            self.pump_events()?;
            if self.ui.quit_requested() {
                break;
            }
            // Presentar cuando hay cambios sin frame callback pendiente
            // (configure/entrada): el commit re-arma el ciclo del compositor.
            if self.needs_redraw && self.window.configured() && !self.frame_pending {
                self.draw_frame()?;
            }
        }
        Ok(())
    }

    fn draw_frame(&mut self) -> Result<()> {
        let root = self.ui.registry.root().ok_or_else(|| {
            UiError::InvalidTree("sin widget raíz: fija registry.set_root".into())
        })?;
        let (bw, bh) = self.window.buffer_size();
        self.ensure_target(bw, bh)?;
        // Layout con el tamaño lógico escalado (HiDPI entero).
        let scale = self.window.scale().max(1) as f32;
        let (lw, lh) = (self.logical.0 as f32 * scale, self.logical.1 as f32 * scale);
        layout_tree(
            &mut self.ui.registry,
            &mut self.ui.text,
            &self.ui.style,
            root,
            lw,
            lh,
        )
        .map_err(|e| UiError::Subsystem(e.to_string()))?;
        self.queue.clear();
        self.queue
            .clear_color(Some(rivet_canvas::Color::opaque(0.06, 0.06, 0.07)));
        render_tree(
            &mut self.ui.registry,
            &mut self.ui.text,
            &self.ui.style,
            &mut self.gl,
            &mut self.queue,
            root,
        )
        .map_err(|e| UiError::Subsystem(e.to_string()))?;
        // GL al FBO y copia (volteada) al buffer wl_shm.
        let fbo = self.target.expect("target creado").0;
        self.gl.set_target(Some(fbo));
        let _stats = self.gl.render(&self.queue);
        let pixels = self.gl.read_pixels(0, 0, bw.max(1), bh.max(1));
        let flipped = flip_rows(&pixels, bw.max(1) as usize, bh.max(1) as usize);
        self.window
            .present_rgba(&flipped)
            .map_err(|e| UiError::Subsystem(e.to_string()))?;
        self.needs_redraw = false;
        self.frame_pending = true; // el frame callback lo re-arma
        Ok(())
    }

    fn ensure_target(&mut self, w: u32, h: u32) -> Result<()> {
        let (w, h) = (w.max(1), h.max(1));
        if self.target_size == (w, h) && self.target.is_some() {
            return Ok(());
        }
        if let Some((fbo, texture)) = self.target.take() {
            self.gl.destroy_render_target(fbo, texture);
        }
        self.target = Some(
            self.gl
                .create_render_target(w, h)
                .map_err(|e| UiError::Subsystem(e.to_string()))?,
        );
        self.gl.resize(w, h);
        self.target_size = (w, h);
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

/// Da la vuelta a las filas (GL entrega bottom-up; wl_shm espera top-down).
fn flip_rows(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for row in (0..height).rev() {
        let start = row * width * 4;
        out.extend_from_slice(&rgba[start..start + width * 4]);
    }
    out
}
