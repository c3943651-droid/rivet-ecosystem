//! Estado interno Wayland: globals, ventanas y todos los `Dispatch`.
//!
//! Los detalles del protocolo quedan encapsulados aquí; la API pública
//! (`crate::window`) expone `Wayland`/`Window`/`WindowEvent`.

use std::collections::HashMap;
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry,
    wl_seat, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{delegate_noop, Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use xkbcommon::xkb;

use crate::event::{KeyEvent, Modifiers, MouseButton, WindowEvent, WindowId};
use crate::shm::ShmPixels;

/// Slots de buffer por ventana (doble buffering).
pub(crate) const BUFFER_SLOTS: usize = 2;

// ---------------------------------------------------------------------------
// User data de los proxies.
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct SurfaceUd {
    pub(crate) window: WindowId,
}

#[derive(Clone)]
pub(crate) struct BufferUd {
    pub(crate) window: WindowId,
    /// Generación del pool: los `release` de pools obsoletos no liberan slots.
    pub(crate) generation: u32,
    pub(crate) slot: usize,
}

#[derive(Clone)]
pub(crate) struct FrameUd {
    pub(crate) window: WindowId,
}

#[derive(Clone)]
pub(crate) struct OutputUd {
    pub(crate) index: usize,
}

// ---------------------------------------------------------------------------
// Estado.
// ---------------------------------------------------------------------------

/// Estado de una ventana.
pub(crate) struct WindowInner {
    pub(crate) surface: wl_surface::WlSurface,
    pub(crate) xdg_surface: xdg_surface::XdgSurface,
    pub(crate) toplevel: xdg_toplevel::XdgToplevel,
    pub(crate) configured: bool,
    /// Tamaño lógico comunicado por el compositor.
    pub(crate) logical_width: u32,
    pub(crate) logical_height: u32,
    /// Escala del buffer (HiDPI entero).
    pub(crate) scale: u32,
    /// Memoria compartida subyacente.
    pub(crate) pixels: ShmPixels,
    pub(crate) pool: wl_shm_pool::WlShmPool,
    pub(crate) generation: u32,
    pub(crate) slots: Vec<bool>, // true = busy (adjunto al compositor)
    /// Buffers por slot del pool vigente (se crean bajo demanda).
    pub(crate) buffers: Vec<Option<wl_buffer::WlBuffer>>,
}

/// Pools obsoletos que esperan `wl_buffer.release` antes de destruirse.
///
/// `pixels` se conserva vivo: el pool/mapeo sigue referenciado por el
/// compositor hasta que libera los buffers.
#[allow(dead_code)]
pub(crate) struct ZombiePool {
    pub(crate) pixels: ShmPixels,
    pub(crate) pool: wl_shm_pool::WlShmPool,
    /// Proxies vivos de los slots; se destruyen al agotar los releases.
    pub(crate) buffers: Vec<Option<wl_buffer::WlBuffer>>,
    pub(crate) window: WindowId,
    pub(crate) generation: u32,
    pub(crate) pending: usize,
}

/// Estado del teclado (xkb).
pub(crate) struct KeyboardInner {
    context: xkb::Context,
    keymap: Option<xkb::Keymap>,
    state: Option<xkb::State>,
}

impl KeyboardInner {
    pub(crate) fn new() -> Self {
        Self {
            context: xkb::Context::new(0),
            keymap: None,
            state: None,
        }
    }

    /// Carga el keymap que envía el compositor desde un fd mmap-eado.
    /// El fd se cierra (OwnedFd) al salir, como exige el protocolo.
    pub(crate) fn load_keymap_from_fd(&mut self, fd: std::os::unix::io::OwnedFd, size: usize) {
        // SAFETY: mmap de solo lectura del fd del compositor, del tamaño que
        // él mismo indico; deshacemos el mapping tras copiar a String y
        // cerramos el fd (requisito del protocolo wl_keyboard.keymap).
        use std::os::unix::io::AsRawFd as _;
        let raw = fd.as_raw_fd();
        // SAFETY: mmap de solo lectura del fd del compositor, del tamaño que
        // él mismo indica; deshacemos el mapping tras copiar a String. El fd
        // lo cierra el Drop de OwnedFd al salir (requisito del protocolo).
        unsafe {
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ,
                libc::MAP_PRIVATE,
                raw,
                0,
            );
            if ptr == libc::MAP_FAILED || ptr.is_null() {
                return;
            }
            let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), size);
            let text = String::from_utf8_lossy(bytes).into_owned();
            libc::munmap(ptr, size);
            if let Some(keymap) =
                xkb::Keymap::new_from_string(&self.context, text, xkb::KEYMAP_FORMAT_TEXT_V1, 0)
            {
                self.state = Some(xkb::State::new(&keymap));
                self.keymap = Some(keymap);
            }
        }
    }

    /// Traduce un evento key a keysym/texto y actualiza el estado.
    pub(crate) fn key_event(&mut self, keycode: u32, pressed: bool) -> KeyEvent {
        let mut event = KeyEvent {
            keycode,
            keysym: 0,
            utf8: String::new(),
            pressed,
            modifiers: Modifiers::default(),
        };
        if let Some(state) = self.state.as_mut() {
            // xkb usa keycodes evdev + 8.
            state.update_key(
                xkb::Keycode::new(keycode + 8),
                if pressed {
                    xkb::KeyDirection::Down
                } else {
                    xkb::KeyDirection::Up
                },
            );
            event.keysym = u32::from(state.key_get_one_sym(xkb::Keycode::new(keycode + 8)));
            event.utf8 = state.key_get_utf8(xkb::Keycode::new(keycode + 8));
            event.modifiers = modifiers_from(state);
        }
        event
    }

    pub(crate) fn update_modifiers(
        &mut self,
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    ) {
        if let Some(state) = self.state.as_mut() {
            state.update_mask(depressed, latched, locked, 0, 0, group);
        }
    }
}

