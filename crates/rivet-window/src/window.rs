//! Public API: `Wayland` (connection), `Window` (toplevel/layer-shell) and
//! basic clipboard.

use std::cell::RefCell;
use std::os::fd::{AsFd as _, AsRawFd as _};
use std::rc::Rc;
use std::time::Duration;

use wayland_client::protocol::{wl_buffer, wl_shm::Format};
use wayland_client::{Connection as WlConnection, EventQueue, Proxy as _, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::Layer as WlrLayer, zwlr_layer_surface_v1::Anchor as WlrAnchor,
    zwlr_layer_surface_v1::KeyboardInteractivity as WlrInteractivity,
};

use rivet_base::event_loop::{interest, EventLoop};

use crate::error::{Result, WindowError};
use crate::event::{WindowEvent, WindowId};
use crate::shm::ShmPixels;
use crate::state::{
    BufferUd, FrameUd, OfferState, Role, SurfaceUd, WaylandState, WindowInner, ZombiePool,
    BUFFER_SLOTS,
};

/// Interior mutability compartida: la usan `Wayland`, `Window` y el handler
/// registrado en el bucle de eventos (todo en un único hilo).
type Shared = Rc<RefCell<Inner>>;

pub(crate) struct Inner {
    pub(crate) conn: WlConnection,
    pub(crate) queue: EventQueue<WaylandState>,
    pub(crate) qh: QueueHandle<WaylandState>,
    pub(crate) state: WaylandState,
}

/// Capa de una superficie anclada (layer-shell).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Fondo del escritorio.
    Background,
    /// Debajo de las ventanas.
    Bottom,
    /// Encima de las ventanas.
    Top,
    /// Encima de todo (overlays, notificaciones).
    Overlay,
}

impl From<Layer> for WlrLayer {
    fn from(layer: Layer) -> Self {
        match layer {
            Layer::Background => WlrLayer::Background,
            Layer::Bottom => WlrLayer::Bottom,
            Layer::Top => WlrLayer::Top,
            Layer::Overlay => WlrLayer::Overlay,
        }
    }
}

/// Anclas de una superficie layer-shell (combinables con `|`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LayerAnchors {
    /// Anclar al borde superior.
    pub top: bool,
    /// Anclar al borde inferior.
    pub bottom: bool,
    /// Anclar al borde izquierdo.
    pub left: bool,
    /// Anclar al borde derecho.
    pub right: bool,
}

impl LayerAnchors {
    /// Anclar a todos los bordes (superficie a pantalla completa).
    #[must_use]
    pub fn all() -> Self {
        Self {
            top: true,
            bottom: true,
            left: true,
            right: true,
        }
    }

    fn to_wlr(self) -> WlrAnchor {
        let mut anchor = WlrAnchor::empty();
        if self.top {
            anchor |= WlrAnchor::Top;
        }
        if self.bottom {
            anchor |= WlrAnchor::Bottom;
        }
        if self.left {
            anchor |= WlrAnchor::Left;
        }
        if self.right {
            anchor |= WlrAnchor::Right;
        }
        anchor
    }
}

/// Interactividad de teclado de una superficie layer-shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardInteractivity {
    /// Sin foco de teclado.
    #[default]
    None,
    /// Foco exclusivo.
    Exclusive,
    /// Foco bajo demanda.
    OnDemand,
}

impl From<KeyboardInteractivity> for WlrInteractivity {
    fn from(value: KeyboardInteractivity) -> Self {
        match value {
            KeyboardInteractivity::None => WlrInteractivity::None,
            KeyboardInteractivity::Exclusive => WlrInteractivity::Exclusive,
            KeyboardInteractivity::OnDemand => WlrInteractivity::OnDemand,
        }
    }
}

/// Opciones de creación de superficie anclada (layer-shell).
///
/// Base para el panel y el lanzador de las apps semilla (doc §4).
#[derive(Debug, Clone)]
pub struct LayerSurfaceOptions {
    /// Tamaño lógico inicial (0 en un eje + anclas = auto-estirar).
    pub width: u32,
    /// Alto lógico inicial.
    pub height: u32,
    /// Capa.
    pub layer: Layer,
    /// Bordes a los que anclarse.
    pub anchors: LayerAnchors,
    /// Zona exclusiva (-1 la desactiva).
    pub exclusive_zone: i32,
    /// Foco de teclado.
    pub keyboard_interactivity: KeyboardInteractivity,
    /// Namespace para agrupar (identificación del cliente).
    pub namespace: String,
}

