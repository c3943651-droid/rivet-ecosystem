//! Layout: dos pasadas (medir → posicionar) sobre el modelo de caja
//! simplificado de v1 (doc §8).
//!
//! Medición: labels consultan métricas de typography; botones añaden el
//! padding del estilo; contenedores suman (o maximizan) según dirección.

use rivet_canvas::Rect;

use crate::error::Result;
use crate::registry::WidgetRegistry;
use crate::text::TextRenderer;
use crate::widget::{WidgetId, WidgetKind};
use crate::StyleTable;

/// Ejecuta el layout completo del árbol de `root` en `width × height`.
pub fn layout_tree(
    registry: &mut WidgetRegistry,
    text: &mut TextRenderer,
    style: &StyleTable,
    root: WidgetId,
    width: f32,
    height: f32,
) -> Result<()> {
    let (pref_w, pref_h) = measure(registry, text, style, root)?;
    // v1: la raíz ocupa todo el espacio disponible (v1.x: alineaciones).
    let w = width.max(pref_w);
    let h = height.max(pref_h);
    registry.node_mut(root)?.rect = Rect::new(0.0, 0.0, w, h);
    position(registry, text, style, root, Rect::new(0.0, 0.0, w, h))?;
    Ok(())
}

/// Medida del tamaño preferido (post-orden).
pub fn measure(
    registry: &WidgetRegistry,
    text: &mut TextRenderer,
    style: &StyleTable,
    id: WidgetId,
) -> Result<(f32, f32)> {
    let node = registry.node(id)?;
    match node.kind {
        WidgetKind::Label => {
            let data = registry.label(id)?;
            let font_size = style
                .get(
                    rivet_style::WidgetKind::Label,
                    rivet_style::WidgetState::NORMAL,
                )
                .font_size;
            let layout = text.layout(&data.text, font_size);
            Ok((layout.width, text.line_height(font_size)))
        }
        WidgetKind::Button => {
            let data = registry.button(id)?;
            let widget_style = style.get(
                rivet_style::WidgetKind::Button,
                rivet_style::WidgetState::NORMAL,
            );
            let layout = text.layout(&data.text, widget_style.font_size);
            let p = widget_style.padding;
            Ok((
                layout.width + p.left + p.right + 8.0,
                text.line_height(widget_style.font_size) + p.top + p.bottom + 4.0,
            ))
        }
        WidgetKind::TextInput => {
            let data = registry.text_input(id)?;
            let widget_style = style.get(
                rivet_style::WidgetKind::TextInput,
                rivet_style::WidgetState::NORMAL,
            );
            let shown = if data.text.is_empty() {
                &data.placeholder
            } else {
                &data.text
            };
            let layout = text.layout(shown, widget_style.font_size);
            let p = widget_style.padding;
            Ok((
                layout.width.max(120.0) + p.left + p.right,
                text.line_height(widget_style.font_size) + p.top + p.bottom,
            ))
        }
        WidgetKind::Image => {
            let pending = &registry.image(id)?.pending;
            Ok(match pending {
                Some(image) => (image.width as f32, image.height as f32),
                None => (0.0, 0.0),
            })
        }
        WidgetKind::Box | WidgetKind::Panel => {
            let container = registry.container(id)?;
            let p = container.padding;
            let mut main = 0.0f32;
            let mut cross = 0.0f32;
            for child in registry.children(id) {
                let (w, h) = measure(registry, text, style, child)?;
                if container.vertical {
                    main += h;
                    cross = cross.max(w);
                } else {
                    main += w;
                    cross = cross.max(h);
                }
            }
            let n = registry.children(id).count();
            let gaps = container.spacing * n.saturating_sub(1) as f32;
            let (w, h) = if container.vertical {
                (cross, main + gaps)
            } else {
                (main + gaps, cross)
            };
            Ok((w + p.left + p.right, h + p.top + p.bottom))
        }
    }
}

/// Asignación de rects (pre-orden): reparte el rect del contenedor entre
/// hijos en orden, con padding y spacing; v1 sin expand/weight.
fn position(
    registry: &mut WidgetRegistry,
    text: &mut TextRenderer,
    style: &StyleTable,
    id: WidgetId,
    rect: Rect,
) -> Result<()> {
    registry.node_mut(id)?.rect = rect;
    let kind = registry.node(id)?.kind;
    if !matches!(kind, WidgetKind::Box | WidgetKind::Panel) {
        return Ok(()); // hoja: rect asignado por el padre
    }
    let container = registry.container(id)?.clone();
    let p = container.padding;
    let children: Vec<WidgetId> = registry.children(id).collect();
    if children.is_empty() {
        return Ok(());
    }
    // Tamaños preferidos de los hijos (la medición ya ocurrió bajo ellos).
    let mut sizes = Vec::with_capacity(children.len());
    for &child in &children {
        sizes.push(measure(registry, text, style, child)?);
    }
    // v1: cada hijo toma su tamaño preferido; el sobrante queda al final.
    let mut offset = if container.vertical {
        rect.y + p.top
    } else {
        rect.x + p.left
    };
    for (index, &child) in children.iter().enumerate() {
        let (w, h) = sizes[index];
        let child_rect = if container.vertical {
            let r = Rect::new(rect.x + p.left, offset, w.max(1.0), h.max(1.0));
            offset += h + container.spacing;
            r
        } else {
            let r = Rect::new(offset, rect.y + p.top, w.max(1.0), h.max(1.0));
            offset += w + container.spacing;
            r
        };
        position(registry, text, style, child, child_rect)?;
    }
    let _ = text;
    Ok(())
}
