//! API pública: `Wayland` (conexión) y `Window` (toplevel xdg-shell).

use std::cell::RefCell;
use std::os::fd::{AsFd as _, AsRawFd as _};
use std::rc::Rc;

use wayland_client::protocol::{wl_buffer, wl_shm::Format};
use wayland_client::{Connection as WlConnection, EventQueue, QueueHandle};

use rivet_foundation::event_loop::{interest, EventLoop};

use crate::error::{Result, WindowError};
use crate::event::{WindowEvent, WindowId};
use crate::shm::ShmPixels;
use crate::state::{
    BufferUd, FrameUd, SurfaceUd, WaylandState, WindowInner, ZombiePool, BUFFER_SLOTS,
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
/// Integradla con el bucle de eventos de `rivet-foundation` vía
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
        let xdg_surface = wm_base.get_xdg_surface(&surface, qh, ud.clone());
        let toplevel = xdg_surface.get_toplevel(qh, ud.clone());
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
            xdg_surface,
            toplevel,
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

    /// Cambia el título.
    pub fn set_title(&self, title: &str) {
        let inner = self.shared.borrow();
        if let Some(w) = inner.state.windows.get(&self.id) {
            w.toplevel.set_title(title.to_owned());
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