impl LayerSurfaceOptions {
    /// Opciones de una barra anclada abajo a pantalla completa.
    #[must_use]
    pub fn bar(height: u32, layer: Layer) -> Self {
        Self {
            width: 0,
            height,
            layer,
            anchors: LayerAnchors {
                top: false,
                bottom: true,
                left: true,
                right: true,
            },
            exclusive_zone: height as i32,
            keyboard_interactivity: KeyboardInteractivity::None,
            namespace: String::from("rivet"),
        }
    }
}

/// Opciones de creación de ventana.
#[derive(Debug, Clone)]
pub struct WindowOptions {
    /// Ancho lógico inicial.
    pub width: u32,
    /// Alto lógico inicial.
    pub height: u32,
    /// Título de la ventana.
    pub title: String,
    /// App id (para asociación de iconos/escritorio).
    pub app_id: String,
}

impl WindowOptions {
    /// Opciones por defecto 800×600.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            title: String::from("Rivet"),
            app_id: String::from("rivet"),
        }
    }

    /// Establece el título.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Establece el app id.
    #[must_use]
    pub fn app_id(mut self, app_id: impl Into<String>) -> Self {
        self.app_id = app_id.into();
        self
    }
}

/// Conexión al compositor Wayland con handshake de globals hecho.
///
/// Integradla con el bucle de eventos de `rivet-base` vía
/// [`Wayland::register_in`]; los eventos llegan por [`Wayland::take_events`].
pub struct Wayland {
    shared: Shared,
    registered: bool,
}

impl Wayland {
    /// Conecta con `$WAYLAND_DISPLAY` y completa el handshake del registry.
    pub fn connect() -> Result<Self> {
        let conn =
            WlConnection::connect_to_env().map_err(|e| WindowError::Connect(e.to_string()))?;
        let queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let state = WaylandState::new(qh.clone());
        let this = Self {
            shared: Rc::new(RefCell::new(Inner {
                conn,
                queue,
                qh,
                state,
            })),
            registered: false,
        };
        {
            let mut inner = this.shared.borrow_mut();
            let Inner { queue, state, .. } = &mut *inner;
            queue
                .roundtrip(state)
                .map_err(|e| WindowError::Connect(e.to_string()))?;
            queue
                .roundtrip(state)
                .map_err(|e| WindowError::Connect(e.to_string()))?;
        }
        Ok(this)
    }

    /// FD del socket Wayland (para integraciones manuales).
    pub fn socket_fd(&self) -> std::os::unix::io::RawFd {
        self.shared.borrow().conn.as_fd().as_raw_fd()
    }

    /// Registra el socket en el bucle `epoll`: en readable, lee y despacha.
    pub fn register_in(&mut self, event_loop: &mut EventLoop) -> Result<()> {
        if self.registered {
            return Ok(());
        }
        let shared = Rc::clone(&self.shared);
        let conn = self.shared.borrow().conn.clone();
        let fd = self.socket_fd();
        event_loop
            .register_fd(
                fd,
                interest::READABLE,
                Box::new(move |_fd, _flags, _dispatcher| {
                    // Lee los eventos disponibles y despáchalos.
                    if let Some(guard) = conn.prepare_read() {
                        let _ = guard.read();
                    }
                    let mut inner = shared.borrow_mut();
                    let Inner {
                        queue, state, conn, ..
                    } = &mut *inner;
                    let _ = queue.dispatch_pending(state);
                    let _ = conn.flush();
                    state.prune_zombies();
                }),
            )
            .map_err(|e| WindowError::Connect(e.to_string()))?;
        self.registered = true;
        Ok(())
    }

    /// Despacha eventos pendientes sin bucle (útil en tests).
    pub fn dispatch_pending(&mut self) -> Result<()> {
        let mut inner = self.shared.borrow_mut();
        let Inner {
            conn, queue, state, ..
        } = &mut *inner;
        if let Some(guard) = conn.prepare_read() {
            let _ = guard.read();
        }
        queue
            .dispatch_pending(state)
            .map_err(|e| WindowError::Connect(e.to_string()))?;
        conn.flush()
            .map_err(|e| WindowError::Connect(e.to_string()))?;
        state.prune_zombies();
        Ok(())
    }

