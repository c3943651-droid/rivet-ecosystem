//! Event dispatch: window input → widget events with
//! hit-testing, hover/active states, click with *bubbling* and focus (doc §7).
//!
//! Keyboard: every key (text, special and with modifiers) arrives as
//! [`UiInput::Key`]; the dispatcher resolves Tab (focus), Escape (blur),
//! Button activation (Enter/Space) and `TextInput` editing (cursor-aware).
//! A global handler installed with [`Ui::set_key_handler`] runs first and
//! may consume shortcuts (Ctrl+S…).

use rivet_window::Modifiers;

use crate::hit::hit_test;
use crate::widget::{state, WidgetId, WidgetKind};
use crate::Ui;

/// Keysyms X usados por el despachador (XKB_KEY_*).
pub mod keysym {
    /// Retroceso (Backspace).
    pub const BACKSPACE: u32 = 0xFF08;
    /// Tabulación.
    pub const TAB: u32 = 0xFF09;
    /// Intro/Enter.
    pub const RETURN: u32 = 0xFF0D;
    /// Escape.
    pub const ESCAPE: u32 = 0xFF1B;
    /// Inicio.
    pub const HOME: u32 = 0xFF50;
    /// Flecha izquierda.
    pub const LEFT: u32 = 0xFF51;
    /// Flecha arriba.
    pub const UP: u32 = 0xFF52;
    /// Flecha derecha.
    pub const RIGHT: u32 = 0xFF53;
    /// Flecha abajo.
    pub const DOWN: u32 = 0xFF54;
    /// Fin.
    pub const END: u32 = 0xFF57;
    /// Suprimir (Delete forward).
    pub const DELETE: u32 = 0xFFFF;
}

/// Entrada convertida de `rivet_window::WindowEvent` (sin coordinates
/// normalizadas de ventana: el ui recibe coords de superficie).
///
/// Las coordenadas de puntero son **físicas** (lógico × escala): el árbol
/// de layout también vive en píxeles físicos.
#[derive(Debug, Clone, PartialEq)]
pub enum UiInput {
    /// Puntero movido a `(x, y)` (coordenadas lógicas del widget raíz).
    PointerMove {
        /// X lógica.
        x: f32,
        /// Y lógica.
        y: f32,
    },
    /// Botón presionado.
    PointerDown {
        /// X lógica.
        x: f32,
        /// Y lógica.
        y: f32,
    },
    /// Botón soltado.
    PointerUp {
        /// X lógica.
        x: f32,
        /// Y lógica.
        y: f32,
    },
    /// Rueda de scroll (delta en clics de rueda; Y positiva hacia abajo).
    Scroll {
        /// X física del puntero.
        x: f32,
        /// Y física del puntero.
        y: f32,
        /// Delta horizontal (clics de rueda).
        dx: f32,
        /// Delta vertical (clics de rueda).
        dy: f32,
    },
    /// Tecla (texto, especial o con modificadores) al árbol.
    Key {
        /// Keysym X (0 si no hay keymap).
        keysym: u32,
        /// Texto UTF-8 generado por la tecla (vacío para no imprimibles).
        text: String,
        /// Modificadores activos.
        modifiers: Modifiers,
        /// `true` al presionar.
        pressed: bool,
    },
}

/// Tecla entregada al handler global de [`Ui::set_key_handler`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInput {
    /// Keysym X (0 si no hay keymap).
    pub keysym: u32,
    /// Texto UTF-8 generado por la tecla (vacío para no imprimibles).
    pub text: String,
    /// Modificadores activos.
    pub modifiers: Modifiers,
    /// `true` al presionar.
    pub pressed: bool,
}

/// Despachador: mantiene el objetivo de press entre down/up.
#[derive(Debug, Default)]
pub struct EventDispatcher {
    pressed_target: Option<WidgetId>,
    /// Última posición del puntero (para completar clicks).
    pub(crate) last_pointer: Option<(f32, f32)>,
}

impl EventDispatcher {
    /// Despacha una entrada sobre el árbol de `ui`.
    pub fn dispatch(ui: &mut Ui, input: UiInput) {
        let root = match ui.registry.root() {
            Some(root) => root,
            None => return,
        };
        match input {
            UiInput::PointerMove { x, y } => {
                ui.dispatcher.last_pointer = Some((x, y));
                let target = hit_test(&ui.registry, root, x, y);
                ui.update_hover(root, target);
            }
            UiInput::PointerDown { x, y } => {
                let target = hit_test(&ui.registry, root, x, y);
                ui.dispatcher.pressed_target = target;
                // Active en la cadena objetivo; foco por puntero.
                if let Some(id) = target {
                    ui.set_state_bit(id, state::ACTIVE);
                    ui.focus_by_widget(id);
                    // Clic directo sobre un TextInput: posicionar cursor.
                    if ui
                        .registry
                        .node(id)
                        .map(|n| n.kind == WidgetKind::TextInput)
                        .unwrap_or(false)
                    {
                        ui.text_input_click_cursor(id, x, y);
                    }
                }
            }
            UiInput::PointerUp { x, y } => {
                let target = hit_test(&ui.registry, root, x, y);
                let pressed = ui.dispatcher.pressed_target.take();
                // Clic completo: mismo widget en down y up, no deshabilitado.
                if let (Some(pressed), Some(released)) = (pressed, target) {
                    if pressed == released && ui.is_clickable(released) {
                        ui.clear_state_bit(released, state::ACTIVE);
                        ui.fire_click(released);
                    } else {
                        ui.clear_state_bit(pressed, state::ACTIVE);
                    }
                }
            }
            UiInput::Key {
                keysym,
                text,
                modifiers,
                pressed,
            } => {
                if !pressed {
                    return;
                }
                dispatch_key(
                    ui,
                    KeyInput {
                        keysym,
                        text,
                        modifiers,
                        pressed,
                    },
                );
            }
            UiInput::Scroll { x, y, dy, .. } => {
                // El ancestro con scroll más profundo bajo el puntero
                // consume el desplazamiento (v0.3).
                let target = hit_test(&ui.registry, root, x, y);
                if let Some(id) = target {
                    ui.scroll_ancestor(id, dy);
                }
            }
        }
    }
}

/// Resolución de una tecla: hook global → navegación → widget con foco.
fn dispatch_key(ui: &mut Ui, key: KeyInput) {
    // Handler global primero (atajos Ctrl+S…); puede consumir la tecla.
    let mut handler = ui.key_handler.take();
    let consumed = handler.as_mut().is_some_and(|h| h(&key, ui));
    ui.key_handler = handler;
    if consumed {
        return;
    }

    // Navegación de foco y desenfoque global.
    match key.keysym {
        keysym::TAB => {
            if key.modifiers.shift {
                let _ = ui.focus.tab_backward(&mut ui.registry);
            } else {
                let _ = ui.focus.tab_forward(&mut ui.registry);
            }
            return;
        }
        keysym::ESCAPE => {
            ui.focus.blur(&mut ui.registry);
            return;
        }
        _ => {}
    }

    let Some(focused) = ui.focus.focused(&ui.registry) else {
        return;
    };
    let Ok(node) = ui.registry.node(focused) else {
        return;
    };
    match node.kind {
        WidgetKind::TextInput => ui.text_input_key(focused, &key),
        // Enter/Space activan el botón enfocado (doc §7).
        WidgetKind::Button if key.keysym == keysym::RETURN || key.keysym == u32::from(b' ') => {
            ui.fire_click(focused);
        }
        _ => {}
    }
}
