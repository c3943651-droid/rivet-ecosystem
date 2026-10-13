//! Estado interno Wayland: globals, ventanas y todos los `Dispatch`.
//!
//! Los detalles del protocolo quedan encapsulados aquí; la API pública
//! (`crate::window`) expone `Wayland`/`Window`/`WindowEvent`.

use std::collections::HashMap;
use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer,
    wl_data_source, wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_surface, wl_touch,
};
use wayland_client::{
    delegate_noop, event_created_child, Connection, Dispatch, Proxy, QueueHandle, WEnum,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

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

/// Rol de la superficie (doc §4): toplevel normal o superficie anclada
/// (base para el panel y el lanzador).
pub(crate) enum Role {
    /// Ventana normal `xdg_toplevel`.
    Toplevel {
        xdg: xdg_surface::XdgSurface,
        toplevel: xdg_toplevel::XdgToplevel,
    },
    /// Superficie anclada `zwp_layer_shell_v1`.
    LayerShell {
        layer: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    },
}

/// Estado de una ventana.
/// Oferta de datos con sus MIMEs acumulados.
#[derive(Clone)]
pub(crate) struct OfferState {
    pub(crate) offer: wl_data_offer::WlDataOffer,
    pub(crate) mimes: Vec<String>,
}

pub(crate) struct WindowInner {
    pub(crate) surface: wl_surface::WlSurface,
    pub(crate) role: Role,
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
    pub(crate) touch_proxy: Option<wl_touch::WlTouch>,
    pub(crate) output_scale: HashMap<usize, u32>,
    pub(crate) layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,

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
    /// Último serial de entrada (necesario para set_selection/start_drag).
    pub(crate) last_serial: u32,
    /// Puntos de contacto activos: id -> ventana.
    pub(crate) touches: HashMap<i32, WindowId>,

    // Clipboard / DnD (doc §8).
    pub(crate) data_device_manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    pub(crate) data_device: Option<wl_data_device::WlDataDevice>,
    /// Texto que ofrecemos como selección (vive mientras el source viva).
    pub(crate) clipboard_text: Option<String>,
    /// Nuestro `wl_data_source` activo.
    pub(crate) own_source: Option<wl_data_source::WlDataSource>,
    /// Oferta de selección ajena con sus MIMEs.
    pub(crate) selection_offer: Option<OfferState>,
    /// Oferta recién anunciada (`data_offer`) a la espera de `selection`.
    pub(crate) pending_offer: Option<OfferState>,
    /// Oferta del drag activo con sus MIMEs.
    pub(crate) dnd_offer: Option<OfferState>,
    /// Ventana bajo el drag y dentro/fuera.
    pub(crate) dnd_window: Option<WindowId>,

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
            touch_proxy: None,
            output_scale: HashMap::new(),
            layer_shell: None,
            next_window: 0,
            windows: HashMap::new(),
            zombies: Vec::new(),
            keyboard: KeyboardInner::new(),
            focused: None,
            pointer_window: None,
            scroll_dx: 0.0,
            scroll_dy: 0.0,
            last_serial: 0,
            touches: HashMap::new(),
            data_device_manager: None,
            data_device: None,
            clipboard_text: None,
            own_source: None,
            selection_offer: None,
            pending_offer: None,
            dnd_offer: None,
            dnd_window: None,
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
                    "wl_data_device_manager" => {
                        state.data_device_manager = Some(
                            registry.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(
                                name,
                                3,
                                qh,
                                (),
                            ),
                        );
                    }
                    "zwp_layer_shell_v1" => {
                        state.layer_shell = Some(
                            registry.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(
                                name,
                                4,
                                qh,
                                (),
                            ),
                        );
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
            if capabilities.contains(wl_seat::Capability::Touch) {
                state.touch_proxy = Some(seat.get_touch(qh, ()));
            }
            // El data device se crea al llegar el seat si hay manager.
            if let Some(manager) = state.data_device_manager.as_ref() {
                state.data_device = Some(manager.get_data_device(seat, qh, ()));
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
            wl_keyboard::Event::Enter {
                serial, surface, ..
            } => {
                state.last_serial = serial;
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

impl Dispatch<wl_touch::WlTouch, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &wl_touch::WlTouch,
        event: wl_touch::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_touch::Event::Down {
                serial,
                surface,
                id,
                x,
                y,
                ..
            } => {
                state.last_serial = serial;
                if let Some(window) = state.window_of_surface(&surface) {
                    state.touches.insert(id, window);
                    state.push_event(WindowEvent::TouchDown { window, id, x, y });
                }
            }
            wl_touch::Event::Motion { id, x, y, .. } => {
                if let Some(&window) = state.touches.get(&id) {
                    state.push_event(WindowEvent::TouchMotion { window, id, x, y });
                }
            }
            wl_touch::Event::Up { id, .. } => {
                if let Some(window) = state.touches.remove(&id) {
                    state.push_event(WindowEvent::TouchUp { window, id });
                }
            }
            wl_touch::Event::Cancel => {
                for (_, window) in std::mem::take(&mut state.touches) {
                    state.push_event(WindowEvent::TouchCancel { window });
                }
            }
            _ => {} // frame/shape/orientación: agrupado o irrelevante en v1
        }
    }
}

// Interfaces sin eventos relevantes.
delegate_noop!(WaylandState: ignore wl_compositor::WlCompositor);
delegate_noop!(WaylandState: ignore wl_shm::WlShm);
delegate_noop!(WaylandState: ignore wl_shm_pool::WlShmPool);
delegate_noop!(WaylandState: ignore wl_data_device_manager::WlDataDeviceManager);
delegate_noop!(WaylandState: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

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
        for (_, window) in self.windows.drain() {
            for buffer in window.buffers.into_iter().flatten() {
                buffer.destroy();
            }
            window.pool.destroy();
            match window.role {
                Role::Toplevel { xdg, toplevel } => {
                    toplevel.destroy();
                    xdg.destroy();
                }
                Role::LayerShell { layer } => {
                    layer.destroy();
                }
            }
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
        if let Some(source) = self.own_source.take() {
            if source.is_alive() {
                source.destroy();
            }
        }
        for offer in [
            &mut self.selection_offer,
            &mut self.dnd_offer,
            &mut self.pending_offer,
        ]
        .into_iter()
        .flatten()
        {
            offer.offer.destroy();
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                // Los MIMEs llegan con eventos `offer` del nuevo objeto.
                state.pending_offer = Some(OfferState {
                    offer: id,
                    mimes: Vec::new(),
                });
            }
            wl_data_device::Event::Selection { id: Some(offer) } => {
                // La selección anuncia el offer recién creado (pending).
                state.selection_offer = state.pending_offer.take().or_else(|| {
                    Some(OfferState {
                        offer,
                        mimes: Vec::new(),
                    })
                });
            }
            wl_data_device::Event::Selection { id: None } => {
                state.selection_offer = None;
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => {
                state.last_serial = serial;
                if let Some(window) = state.window_of_surface(&surface) {
                    state.dnd_window = Some(window);
                    // El offer del drag puede llegar antes o con este enter.
                    if state.dnd_offer.is_none() {
                        state.dnd_offer = state.pending_offer.take().or_else(|| {
                            id.map(|offer| OfferState {
                                offer,
                                mimes: Vec::new(),
                            })
                        });
                    }
                    state.push_event(WindowEvent::DndEnter { window, x, y });
                }
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                if let Some(window) = state.dnd_window {
                    state.push_event(WindowEvent::DndMotion { window, x, y });
                }
            }
            wl_data_device::Event::Leave => {
                if let Some(window) = state.dnd_window.take() {
                    state.push_event(WindowEvent::DndLeave { window });
                }
            }
            wl_data_device::Event::Drop => {
                if let Some(window) = state.dnd_window {
                    state.push_event(WindowEvent::DndDrop { window });
                    // El app lee el texto con `Wayland::dnd_text_blocking`.
                }
            }
            _ => {}
        }
    }

    event_created_child!(WaylandState, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for WaylandState {
    fn event(
        state: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            // Acumula MIMEs en el offer que corresponda.
            for slot in [
                &mut state.pending_offer,
                &mut state.dnd_offer,
                &mut state.selection_offer,
            ]
            .into_iter()
            .flatten()
            {
                if slot.offer == *offer {
                    slot.mimes.push(mime_type);
                    break;
                }
            }
        }
        // source_actions/action: acciones DnD, fuera de v1 básico.
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for WaylandState {
    fn event(
        state: &mut Self,
        source: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { fd, .. } => {
                // Sirve el texto del clipboard al fd que pide el pegador.
                if let Some(text) = state.clipboard_text.clone() {
                    write_fd_text(fd, &text);
                }
                // El OwnedFd cierra el descriptor al caer del scope.
            }
            wl_data_source::Event::Cancelled => {
                // Reemplazaron nuestra selección: limpiar y destruir.
                state.clipboard_text = None;
                if state.own_source.as_ref().is_some_and(|s| s == source) {
                    state.own_source = None;
                }
                source.destroy();
            }
            _ => {} // target/dnd_*/action: fuera de v1 básico
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, SurfaceUd> for WaylandState {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &SurfaceUd,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                layer_surface.ack_configure(serial);
                if let Some(ud) = layer_surface.data::<SurfaceUd>() {
                    if let Some(window) = state.windows.get_mut(&ud.window) {
                        if width > 0 {
                            window.logical_width = width;
                        }
                        if height > 0 {
                            window.logical_height = height;
                        }
                        window.configured = true;
                        let (w, h) = (window.logical_width, window.logical_height);
                        state.push_event(WindowEvent::Configure {
                            window: ud.window,
                            width: w,
                            height: h,
                        });
                    }
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                if let Some(ud) = layer_surface.data::<SurfaceUd>() {
                    state.push_event(WindowEvent::Close { window: ud.window });
                }
            }
            _ => {}
        }
    }
}

/// Escribe `text` en un fd; el OwnedFd se cierra al salir (requisito del
/// protocolo: el fd es del pegador y solo se usa una vez).
fn write_fd_text(fd: std::os::unix::io::OwnedFd, text: &str) {
    use std::os::unix::io::AsRawFd as _;
    let bytes = text.as_bytes();
    let mut off = 0usize;
    // SAFETY: write sobre un fd abierto con buffer propio y tamaño correcto.
    unsafe {
        while off < bytes.len() {
            let n = libc::write(
                fd.as_raw_fd(),
                bytes[off..].as_ptr().cast(),
                bytes.len() - off,
            );
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }
            off += n as usize;
        }
    }
}