    /// Roundtrip bloqueante: envía peticiones y espera la respuesta del
    /// compositor (incluye los eventos generados por ellas).
    pub fn roundtrip(&mut self) -> Result<()> {
        let mut inner = self.shared.borrow_mut();
        let Inner { queue, state, .. } = &mut *inner;
        queue
            .roundtrip(state)
            .map_err(|e| WindowError::Connect(e.to_string()))?;
        state.prune_zombies();
        Ok(())
    }

    /// Envía las peticiones acumuladas al compositor.
    pub fn flush(&self) -> Result<()> {
        self.shared
            .borrow()
            .conn
            .flush()
            .map_err(|e| WindowError::Connect(e.to_string()))
    }

    /// Crea una ventana toplevel (xdg-shell).
    ///
    /// El primer [`WindowEvent::Configure`] del compositor indica que ya se
    /// puede presentar con [`Window::present`].
    pub fn create_window(&mut self, opts: WindowOptions) -> Result<Window> {
        let mut inner = self.shared.borrow_mut();
        let Inner {
            conn, qh, state, ..
        } = &mut *inner;
        let compositor = state
            .compositor
            .clone()
            .ok_or_else(|| WindowError::MissingGlobal("wl_compositor".into()))?;
        let shm = state
            .shm
            .clone()
            .ok_or_else(|| WindowError::MissingGlobal("wl_shm".into()))?;
        let wm_base = state
            .wm_base
            .clone()
            .ok_or_else(|| WindowError::MissingGlobal("xdg_wm_base".into()))?;

        let window_id = state.next_window;
        state.next_window = window_id
            .checked_add(1)
            .expect("desbordamiento de IDs de ventana");
        let ud = SurfaceUd { window: window_id };

        // Superficie + rol toplevel.
        let surface = compositor.create_surface(qh, ud.clone());
        let xdg = wm_base.get_xdg_surface(&surface, qh, ud.clone());
        let toplevel = xdg.get_toplevel(qh, ud.clone());
        toplevel.set_title(opts.title.clone());
        toplevel.set_app_id(opts.app_id.clone());

        // Pool shm a tamaño físico (escala entera de las salidas).
        let scale = state.max_scale();
        let phys_w = opts.width.max(1);
        let phys_h = opts.height.max(1);
        let pixels = ShmPixels::new(phys_w, phys_h, BUFFER_SLOTS);
        let pool = shm.create_pool(
            pixels.borrowed_fd(),
            pixels.mapping_len_total() as i32,
            qh,
            (),
        );
        let _ = conn; // el flush lo hace el caller via Wayland::flush/register

        let window_inner = WindowInner {
            surface,
            role: Role::Toplevel { xdg, toplevel },
            configured: false,
            logical_width: opts.width.max(1),
            logical_height: opts.height.max(1),
            scale,
            pixels,
            pool,
            generation: 0,
            slots: vec![false; BUFFER_SLOTS],
            buffers: Vec::new(),
        };
        state.windows.insert(window_id, window_inner);

        // Primer commit sin buffer: el compositor responde con configure.
        state.windows[&window_id].surface.commit();
        Ok(Window {
            id: window_id,
            shared: Rc::clone(&self.shared),
        })
    }

    /// `true` si el compositor ofrece `zwp_layer_shell_v1`.
    #[must_use]
    pub fn has_layer_shell(&self) -> bool {
        self.shared.borrow().state.layer_shell.is_some()
    }

