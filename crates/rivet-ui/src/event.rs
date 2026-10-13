//! Despacho de eventos: entrada de window → eventos de widget con
//! hit-testing, estados hover/active, clic con *bubbling* y foco (doc §7).
//!
//! Fases v1: el clic se entrega al objetivo y **burbujea** hacia la raíz
//! hasta que un ancestro lo maneja (el primer botón de la cadena).

use crate::hit::hit_test;
use crate::widget::{state, WidgetId, WidgetKind};
use crate::Ui;

/// Entrada convertida de `rivet_window::WindowEvent` (sin coordinates
/// normalizadas de ventana: el ui recibe coords de superficie).
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
    /// Tecla de texto (UTF-8) al widget con foco.
    KeyText {
        /// Texto generado por la tecla.
        text: String,
        /// Keysym X (0 si no hay keymap).
        keysym: u32,
        /// `true` al presionar.
        pressed: bool,
    },
    /// Tecla Tab (avanza el foco).
    Tab,
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
            UiInput::KeyText {
                text,
                keysym,
                pressed,
            } => {
                if !pressed {
                    return;
                }
                let Some(focused) = ui.focus.focused(&ui.registry) else {
                    return;
                };
                let Ok(node) = ui.registry.node(focused) else {
                    return;
                };
                if node.kind == WidgetKind::TextInput {
                    ui.text_input_insert(focused, &text, keysym);
                }
            }
            UiInput::Tab => {
                let _ = ui.focus.tab_forward(&mut ui.registry);
            }
        }
    }
}