fn modifiers_from(state: &xkb::State) -> Modifiers {
    let active = |name: &str| state.mod_name_is_active(name, xkb::STATE_MODS_DEPRESSED);
    Modifiers {
        ctrl: active("Control"),
        shift: active("Shift"),
        alt: active("Mod1"),
        logo: active("Mod4"),
    }
}

/// Estado global de la conexión.
pub(crate) struct WaylandState {
    /// Handle de la cola (para crear objetos en `retire_pool`).
    pub(crate) qh: QueueHandle<WaylandState>,
    // Globals.
    pub(crate) compositor: Option<wl_compositor::WlCompositor>,
    pub(crate) shm: Option<wl_shm::WlShm>,
    pub(crate) wm_base: Option<xdg_wm_base::XdgWmBase>,
    pub(crate) seat: Option<wl_seat::WlSeat>,
    pub(crate) keyboard_proxy: Option<wl_keyboard::WlKeyboard>,
    pub(crate) pointer_proxy: Option<wl_pointer::WlPointer>,
    pub(crate) output_scale: HashMap<usize, u32>,

    // Ventanas.
    pub(crate) next_window: WindowId,
    pub(crate) windows: HashMap<WindowId, WindowInner>,
    pub(crate) zombies: Vec<ZombiePool>,

    // Entrada.
    pub(crate) keyboard: KeyboardInner,
    pub(crate) focused: Option<WindowId>,
    pub(crate) pointer_window: Option<WindowId>,
    pub(crate) scroll_dx: f64,
    pub(crate) scroll_dy: f64,

    /// Eventos pendientes de entregar al consumidor.
    pub(crate) events: Vec<WindowEvent>,
}

impl WaylandState {
    pub(crate) fn new(qh: QueueHandle<WaylandState>) -> Self {
        Self {
            qh,
            compositor: None,
            shm: None,
            wm_base: None,
            seat: None,
            keyboard_proxy: None,
            pointer_proxy: None,
            output_scale: HashMap::new(),
            next_window: 0,
            windows: HashMap::new(),
            zombies: Vec::new(),
            keyboard: KeyboardInner::new(),
            focused: None,
            pointer_window: None,
            scroll_dx: 0.0,
            scroll_dy: 0.0,
            events: Vec::new(),
        }
    }

    pub(crate) fn push_event(&mut self, event: WindowEvent) {
        self.events.push(event);
    }

    /// Escala entera máxima entre las salidas conocidas (HiDPI básico).
    pub(crate) fn max_scale(&self) -> u32 {
        self.output_scale
            .values()
            .copied()
            .max()
            .unwrap_or(1)
            .max(1)
    }

    pub(crate) fn window_of_surface(&self, surface: &wl_surface::WlSurface) -> Option<WindowId> {
        surface.data::<SurfaceUd>().map(|ud| ud.window)
    }
}