    /// Crea una superficie anclada (layer-shell).
    ///
    /// Requiere `has_layer_shell()`; si no, devuelve [`WindowError::MissingGlobal`].
    /// El primer [`WindowEvent::Configure`] indica tamaño definitivo y que ya
    /// se puede presentar con [`Window::present_rgba`].
    pub fn create_layer_surface(&mut self, opts: LayerSurfaceOptions) -> Result<Window> {
        let mut inner = self.shared.borrow_mut();
        let Inner { qh, state, .. } = &mut *inner;
        let compositor = state
            .compositor
            .clone()
            .ok_or_else(|| WindowError::MissingGlobal("wl_compositor".into()))?;
        let shm = state
            .shm
            .clone()
            .ok_or_else(|| WindowError::MissingGlobal("wl_shm".into()))?;
        let layer_shell = state
            .layer_shell
            .clone()
            .ok_or_else(|| WindowError::MissingGlobal("zwp_layer_shell_v1".into()))?;

        let window_id = state.next_window;
        state.next_window = window_id
            .checked_add(1)
            .expect("desbordamiento de IDs de ventana");
        let ud = SurfaceUd { window: window_id };

        let surface = compositor.create_surface(qh, ud.clone());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None, // salida: la que elija el compositor
            opts.layer.into(),
            opts.namespace.clone(),
            qh,
            ud.clone(),
        );
        layer_surface.set_size(opts.width.max(1), opts.height);
        layer_surface.set_anchor(opts.anchors.to_wlr());
        layer_surface.set_exclusive_zone(opts.exclusive_zone);
        layer_surface.set_keyboard_interactivity(opts.keyboard_interactivity.into());

        let scale = state.max_scale();
        let phys_w = opts.width.max(1);
        let phys_h = opts.height.max(1);
        let pixels = ShmPixels::new(phys_w, phys_h, BUFFER_SLOTS);
        let pool = shm.create_pool(
            pixels.borrowed_fd(),
            pixels.mapping_len_total() as i32,
            qh,
            (),
        );

        let window_inner = WindowInner {
            surface,
            role: Role::LayerShell {
                layer: layer_surface,
            },
            configured: false,
            logical_width: opts.width.max(1),
            logical_height: opts.height.max(1),
            scale,
            pixels,
            pool,
            generation: 0,
            slots: vec![false; BUFFER_SLOTS],
            buffers: Vec::new(),
        };
        state.windows.insert(window_id, window_inner);
        state.windows[&window_id].surface.commit();
        Ok(Window {
            id: window_id,
            shared: Rc::clone(&self.shared),
        })
    }

    /// `true` si el compositor ofrece `zwp_pointer_gestures_v1`.
    #[must_use]
    pub fn has_gestures(&self) -> bool {
        self.shared.borrow().state.gestures.is_some()
    }

    /// `true` cuando ya llegó algún serial de entrada (requisito de
    /// `set_selection`/`start_drag` en compositores estrictos como KWin).
    #[must_use]
    pub fn has_input_serial(&self) -> bool {
        self.shared.borrow().state.last_serial != 0
    }

    /// Inicia un arrastre (DnD) con texto, tomando `window` como origen.
    ///
    /// Requiere un serial de entrada válido ([`Wayland::has_input_serial`]);
    /// el destinatario lee el texto con `text/plain;charset=utf-8`.
    pub fn start_text_drag(&mut self, window: &Window, text: &str) -> Result<()> {
        let mut inner = self.shared.borrow_mut();
        let Inner {
            conn, qh, state, ..
        } = &mut *inner;
        let manager = state
            .data_device_manager
            .clone()
            .ok_or_else(|| WindowError::MissingGlobal("wl_data_device_manager".into()))?;
        let device = state
            .data_device
            .clone()
            .ok_or_else(|| WindowError::MissingGlobal("wl_data_device".into()))?;
        let origin = state
            .windows
            .get(&window.id)
            .expect("ventana viva: el handle garantiza su presencia")
            .surface
            .clone();
        if state.last_serial == 0 {
            return Err(WindowError::Connect(
                "sin serial de entrada: requiere interacción previa del usuario".into(),
            ));
        }
        let source = manager.create_data_source(qh, ());
        source.offer(String::from("text/plain;charset=utf-8"));
        source.offer(String::from("text/plain"));
        device.start_drag(Some(&source), &origin, None, state.last_serial);
        state.drag_text.push((source, text.to_string()));
        conn.flush()
            .map_err(|e| WindowError::Connect(e.to_string()))?;
        Ok(())
    }

    /// Fija el texto del portapapeles (selección).
    ///
    /// Sirve `text/plain;charset=utf-8` y `text/plain` a quien pida pegar.
    pub fn set_clipboard_text(&mut self, text: &str) -> Result<()> {
        let mut inner = self.shared.borrow_mut();
        let Inner {
            conn, qh, state, ..
        } = &mut *inner;
        let manager = state
            .data_device_manager
            .clone()
            .ok_or_else(|| WindowError::MissingGlobal("wl_data_device_manager".into()))?;
        let device = state
            .data_device
            .clone()
            .ok_or_else(|| WindowError::MissingGlobal("wl_data_device".into()))?;
        let serial = state.last_serial;

        let source = manager.create_data_source(qh, ());
        source.offer(String::from("text/plain;charset=utf-8"));
        source.offer(String::from("text/plain"));
        device.set_selection(Some(&source), serial);
        if let Some(old) = state.own_source.replace(source) {
            if old.is_alive() {
                old.destroy();
            }
        }
        state.clipboard_text = Some(text.to_string());
        conn.flush()
            .map_err(|e| WindowError::Connect(e.to_string()))?;
        Ok(())
    }

    /// Lee el texto del portapapeles (selección ajena), con tope de espera.
    ///
    /// `None` si no hay selección, no ofrece texto o expira el timeout.
    /// v1 síncrono (poll con timeout); la variante asíncrona llegará con la
    /// integración completa del bucle.
    pub fn clipboard_text_blocking(&mut self, timeout: Duration) -> Option<String> {
        let mut inner = self.shared.borrow_mut();
        let offer = inner.state.selection_offer.clone()?;
        read_offer_text(&mut inner, &offer, timeout)
    }

    /// Lee el texto del arrastre soltado (tras un `WindowEvent::DndDrop`).
    pub fn dnd_text_blocking(&mut self, timeout: Duration) -> Option<String> {
        let mut inner = self.shared.borrow_mut();
        let offer = inner.state.dnd_offer.clone()?;
        let text = read_offer_text(&mut inner, &offer, timeout);
        // La oferta se agota tras el drop: liberarla.
        if let Some(o) = inner.state.dnd_offer.take() {
            o.offer.destroy();
        }
        text
    }

    /// Vacía y devuelve los eventos acumulados.
    pub fn take_events(&mut self) -> Vec<WindowEvent> {
        std::mem::take(&mut self.shared.borrow_mut().state.events)
    }

    /// Escala entera máxima de las salidas detectadas.
    #[must_use]
    pub fn max_scale(&self) -> u32 {
        self.shared.borrow().state.max_scale()
    }
}

