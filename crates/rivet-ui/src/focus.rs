//! Keyboard focus management (doc §7): focused widget, `Tab`-order and
//! pointer focus.

use crate::error::Result;
use crate::registry::WidgetRegistry;
use crate::widget::{state, WidgetId, WidgetKind};

/// Administra qué widget recibe teclado.
#[derive(Debug, Default)]
pub struct FocusManager {
    focused: Option<WidgetId>,
}

impl FocusManager {
    /// Enfoca un widget interactivo (botones y entradas de texto).
    pub fn focus(&mut self, registry: &mut WidgetRegistry, id: WidgetId) -> Result<()> {
        if !registry.is_alive(id) {
            return Ok(()); // ignorar IDs muertos, no es error de uso
        }
        self.clear_state(registry);
        self.focused = Some(id);
        if let Ok(node) = registry.node_mut(id) {
            node.state |= state::FOCUSED;
            node.dirty = true;
        }
        Ok(())
    }

    /// Des-enfoca el widget actual.
    pub fn blur(&mut self, registry: &mut WidgetRegistry) {
        self.clear_state(registry);
        self.focused = None;
    }

    /// Widget enfocado (si sigue vivo).
    #[must_use]
    pub fn focused(&self, registry: &WidgetRegistry) -> Option<WidgetId> {
        self.focused.filter(|&id| registry.is_alive(id))
    }

    /// Avanza el foco con Tab: siguiente interactive en orden de árbol.
    pub fn tab_forward(&mut self, registry: &mut WidgetRegistry) -> Result<()> {
        let Some(root) = registry.root() else {
            return Ok(());
        };
        // Orden de árbol (pre-orden) de widgets interactivos.
        let order = tab_order(registry, root);
        if order.is_empty() {
            self.blur(registry);
            return Ok(());
        }
        let current = self.focused(registry);
        let next = match current {
            Some(id) => order
                .iter()
                .position(|&w| w == id)
                .map(|index| order[(index + 1) % order.len()])
                .unwrap_or(order[0]),
            None => order[0],
        };
        self.focus(registry, next)
    }

    fn clear_state(&mut self, registry: &mut WidgetRegistry) {
        if let Some(id) = self.focused {
            if let Ok(node) = registry.node_mut(id) {
                node.state &= !state::FOCUSED;
                node.dirty = true;
            }
        }
    }
}

/// Orden de tabulación: pre-orden del árbol, solo widgets interactivos.
fn tab_order(registry: &WidgetRegistry, root: WidgetId) -> Vec<WidgetId> {
    let mut order = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !registry.is_alive(id) {
            continue;
        }
        if let Ok(node) = registry.node(id) {
            if is_interactive(node.kind) && !node.has_state(state::DISABLED) {
                order.push(id);
            }
            // Apilar hijos invertidos para conservar pre-orden.
            let mut kids = registry.children(id).collect::<Vec<_>>();
            kids.reverse();
            stack.extend(kids);
        }
    }
    order
}

fn is_interactive(kind: WidgetKind) -> bool {
    matches!(kind, WidgetKind::Button | WidgetKind::TextInput)
}
