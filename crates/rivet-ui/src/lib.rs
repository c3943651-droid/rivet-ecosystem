//! Rivet UI: widgets by composition, layout, events and focus.
//!
//! Two-level store (ADR-03): a common `WidgetNode` in a slab +
//! **slabs per kind** for kind-specific data. Children via intrusive
//! links (ADR-04), 64-bit IDs with generation. Styles looked up
//! by (kind, state) in flat O(1) tables from `rivet-style`; text via the
//! atlas from `rivet-typography`; drawing emitted to the `rivet-canvas` queue.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod app;
pub mod error;
pub mod event;
pub mod focus;
pub mod hit;
pub mod layout;
pub mod registry;
pub mod render;
pub mod text;
pub mod widget;

pub use app::Application;
pub use error::{Result, UiError};
pub use event::{EventDispatcher, UiInput};
pub use focus::FocusManager;
pub use registry::WidgetRegistry;
pub use text::TextRenderer;
pub use widget::{state, WidgetId, WidgetKind, WidgetNode};

use rivet_style::StyleTable;

/// Interfaz de usuario completa: registro + foco + texto + estilo.
///
/// El [`Application`](crate::app::Application) posee una `Ui` y la conecta
/// con Wayland/GL; en tests se puede usar sola (layout/render a una cola).
pub struct Ui {
    /// Árbol de widgets.
    pub registry: WidgetRegistry,
    /// Foco de teclado.
    pub focus: FocusManager,
    /// Estado del despachador (press en curso…).
    pub(crate) dispatcher: EventDispatcher,
    /// Texto: fuente, caché, atlas, texturas.
    pub text: TextRenderer,
    /// Tema compilado.
    pub style: StyleTable,
    /// Solicita terminar el bucle.
    quit_requested: bool,
}

impl Ui {
    /// UI con el tema indicado compilado del DSL.
    #[must_use]
    pub fn new(style_dsl: &str, theme: &str) -> Self {
        let style =
            rivet_style::compile(style_dsl, theme).unwrap_or_else(|_| default_style_table());
        Self {
            registry: WidgetRegistry::new(),
            focus: FocusManager::default(),
            dispatcher: EventDispatcher::default(),
            text: TextRenderer::new(),
            style,
            quit_requested: false,
        }
    }

    /// UI con el tema oscuro del DSL de ejemplo de rivet-style.
    #[must_use]
    pub fn new_dark() -> Self {
        Self::new(rivet_style::EXAMPLE_DSL, "dark")
    }

    /// Marca el árbol de raíz `root` para redibujar (todo sucio).
    pub fn invalidate(&mut self, root: Option<WidgetId>) {
        let root = root.or_else(|| self.registry.root());
        if let Some(root) = root {
            let mut stack = vec![root];
            while let Some(id) = stack.pop() {
                if !self.registry.is_alive(id) {
                    continue;
                }
                if let Ok(node) = self.registry.node_mut(id) {
                    node.dirty = true;
                }
                stack.extend(self.registry.children(id));
            }
        }
    }

    /// Solicita cerrar la aplicación.
    pub fn quit(&mut self) {
        self.quit_requested = true;
    }

    /// `true` si se pidió cerrar.
    #[must_use]
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// Última posición conocida del puntero (para completar clicks).
    #[must_use]
    pub fn last_pointer(&self) -> Option<(f32, f32)> {
        self.dispatcher.last_pointer
    }

    // ------------------------------------------------------------------
    // Internos del despacho de eventos.
    // ------------------------------------------------------------------

    pub(crate) fn set_state_bit(&mut self, id: WidgetId, bit: u8) {
        if let Ok(node) = self.registry.node_mut(id) {
            node.state |= bit;
            node.dirty = true;
        }
    }

    pub(crate) fn clear_state_bit(&mut self, id: WidgetId, bit: u8) {
        if let Ok(node) = self.registry.node_mut(id) {
            node.state &= !bit;
            node.dirty = true;
        }
    }