/// Ventana toplevel con doble buffer `wl_shm`.
///
/// Flujo por frame: esperar [`WindowEvent::Frame`] → dibujar →
/// [`Window::present`] (adjunta, daña y commitea; solicita el siguiente
/// frame callback).
pub struct Window {
    id: WindowId,
    shared: Shared,
}

impl Window {
    /// ID interno de la ventana.
    #[must_use]
    pub fn id(&self) -> WindowId {
        self.id
    }

    /// Tamaño lógico actual (según el último configure).
    #[must_use]
    pub fn logical_size(&self) -> (u32, u32) {
        let inner = self.shared.borrow();
        let w = inner.state.windows.get(&self.id);
        w.map_or((1, 1), |w| (w.logical_width, w.logical_height))
    }

    /// Tamaño físico del buffer (lógico × escala).
    #[must_use]
    pub fn buffer_size(&self) -> (u32, u32) {
        let inner = self.shared.borrow();
        let w = inner.state.windows.get(&self.id);
        w.map_or((1, 1), |w| {
            (
                w.logical_width.max(1) * w.scale,
                w.logical_height.max(1) * w.scale,
            )
        })
    }

    /// Escala del buffer de esta ventana.
    #[must_use]
    pub fn scale(&self) -> u32 {
        let inner = self.shared.borrow();
        inner.state.windows.get(&self.id).map_or(1, |w| w.scale)
    }

    /// `true` tras el primer configure del compositor.
    #[must_use]
    pub fn configured(&self) -> bool {
        let inner = self.shared.borrow();
        inner
            .state
            .windows
            .get(&self.id)
            .is_some_and(|w| w.configured)
    }

    /// Cambia el título (solo ventanas con rol toplevel).
    pub fn set_title(&self, title: &str) {
        let inner = self.shared.borrow();
        if let Some(w) = inner.state.windows.get(&self.id) {
            if let Role::Toplevel { toplevel, .. } = &w.role {
                toplevel.set_title(title.to_owned());
            }
        }
    }

