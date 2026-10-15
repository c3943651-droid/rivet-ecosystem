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
pub mod list;
pub mod registry;
pub mod render;
pub mod text;
pub mod widget;

pub use app::{Application, WindowIndex};
pub use error::{Result, UiError};
pub use event::{keysym, EventDispatcher, KeyInput, UiInput};
pub use focus::FocusManager;
pub use list::ListView;
pub use registry::WidgetRegistry;
pub use rivet_window::Modifiers;
pub use text::TextRenderer;
pub use widget::{state, WidgetId, WidgetKind, WidgetNode};

use std::any::{Any, TypeId};
use std::collections::HashMap;

use rivet_style::StyleTable;

/// Handler global de teclas (atajos): `true` consume la tecla.
pub type OnKey = Box<dyn FnMut(&KeyInput, &mut Ui) -> bool>;

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
    /// Atajos globales (Ctrl+S…); se ejecuta antes que el widget con foco.
    pub(crate) key_handler: Option<OnKey>,
    /// Estado tipado de la app (`Any` + downcast; v0.2): evita delegar
    /// todo en `Rc<RefCell<…>>` desde los callbacks.
    state: HashMap<TypeId, Box<dyn Any>>,
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
            key_handler: None,
            state: HashMap::new(),
            quit_requested: false,
        }
    }

    /// UI con el tema oscuro por defecto de rivet-ui (v0.3: paleta
    /// coherente panel/label/button/text-input).
    #[must_use]
    pub fn new_dark() -> Self {
        Self::new(DEFAULT_DARK_DSL, "dark")
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

    /// `true` si algún widget del árbol está sucio (necesita repintar).
    ///
    /// El bucle de `Application` consulta esto tras despachar eventos:
    /// frames sin cambios no se renderizan (v0.2; antes se repintaba por
    /// cada evento de puntero).
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        let Some(root) = self.registry.root() else {
            return false;
        };
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let Ok(node) = self.registry.node(id) else {
                continue;
            };
            if node.dirty {
                return true;
            }
            stack.extend(self.registry.children(id));
        }
        false
    }

    /// Limpia los bits sucios del árbol (tras presentar un frame).
    pub fn clear_dirty(&mut self) {
        let Some(root) = self.registry.root() else {
            return;
        };
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !self.registry.is_alive(id) {
                continue;
            }
            if let Ok(node) = self.registry.node_mut(id) {
                node.dirty = false;
            }
            stack.extend(self.registry.children(id));
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
    ///
    /// Coordenadas **físicas** (lógico × escala): el mismo espacio que
    /// los rects del layout.
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
            // Hover en la cadena de ANCESTROS del objetivo (incluido él):
            // `is_descendant(ancestor, start)` sube desde `start`; aquí
            // subimos desde `target` buscando `id` (v0.2: antes los
            // argumentos invertidos marcaban a los descendientes).
            let hovered = target.is_some_and(|t| self.registry.is_descendant(id, t));
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

    /// Instala un handler global de teclas (atajos como Ctrl+S).
    ///
    /// Se ejecuta **antes** que el widget con foco; devolver `true`
    /// consume la tecla. Solo puede haber uno (lo reemplaza).
    pub fn set_key_handler<F>(&mut self, handler: F)
    where
        F: FnMut(&KeyInput, &mut Ui) -> bool + 'static,
    {
        self.key_handler = Some(Box::new(handler));
    }

    // ------------------------------------------------------------------
    // Estado tipado de la aplicación (v0.2).
    // ------------------------------------------------------------------

    /// Guarda un valor de estado tipado (reemplaza el anterior del mismo
    /// tipo y lo devuelve).
    ///
    /// ```ignore
    /// ui.insert_state(NotebookState::default());
    /// ui.state_mut::<NotebookState>().unwrap().dirty = true;
    /// ```
    pub fn insert_state<T: Any>(&mut self, value: T) -> Option<T> {
        self.state
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|old| old.downcast::<T>().ok().map(|boxed| *boxed))
    }

    /// Referencia al estado del tipo `T` (si se insertó antes).
    #[must_use]
    pub fn state<T: Any>(&self) -> Option<&T> {
        self.state.get(&TypeId::of::<T>())?.downcast_ref::<T>()
    }

    /// Referencia mutable al estado del tipo `T` (si se insertó antes).
    pub fn state_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.state.get_mut(&TypeId::of::<T>())?.downcast_mut::<T>()
    }

    // ------------------------------------------------------------------
    // Scroll (v0.3).
    // ------------------------------------------------------------------

    /// Rueda sobre `from`: el ancestro con scroll más profundo consume el
    /// desplazamiento (clamp al rango del contenido).
    pub(crate) fn scroll_ancestor(&mut self, from: WidgetId, wheel_dy: f32) {
        const WHEEL_STEP: f32 = 48.0;
        let mut current = Some(from);
        while let Some(id) = current {
            let scrollable = self
                .registry
                .container(id)
                .map(|c| c.scroll)
                .unwrap_or(false);
            if scrollable {
                let container = self.registry.container_mut(id).expect("presente");
                let max = (container.content_h - container.viewport_h).max(0.0);
                let target = (container.scroll_y + wheel_dy * WHEEL_STEP).clamp(0.0, max);
                if target != container.scroll_y {
                    container.scroll_y = target;
                    self.mark_dirty(id);
                }
                return;
            }
            current = self.registry.node(id).ok().and_then(|n| n.parent);
        }
    }

    // ------------------------------------------------------------------
    // Foco programático (v0.2).
    // ------------------------------------------------------------------
    /// Enfoca un widget interactivo (o el primero dentro de un contenedor).
    pub fn focus_widget(&mut self, id: WidgetId) {
        self.focus_by_widget(id);
    }

    /// Avanza el foco al siguiente interactivo (como Tab).
    pub fn focus_next(&mut self) {
        let _ = self.focus.tab_forward(&mut self.registry);
    }

    /// Retrocede el foco al interactivo anterior (como Shift+Tab).
    pub fn focus_previous(&mut self) {
        let _ = self.focus.tab_backward(&mut self.registry);
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

    // ------------------------------------------------------------------
    // Edición de TextInput (cursor-aware).
    // ------------------------------------------------------------------

    /// Aplica una tecla sobre el `TextInput` enfocado (edición con cursor).
    ///
    /// Las combinaciones con modificadores (Ctrl/Alt/Logo) se ignoran aquí:
    /// las resuelve la app (portapapeles, atajos) o el handler global.
    pub(crate) fn text_input_key(&mut self, id: WidgetId, key: &KeyInput) {
        if key.modifiers.ctrl || key.modifiers.alt || key.modifiers.logo {
            return;
        }
        self.mark_dirty(id);
        let multiline = self
            .registry
            .text_input(id)
            .map(|input| input.multiline)
            .unwrap_or(false);
        let changed = match key.keysym {
            keysym::BACKSPACE => self.text_input_backspace(id),
            keysym::DELETE => self.text_input_delete_forward(id),
            keysym::LEFT => self.text_input_move_cursor(id, -1),
            keysym::RIGHT => self.text_input_move_cursor(id, 1),
            keysym::UP if multiline => self.text_input_move_line(id, -1),
            keysym::DOWN if multiline => self.text_input_move_line(id, 1),
            keysym::HOME | keysym::UP => self.text_input_set_cursor(id, 0),
            keysym::END | keysym::DOWN => {
                let len = self
                    .registry
                    .text_input(id)
                    .map(|input| input.text.len())
                    .unwrap_or(0);
                self.text_input_set_cursor(id, len);
                false
            }
            keysym::RETURN if multiline => {
                self.text_input_insert_at_cursor(id, "\n");
                true
            }
            _ => {
                if key.text.is_empty() {
                    false
                } else {
                    self.text_input_insert_at_cursor(id, &key.text);
                    true
                }
            }
        };
        if changed {
            self.notify_text_change(id);
        }
    }

    /// Inserta texto en la posición del cursor y notifica `on_change`.
    pub fn text_input_insert_text(&mut self, id: WidgetId, text: &str) {
        self.text_input_insert_at_cursor(id, text);
        self.mark_dirty(id);
        self.notify_text_change(id);
    }

    fn text_input_insert_at_cursor(&mut self, id: WidgetId, text: &str) {
        if let Ok(input) = self.registry.text_input_mut(id) {
            let cursor = input.cursor.min(input.text.len());
            input.text.insert_str(cursor, text);
            input.cursor = cursor + text.len();
        }
    }

    /// Texto del input (para copiar al portapapeles desde la app).
    #[must_use]
    pub fn text_input_text(&self, id: WidgetId) -> Option<&str> {
        self.registry.text_input(id).ok().map(|d| d.text.as_str())
    }

    /// Reemplaza todo el contenido y recoloca el cursor al final
    /// (notifica `on_change`).
    pub fn text_input_set_text(&mut self, id: WidgetId, text: &str) {
        if let Ok(input) = self.registry.text_input_mut(id) {
            input.set_text(text);
        }
        self.mark_dirty(id);
        self.notify_text_change(id);
    }

    fn mark_dirty(&mut self, id: WidgetId) {
        if let Ok(node) = self.registry.node_mut(id) {
            node.dirty = true;
        }
    }

    fn text_input_backspace(&mut self, id: WidgetId) -> bool {
        if let Ok(input) = self.registry.text_input_mut(id) {
            let cursor = input.cursor.min(input.text.len());
            if cursor > 0 {
                let boundary = prev_boundary(&input.text, cursor);
                input.text.replace_range(boundary..cursor, "");
                input.cursor = boundary;
                return true;
            }
        }
        false
    }

    fn text_input_delete_forward(&mut self, id: WidgetId) -> bool {
        if let Ok(input) = self.registry.text_input_mut(id) {
            let cursor = input.cursor.min(input.text.len());
            if cursor < input.text.len() {
                let boundary = next_boundary(&input.text, cursor);
                input.text.replace_range(cursor..boundary, "");
                return true;
            }
        }
        false
    }

    fn text_input_move_cursor(&mut self, id: WidgetId, delta: i32) -> bool {
        if let Ok(input) = self.registry.text_input_mut(id) {
            let cursor = input.cursor.min(input.text.len());
            input.cursor = if delta < 0 {
                prev_boundary(&input.text, cursor)
            } else {
                next_boundary(&input.text, cursor)
            };
        }
        false // mover el cursor no notifica on_change
    }

    /// Mueve el cursor una línea arriba/abajo (multilínea, v0.3),
    /// conservando la columna con clamp al largo de la línea destino.
    fn text_input_move_line(&mut self, id: WidgetId, delta: i32) -> bool {
        if let Ok(input) = self.registry.text_input_mut(id) {
            let text = &input.text;
            let cursor = input.cursor.min(text.len());
            let (current_start, _) = line_span(text, line_of(text, cursor));
            let col = cursor - current_start;
            let target_line = (line_of(text, cursor) as i32 + delta).max(0) as usize;
            let total_lines = text.split('\n').count();
            let target_line = target_line.min(total_lines.saturating_sub(1));
            let (target_start, target_end) = line_span(text, target_line);
            input.cursor = target_start + col.min(target_end - target_start);
        }
        false // mover el cursor no notifica on_change
    }

    fn text_input_set_cursor(&mut self, id: WidgetId, position: usize) -> bool {
        if let Ok(input) = self.registry.text_input_mut(id) {
            input.cursor = position.min(input.text.len());
        }
        false
    }

    /// Posiciona el cursor por clic: boundary más cercano a `(x, y)`
    /// físicos (multilínea: la Y selecciona la línea).
    pub(crate) fn text_input_click_cursor(&mut self, id: WidgetId, x: f32, y: f32) {
        let (rect, text, size, padding) = {
            let Ok(node) = self.registry.node(id) else {
                return;
            };
            let rect = node.rect;
            let widget_style = self.style.get(
                rivet_style::WidgetKind::TextInput,
                rivet_style::WidgetState::NORMAL,
            );
            (
                rect,
                self.registry
                    .text_input(id)
                    .map(|input| input.text.clone())
                    .unwrap_or_default(),
                widget_style.font_size,
                widget_style.padding,
            )
        };
        let multiline = self
            .registry
            .text_input(id)
            .map(|input| input.multiline)
            .unwrap_or(false);
        let x_local = x - rect.x - padding.left;
        if multiline {
            // Multilínea: la Y elige la línea; la X, la frontera dentro
            // de esa línea (v0.3).
            let line_h = self.text.line_height(size);
            let y_local = y - rect.y - padding.top;
            let total = text.split('\n').count().max(1);
            let line = ((y_local.max(0.0) / line_h).floor() as usize).min(total - 1);
            let (start, end) = line_span(&text, line);
            let line_text = &text[start..end];
            let mut best = start;
            let mut best_dist = f32::INFINITY;
            let mut boundary = start;
            loop {
                let width = if boundary == start {
                    0.0
                } else {
                    self.text.layout(&text[start..boundary], size).width
                };
                let dist = (x_local - width).abs();
                if dist < best_dist {
                    best_dist = dist;
                    best = boundary;
                }
                if boundary >= end {
                    break;
                }
                boundary = next_boundary(line_text, boundary - start) + start;
            }
            if let Ok(input) = self.registry.text_input_mut(id) {
                input.cursor = best;
            }
        } else {
            let mut best = 0usize;
            let mut best_dist = f32::INFINITY;
            let mut boundary = 0usize;
            loop {
                let width = if boundary == 0 {
                    0.0
                } else {
                    self.text.layout(&text[..boundary], size).width
                };
                let dist = (x_local - width).abs();
                if dist < best_dist {
                    best_dist = dist;
                    best = boundary;
                }
                if boundary >= text.len() {
                    break;
                }
                boundary = next_boundary(&text, boundary);
            }
            if let Ok(input) = self.registry.text_input_mut(id) {
                input.cursor = best;
            }
        }
        self.mark_dirty(id);
    }

    /// Notifica `on_change` con el texto vigente (patrón take/restore de
    /// `fire_click`: el handler recibe `&mut Ui` completo).
    fn notify_text_change(&mut self, id: WidgetId) {
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

/// Frontera de char previa (estrictamente hacia atrás) a partir de `index`.
fn prev_boundary(text: &str, index: usize) -> usize {
    let mut i = index.min(text.len()).saturating_sub(1);
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Frontera de char siguiente (estrictamente hacia delante) a partir de
/// `index`; queda en `len` al llegar al final.
fn next_boundary(text: &str, index: usize) -> usize {
    let mut i = (index.min(text.len()) + 1).min(text.len());
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Índice de línea (0-based) del byte `cursor`.
fn line_of(text: &str, cursor: usize) -> usize {
    text[..cursor.min(text.len())].matches('\n').count()
}

/// `(inicio, fin)` en bytes de la línea `line` (sin el `\n`).
fn line_span(text: &str, line: usize) -> (usize, usize) {
    let mut start = 0usize;
    let mut current = 0usize;
    for (i, ch) in text.char_indices() {
        if current == line {
            let end = text[i..].find('\n').map_or(text.len(), |rel| i + rel);
            return (start, end);
        }
        if ch == '\n' {
            current += 1;
            start = i + 1;
        }
    }
    (start, text.len())
}

/// Tema oscuro coherente por defecto (v0.3): paleta única para panel,
/// label, botón y entrada de texto — antes solo se estilizaba `button` y
/// el resto caía en el gris genérico de `WidgetStyle::default()`.
pub const DEFAULT_DARK_DSL: &str = r#"
widget panel {
    background: #16181d;
    border-color: #262a33;
    border-width: 1;
    border-radius: 10;
    padding: 12;
}
widget label {
    color: #d7dce4;
    font-size: 14;
}
widget button {
    background: #2d6cdf;
    color: #f4f7fb;
    border-radius: 6;
    padding: 8 14;
    font-size: 14;
    transition: 120ms ease;

    state hover  { background: lighten(background, 12%); }
    state active { background: darken(background, 15%); }
    state disabled { color: alpha(color, 45%); }
}
widget text-input {
    background: #0f1115;
    color: #d7dce4;
    border-color: #2a2f3a;
    border-radius: 6;
    padding: 8 10;
    font-size: 14;

    state hover { border-color: lighten(border-color, 20%); }
}
theme dark {
    base: #16181d;
    text: #d7dce4;
}
"#;

/// Tabla con `WidgetStyle::default()` en todas las celdas (sin DSL):
/// respaldo si el tema pedido no compila.
#[must_use]
pub fn default_style_table() -> StyleTable {
    rivet_style::compile("theme neutral {}", "neutral").expect("DSL mínimo siempre compila")
}