// ---------------------------------------------------------------------------
// Dispatch: registro de globals.
// ---------------------------------------------------------------------------

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name, interface, ..
            } => {
                match &interface[..] {
                    "wl_compositor" => {
                        state.compositor = Some(
                            registry.bind::<wl_compositor::WlCompositor, _, _>(name, 4, qh, ()),
                        );
                    }
                    "wl_shm" => {
                        state.shm = Some(registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ()));
                    }
                    "xdg_wm_base" => {
                        state.wm_base =
                            Some(registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 2, qh, ()));
                    }
                    "wl_seat" => {
                        // get_keyboard/get_pointer llegan con la capability (abajo).
                        state.seat = Some(registry.bind::<wl_seat::WlSeat, _, _>(name, 5, qh, ()));
                    }
                    "wl_output" => {
                        let index = state.output_scale.len();
                        state.output_scale.insert(index, 1);
                        let _output: wl_output::WlOutput =
                            registry.bind(name, 3, qh, OutputUd { index });
                    }
                    _ => {}
                }
            }
            wl_registry::Event::GlobalRemove { name } => {
                let _ = name; // v1: no hay baja de globals individuales
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatch: xdg shell.
// ---------------------------------------------------------------------------

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for WaylandState {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, SurfaceUd> for WaylandState {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &SurfaceUd,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
            if let Some(ud) = xdg_surface.data::<SurfaceUd>() {
                if let Some(window) = state.windows.get_mut(&ud.window) {
                    window.configured = true;
                    window
                        .surface
                        .set_buffer_scale(window.scale.try_into().unwrap_or(1));
                    let (w, h) = (window.logical_width, window.logical_height);
                    state.push_event(WindowEvent::Configure {
                        window: ud.window,
                        width: w,
                        height: h,
                    });
                }
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, SurfaceUd> for WaylandState {
    fn event(
        state: &mut Self,
        toplevel: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &SurfaceUd,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // El toplevel lleva SurfaceUd asignado al crearse en `window.rs`.
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                // 0 significa "sin cambios".
                if let Some(ud) = toplevel.data::<SurfaceUd>() {
                    if let Some(inner) = state.windows.get_mut(&ud.window) {
                        if width > 0 {
                            inner.logical_width = width as u32;
                        }
                        if height > 0 {
                            inner.logical_height = height as u32;
                        }
                    }
                }
            }
            xdg_toplevel::Event::Close => {
                if let Some(ud) = toplevel.data::<SurfaceUd>() {
                    state.push_event(WindowEvent::Close { window: ud.window });
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatch: buffers, frames, salida.
// ---------------------------------------------------------------------------

impl Dispatch<wl_buffer::WlBuffer, BufferUd> for WaylandState {
    fn event(
        state: &mut Self,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        ud: &BufferUd,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_buffer::Event::Release) {
            if let Some(window) = state.windows.get_mut(&ud.window) {
                if window.generation == ud.generation {
                    window.slots[ud.slot] = false;
                    return;
                }
            }
            // Pool obsoleto: contabiliza la baja del zombie correspondiente.
            if let Some(zombie) = state
                .zombies
                .iter_mut()
                .find(|z| z.pending > 0 && z.window == ud.window && z.generation == ud.generation)
            {
                zombie.pending -= 1;
            }
        }
    }
}

impl Dispatch<wl_callback::WlCallback, FrameUd> for WaylandState {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        ud: &FrameUd,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_callback::Event::Done { .. }) {
            state.push_event(WindowEvent::Frame { window: ud.window });
        }
    }
}

impl Dispatch<wl_output::WlOutput, OutputUd> for WaylandState {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        ud: &OutputUd,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Scale { factor } = event {
            state.output_scale.insert(ud.index, factor.max(1) as u32);
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatch: entrada.
// ---------------------------------------------------------------------------

impl Dispatch<wl_seat::WlSeat, ()> for WaylandState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        {
            if capabilities.contains(wl_seat::Capability::Keyboard) {
                state.keyboard_proxy = Some(seat.get_keyboard(qh, ()));
            }
            if capabilities.contains(wl_seat::Capability::Pointer) {
                state.pointer_proxy = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap {
                fd,
                format: WEnum::Value(format),
                size,
            } => {
                // Solo soportamos el keymap de texto XKB (formato 1).
                if matches!(format, wl_keyboard::KeymapFormat::XkbV1) {
                    state.keyboard.load_keymap_from_fd(fd, size as usize);
                }
                // Otros formatos: el OwnedFd se cierra solo al caer del scope.
            }
            wl_keyboard::Event::Enter { surface, .. } => {
                state.focused = state.window_of_surface(&surface);
            }
            wl_keyboard::Event::Leave { .. } => {
                state.focused = None;
            }
            wl_keyboard::Event::Key {
                key,
                state: WEnum::Value(key_state),
                ..
            } => {
                let pressed = matches!(key_state, wl_keyboard::KeyState::Pressed);
                if let Some(window) = state.focused {
                    let key = state.keyboard.key_event(key, pressed);
                    state.push_event(WindowEvent::Key { window, key });
                }
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                state
                    .keyboard
                    .update_modifiers(mods_depressed, mods_latched, mods_locked, group);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                if let Some(window) = state.window_of_surface(&surface) {
                    state.pointer_window = Some(window);
                    state.push_event(WindowEvent::PointerEnter {
                        window,
                        x: surface_x,
                        y: surface_y,
                    });
                }
            }
            wl_pointer::Event::Leave { .. } => {
                if let Some(window) = state.pointer_window.take() {
                    state.push_event(WindowEvent::PointerLeave { window });
                }
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                if let Some(window) = state.pointer_window {
                    state.push_event(WindowEvent::PointerMove {
                        window,
                        x: surface_x,
                        y: surface_y,
                    });
                }
            }
            wl_pointer::Event::Button {
                button,
                state: WEnum::Value(btn_state),
                ..
            } => {
                if let Some(window) = state.pointer_window {
                    let pressed = matches!(btn_state, wl_pointer::ButtonState::Pressed);
                    state.push_event(WindowEvent::PointerButton {
                        window,
                        button: MouseButton::from(button),
                        pressed,
                    });
                }
            }
            wl_pointer::Event::Axis {
                axis: WEnum::Value(axis),
                value,
                ..
            } => {
                let (dx, dy) = match axis {
                    wl_pointer::Axis::VerticalScroll => (0.0, value),
                    wl_pointer::Axis::HorizontalScroll => (value, 0.0),
                    _ => (0.0, 0.0),
                };
                state.scroll_dx += dx / 120.0;
                state.scroll_dy += dy / 120.0;
            }
            wl_pointer::Event::AxisSource { .. } => {}
            wl_pointer::Event::AxisStop {
                axis: WEnum::Value(axis),
                ..
            } => {
                let (dx, dy) = match axis {
                    wl_pointer::Axis::VerticalScroll => (0.0, std::mem::take(&mut state.scroll_dy)),
                    wl_pointer::Axis::HorizontalScroll => {
                        (std::mem::take(&mut state.scroll_dx), 0.0)
                    }
                    _ => (0.0, 0.0),
                };
                if let Some(window) = state.pointer_window {
                    if dx != 0.0 || dy != 0.0 {
                        state.push_event(WindowEvent::Scroll { window, dx, dy });
                    }
                }
            }
            wl_pointer::Event::Frame => {
                // Coalesceo simple: sin axis_stop, entregamos acumulado aquí.
                let (dx, dy) = (
                    std::mem::take(&mut state.scroll_dx),
                    std::mem::take(&mut state.scroll_dy),
                );
                if let Some(window) = state.pointer_window {
                    if dx != 0.0 || dy != 0.0 {
                        state.push_event(WindowEvent::Scroll { window, dx, dy });
                    }
                }
            }
            _ => {}
        }
    }
}

// Interfaces sin eventos relevantes.
delegate_noop!(WaylandState: ignore wl_compositor::WlCompositor);
delegate_noop!(WaylandState: ignore wl_shm::WlShm);
delegate_noop!(WaylandState: ignore wl_shm_pool::WlShmPool);

impl Dispatch<wl_surface::WlSurface, SurfaceUd> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &SurfaceUd,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // enter/leave de outputs: irrelevante para v1 (escala global).
    }
}

impl Drop for WaylandState {
    fn drop(&mut self) {
        // Destrucción ordenada de los objetos Wayland que poseemos.
        // (Aceptable al cierre del proceso; un compositor correcto tolera el
        // disconnect implícito, pero el destroy explícito es más limpio.)
        for (_, window) in self.windows.drain() {
            let _ = window.xdg_surface;
            for buffer in window.buffers.into_iter().flatten() {
                buffer.destroy();
            }
            window.pool.destroy();
            window.toplevel.destroy();
            window.surface.destroy();
        }
        for zombie in self.zombies.drain(..) {
            for buffer in zombie.buffers.into_iter().flatten() {
                if buffer.is_alive() {
                    buffer.destroy();
                }
            }
            if zombie.pool.is_alive() {
                zombie.pool.destroy();
            }
        }
    }
}