    /// Presenta un frame completo.
    ///
    /// * `rgba`: píxeles **físicos** (`buffer_size`) en RGBA; se convierten a
    ///   BGRA (byte order de `wl_shm` ARGB8888) al copiar.
    ///
    /// Errores esperables: [`WindowError::NotConfigured`] antes del primer
    /// configure y [`WindowError::WouldBlock`] si el compositor aún no
    /// liberó ningún buffer (reintentar en el próximo `Frame`).
    pub fn present_rgba(&mut self, rgba: &[u8]) -> Result<()> {
        let mut inner = self.shared.borrow_mut();
        let Inner {
            conn, qh, state, ..
        } = &mut *inner;
        let window = state
            .windows
            .get(&self.id)
            .ok_or(WindowError::NotConfigured)?;
        if !window.configured {
            return Err(WindowError::NotConfigured);
        }

        // Tamaño físico deseado; recrear pool si cambió.
        let (phys_w, phys_h) = (
            window.logical_width.max(1) * window.scale,
            window.logical_height.max(1) * window.scale,
        );
        if window.pixels.width() != phys_w || window.pixels.height() != phys_h {
            state.retire_pool(self.id);
        }

        let slot = {
            let window = state.windows.get_mut(&self.id).expect("ventana presente");
            window
                .slots
                .iter()
                .position(|busy| !busy)
                .ok_or(WindowError::WouldBlock)?
        };

        // Copia con swizzle RGBA→BGRA al frame del slot.
        {
            let window = state.windows.get_mut(&self.id).expect("ventana presente");
            let frame_len = window.pixels.frame_len();
            if rgba.len() != frame_len {
                return Err(WindowError::InvalidBuffer(format!(
                    "esperaba {frame_len} bytes ({}×{}×4), recibí {}",
                    window.pixels.width(),
                    window.pixels.height(),
                    rgba.len()
                )));
            }
            let slot_pixels = window.pixels.slot_slice_mut(slot);
            for (dst, src) in slot_pixels.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
                dst[0] = src[2];
                dst[1] = src[1];
                dst[2] = src[0];
                dst[3] = src[3];
            }
        }

        // Crea (o reutiliza) el wl_buffer del slot y adjunta.
        let (surface, buffer) = {
            let window = state.windows.get_mut(&self.id).expect("ventana presente");
            let buffer = window.take_slot_buffer(
                qh,
                BufferUd {
                    window: self.id,
                    generation: window.generation,
                    slot,
                },
            );
            (window.surface.clone(), buffer)
        };
        surface.attach(Some(&buffer), 0, 0);
        let (w, h) = (phys_w as i32, phys_h as i32);
        surface.damage_buffer(0, 0, w, h);
        // El callback de frame debe registrarse ANTES del commit para que
        // dispare con este frame (paceo del redibujado, doc §7).
        surface.frame(qh, FrameUd { window: self.id });
        surface.commit();

        state
            .windows
            .get_mut(&self.id)
            .expect("ventana presente")
            .slots[slot] = true;
        conn.flush()
            .map_err(|e| WindowError::System(std::io::Error::other(e.to_string())))?;
        Ok(())
    }
}

impl WaylandState {
    /// Retira el pool actual de una ventana a la lista de zombies y crea uno
    /// nuevo del tamaño físico vigente.
    pub(crate) fn retire_pool(&mut self, window_id: WindowId) {
        let Some(window) = self.windows.get(&window_id) else {
            return;
        };
        let (phys_w, phys_h) = (
            window.logical_width.max(1) * window.scale,
            window.logical_height.max(1) * window.scale,
        );
        let new_pixels = ShmPixels::new(phys_w, phys_h, BUFFER_SLOTS);
        let new_pool = self.shm.as_ref().expect("wl_shm presente").create_pool(
            new_pixels.borrowed_fd(),
            new_pixels.mapping_len_total() as i32,
            &self.qh,
            (),
        );
        let window = self.windows.get_mut(&window_id).expect("ventana presente");
        let old_pixels = std::mem::replace(&mut window.pixels, new_pixels);
        let old_pool = std::mem::replace(&mut window.pool, new_pool);
        let busy = window.slots.iter().filter(|b| **b).count();
        window.generation += 1;
        window.slots = vec![false; BUFFER_SLOTS];
        let old_buffers = std::mem::take(&mut window.buffers);
        self.zombies.push(ZombiePool {
            pixels: old_pixels,
            pool: old_pool,
            buffers: old_buffers,
            window: window_id,
            generation: window.generation - 1,
            pending: busy,
        });
    }

