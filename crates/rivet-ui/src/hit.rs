//! Geometric hit-testing: coordinate → widget (doc §7).
//!
//! Traverses **back to front** (last child first): the paint order is
//! the tree order, so later siblings are at the
//! front. Respects visibility and `DISABLED` (not interactive).

use crate::registry::WidgetRegistry;
use crate::widget::{state, WidgetId};

/// Widget más alto bajo el punto `(x, y)`, partiendo de `root`.
#[must_use]
pub fn hit_test(registry: &WidgetRegistry, root: WidgetId, x: f32, y: f32) -> Option<WidgetId> {
    if !registry.is_alive(root) {
        return None;
    }
    let node = registry.node(root).ok()?;
    if !node.has_state(state::VISIBLE) {
        return None;
    }
    // Primero los hijos (al frente); el propio nodo es el fondo.
    let mut child = node.last_child;
    while let Some(c) = child {
        if let Some(hit) = hit_test(registry, c, x, y) {
            return Some(hit);
        }
        child = registry.node(c).ok().and_then(|n| n.prev_sibling);
    }
    let rect = &node.rect;
    if x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom() {
        Some(root)
    } else {
        None
    }
}
