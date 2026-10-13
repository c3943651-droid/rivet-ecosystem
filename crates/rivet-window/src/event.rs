//! Eventos de ventana traducidos al dominio Rivet.
//!
//! `docs/rivet-window.md` §6: los eventos Wayland se traducen a tipos propios
//! y se entregan al consumidor; `rivet-window` no decide foco ni hit-testing.

/// Identificador de ventana.
pub type WindowId = u64;

/// Botón del puntero (códigos `BTN_*` de Linux input).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Botón primario (BTN_LEFT).
    Left,
    /// Botón secundario (BTN_RIGHT).
    Right,
    /// Botón central (BTN_MIDDLE).
    Middle,
    /// Otro botón (código `BTN_*` crudo).
    Other(u32),
}

impl From<u32> for MouseButton {
    fn from(code: u32) -> Self {
        match code {
            0x110 => MouseButton::Left,
            0x111 => MouseButton::Right,
            0x112 => MouseButton::Middle,
            other => MouseButton::Other(other),
        }
    }
}

/// Estado de los modificadores del teclado.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Ctrl.
    pub ctrl: bool,
    /// Shift.
    pub shift: bool,
    /// Alt.
    pub alt: bool,
    /// Logo/Super.
    pub logo: bool,
}

/// Evento de teclado con traducción xkb ya aplicada.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    /// Código evdev crudo (sin offset de xkb).
    pub keycode: u32,
    /// Keysym X (XKB_KEY_*) o 0 si no hay keymap.
    pub keysym: u32,
    /// Texto UTF-8 generado por la tecla (puede ser vacío).
    pub utf8: String,
    /// `true` al presionar, `false` al soltar.
    pub pressed: bool,
    /// Modificadores activos.
    pub modifiers: Modifiers,
}

/// Evento de ventana unificado.
#[derive(Debug, Clone)]
pub enum WindowEvent {
    /// El compositor configuró/redimensionó la ventana (tamaño lógico).
    Configure {
        /// Ventana afectada.
        window: WindowId,
        /// Ancho lógico.
        width: u32,
        /// Alto lógico.
        height: u32,
    },
    /// El usuario pidió cerrar la ventana.
    Close {
        /// Ventana afectada.
        window: WindowId,
    },
    /// Callback de frame: buen momento para redibujar y presentar.
    Frame {
        /// Ventana afectada.
        window: WindowId,
    },
    /// El puntero entró en la ventana.
    PointerEnter {
        /// Ventana afectada.
        window: WindowId,
        /// X lógica.
        x: f64,
        /// Y lógica.
        y: f64,
    },
    /// El puntero se movió dentro de la ventana.
    PointerMove {
        /// Ventana afectada.
        window: WindowId,
        /// X lógica.
        x: f64,
        /// Y lógica.
        y: f64,
    },
    /// El puntero salió de la ventana.
    PointerLeave {
        /// Ventana afectada.
        window: WindowId,
    },
    /// Botón del puntero presionado/soltado.
    PointerButton {
        /// Ventana afectada.
        window: WindowId,
        /// Botón.
        button: MouseButton,
        /// `true` al presionar.
        pressed: bool,
    },
    /// Rueda de scroll.
    Scroll {
        /// Ventana afectada.
        window: WindowId,
        /// Delta horizontal en "clics de rueda".
        dx: f64,
        /// Delta vertical en "clics de rueda" (positivo hacia abajo).
        dy: f64,
    },
    /// Tecla presionada/soltada (con foco de teclado en la ventana).
    Key {
        /// Ventana con foco.
        window: WindowId,
        /// Tecla traducida.
        key: KeyEvent,
    },
    /// Punto de contacto iniciado en la ventana.
    TouchDown {
        /// Ventana afectada.
        window: WindowId,
        /// ID del punto de contacto (multi-touch).
        id: i32,
        /// X lógica.
        x: f64,
        /// Y lógica.
        y: f64,
    },
    /// Punto de contacto movido.
    TouchMotion {
        /// Ventana afectada.
        window: WindowId,
        /// ID del punto de contacto.
        id: i32,
        /// X lógica.
        x: f64,
        /// Y lógica.
        y: f64,
    },
    /// Punto de contacto levantado.
    TouchUp {
        /// Ventana afectada.
        window: WindowId,
        /// ID del punto de contacto.
        id: i32,
    },
    /// El compositor canceló los puntos de contacto (p. ej. gesture).
    TouchCancel {
        /// Ventana afectada.
        window: WindowId,
    },
    /// Arrastre (DnD) entró en una ventana.
    DndEnter {
        /// Ventana afectada.
        window: WindowId,
        /// X lógica.
        x: f64,
        /// Y lógica.
        y: f64,
    },
    /// Arrastre se movió dentro de la ventana.
    DndMotion {
        /// Ventana afectada.
        window: WindowId,
        /// X lógica.
        x: f64,
        /// Y lógica.
        y: f64,
    },
    /// Arrastre salió de la ventana.
    DndLeave {
        /// Ventana afectada.
        window: WindowId,
    },
    /// Arrastre soltado en la ventana; leer el contenido con
    /// `Wayland::dnd_text_blocking`.
    DndDrop {
        /// Ventana afectada.
        window: WindowId,
    },
}