    /// Elimina zombies sin buffers pendientes de release, destruyendo sus
    /// objetos Wayland (wl_buffer + wl_shm_pool) de forma segura.
    pub(crate) fn prune_zombies(&mut self) {
        let mut i = 0;
        while i < self.zombies.len() {
            if self.zombies[i].pending == 0 {
                let zombie = self.zombies.remove(i);
                for buffer in zombie.buffers.into_iter().flatten() {
                    buffer.destroy();
                }
                zombie.pool.destroy();
                // `pixels` se libera por Drop (munmap + close fd).
            } else {
                i += 1;
            }
        }
    }
}

impl WindowInner {
    /// Devuelve (o crea) el `wl_buffer` del slot sobre el pool actual.
    pub(crate) fn take_slot_buffer(
        &mut self,
        qh: &QueueHandle<WaylandState>,
        ud: BufferUd,
    ) -> wl_buffer::WlBuffer {
        if let Some(Some(existing)) = self.buffers.get(ud.slot) {
            return existing.clone();
        }
        let slot_len = self.pixels.frame_len();
        let slot = ud.slot;
        let buffer = self.pool.create_buffer(
            (slot * slot_len) as i32,
            self.pixels.width() as i32,
            self.pixels.height() as i32,
            self.pixels.stride() as i32,
            Format::Argb8888,
            qh,
            ud,
        );
        if self.buffers.len() < slot + 1 {
            self.buffers.resize(slot + 1, None);
            self.buffers[slot] = Some(buffer.clone());
        }
        buffer
    }
}

/// Lee `text/plain` de una oferta vía pipe + poll con timeout.
///
/// Intercale `dispatch_pending` en la espera: la fuente de la selección puede
/// ser **este mismo cliente** (vía `wl_data_source.send`), así que los datos
/// solo llegan si despachamos mientras esperamos.
fn read_offer_text(inner: &mut Inner, offer: &OfferState, timeout: Duration) -> Option<String> {
    let mime = offer
        .mimes
        .iter()
        .find(|m| *m == "text/plain;charset=utf-8")
        .or_else(|| offer.mimes.iter().find(|m| m.as_str() == "text/plain"))?
        .clone();
    let mut fds = [0 as std::os::unix::io::RawFd; 2];
    // SAFETY: pipe2 con flags válidos; comprobamos el resultado.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
        return None;
    }
    // receive: la fuente escribirá en el fd que le pasamos → extremo de
    // ESCRITURA (fds[1]); nosotros leemos del extremo de lectura (fds[0]).
    offer
        .offer
        .receive(mime, unsafe { std::os::fd::BorrowedFd::borrow_raw(fds[1]) });
    if inner.conn.flush().is_err() {
        // SAFETY: fds abiertos por nosotros.
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
        return None;
    }

    let deadline = std::time::Instant::now() + timeout;
    let mut buf = Vec::new();
    let mut done = false;
    while !done && std::time::Instant::now() < deadline {
        let mut pfd = libc::pollfd {
            fd: fds[0],
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll sobre un fd propio, un elemento, espera breve.
        let ready = unsafe { libc::poll(&mut pfd, 1, 20) };
        if ready > 0 && (pfd.revents & libc::POLLIN) != 0 {
            let mut chunk = [0u8; 4096];
            loop {
                // SAFETY: lectura en buffer propio del tamaño indicado.
                let n = unsafe { libc::read(fds[0], chunk.as_mut_ptr().cast(), chunk.len()) };
                if n <= 0 {
                    // 0 = EOF (el escritor cerró): lectura completa.
                    done = n == 0;
                    break;
                }
                buf.extend_from_slice(&chunk[..n as usize]);
            }
        } else {
            // Sin datos aún: despachar (el send puede venir de nuestro source).
            let Inner {
                conn, queue, state, ..
            } = inner;
            if let Some(guard) = conn.prepare_read() {
                let _ = guard.read();
            }
            let _ = queue.dispatch_pending(state);
            let _ = conn.flush();
            state.prune_zombies();
        }
    }
    // SAFETY: fds abiertos por nosotros; cierre determinista.
    unsafe {
        libc::close(fds[0]);
        libc::close(fds[1]);
    }
    (!buf.is_empty()).then(|| String::from_utf8_lossy(&buf).into_owned())
}