    /// Limpia el hover global y lo fija en la cadena de `target`.
    pub(crate) fn update_hover(&mut self, root: WidgetId, target: Option<WidgetId>) {
        // v1: recorrer todo el árbol y reflejar hover (árboles pequeños).
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !self.registry.is_alive(id) {
                continue;
            }
            let hovered = target.is_some_and(|t| self.registry.is_descendant(t, id) || t == id);
            let node = match self.registry.node_mut(id) {
                Ok(node) => node,
                Err(_) => continue,
            };
            let should = hovered && node.state & state::DISABLED == 0;
            let was = node.state & state::HOVER != 0;
            if should != was {
                node.state ^= state::HOVER;
                node.dirty = true;
            }
            stack.extend(self.registry.children(id));
        }
    }

    pub(crate) fn is_clickable(&self, id: WidgetId) -> bool {
        self.registry
            .node(id)
            .map(|node| node.kind == WidgetKind::Button && node.state & state::DISABLED == 0)
            .unwrap_or(false)
    }

    /// Burbujea el clic hacia la raíz hasta el primer botón (doc §7).
    pub(crate) fn fire_click(&mut self, target: WidgetId) {
        let mut current = Some(target);
        while let Some(id) = current {
            if !self.registry.is_alive(id) {
                break;
            }
            let is_button = self
                .registry
                .node(id)
                .map(|n| n.kind == WidgetKind::Button)
                .unwrap_or(false);
            if is_button {
                // Se saca el callback antes de invocarlo: el handler
                // recibe &mut Ui completo (puede rearmar el árbol).
                let mut on_click = self
                    .registry
                    .button_mut(id)
                    .ok()
                    .and_then(|button| button.on_click.take());
                if let Some(handler) = on_click.as_mut() {
                    handler(self);
                }
                if let Ok(button) = self.registry.button_mut(id) {
                    button.on_click = on_click;
                }
                return;
            }
            current = self.registry.node(id).ok().and_then(|n| n.parent);
        }
    }

    pub(crate) fn focus_by_widget(&mut self, id: WidgetId) {
        let focusable = self
            .registry
            .node(id)
            .map(|node| {
                matches!(node.kind, WidgetKind::Button | WidgetKind::TextInput)
                    && node.state & state::DISABLED == 0
            })
            .unwrap_or(false);
        if !focusable {
            // El contenedor con foco por puntero: buscar el primer
            // interactivo dentro.
            let mut found = None;
            let mut stack = vec![id];
            while let Some(candidate) = stack.pop() {
                if !self.registry.is_alive(candidate) {
                    continue;
                }
                if let Ok(node) = self.registry.node(candidate) {
                    if matches!(node.kind, WidgetKind::Button | WidgetKind::TextInput)
                        && node.state & state::DISABLED == 0
                    {
                        found = Some(candidate);
                        break;
                    }
                    stack.extend(self.registry.children(candidate));
                }
            }
            match found {
                Some(f) => {
                    let _ = self.focus.focus(&mut self.registry, f);
                }
                None => self.focus.blur(&mut self.registry),
            }
            return;
        }
        let _ = self.focus.focus(&mut self.registry, id);
    }

    pub(crate) fn text_input_insert(&mut self, id: WidgetId, text: &str, keysym: u32) {
        // Backspace (keysym 0xFF08) borra; el resto se inserta.
        if keysym == 0xFF08 {
            if let Ok(input) = self.registry.text_input_mut(id) {
                input.text.pop();
            }
        } else {
            if let Ok(input) = self.registry.text_input_mut(id) {
                input.text.push_str(text);
            }
        }
        let payload = self
            .registry
            .text_input(id)
            .map(|input| input.text.clone())
            .unwrap_or_default();
        let mut callback = self
            .registry
            .text_input_mut(id)
            .ok()
            .and_then(|input| input.on_change.take());
        if let Some(on_change) = callback.as_mut() {
            (on_change)(&payload, self);
        }
        if let Ok(input) = self.registry.text_input_mut(id) {
            input.on_change = callback;
        }
    }
}

/// Tabla con `WidgetStyle::default()` en todas las celdas (sin DSL):
/// respaldo si el tema pedido no compila.
#[must_use]
pub fn default_style_table() -> StyleTable {
    rivet_style::compile("theme neutral {}", "neutral").expect("DSL mínimo siempre compila")
}
