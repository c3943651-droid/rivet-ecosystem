//! Layout: two passes (measure → position) over the simplified box model
//! of v1 (doc §8).
//!
//! Measurement: labels query typography metrics; buttons add the
//! style padding; containers sum (or maximize) depending on direction.
//! Each widget is measured **once per frame**: `measure_cached` fills a
//! cache that `position` reads back (v0.2; before, `position` re-ran
//! `measure` recursively per child).

use std::collections::HashMap;

use rivet_canvas::Rect;

use crate::error::Result;
use crate::registry::WidgetRegistry;
use crate::text::TextRenderer;
use crate::widget::{WidgetId, WidgetKind};
use crate::StyleTable;

/// Tamaños preferidos medidos en la pasada de measure (v0.2).
pub type MeasureCache = HashMap<WidgetId, (f32, f32)>;

/// Ejecuta el layout completo del árbol de `root` en `width × height`.
pub fn layout_tree(
    registry: &mut WidgetRegistry,
    text: &mut TextRenderer,
    style: &StyleTable,
    root: WidgetId,
    width: f32,
    height: f32,
) -> Result<()> {
    let mut cache = MeasureCache::new();
    let (pref_w, pref_h) = measure_cached(registry, text, style, root, &mut cache)?;
    // v0.3: la raíz es el *viewport* de la ventana: ocupa exactamente el
    // espacio disponible (antes crecía al tamaño preferido, lo que
    // anulaba el scroll y desbordaba el buffer). El preferido medido se
    // conserva en el caché para los contenedores con scroll.
    let _ = (pref_w, pref_h);
    let w = width;
    let h = height;
    registry.node_mut(root)?.rect = Rect::new(0.0, 0.0, w, h);
    position(registry, style, root, Rect::new(0.0, 0.0, w, h), &cache)?;
    Ok(())
}

/// Medida del tamaño preferido (post-orden), con caché propia.
///
/// Cada widget se mide exactamente una vez por invocación; los
/// contenedores leen los tamaños de sus hijos de `cache`.
pub fn measure(
    registry: &WidgetRegistry,
    text: &mut TextRenderer,
    style: &StyleTable,
    id: WidgetId,
) -> Result<(f32, f32)> {
    let mut cache = MeasureCache::new();
    measure_cached(registry, text, style, id, &mut cache)
}

/// Medida que comparte caché con `layout_tree` (un medidor por frame).
fn measure_cached(
    registry: &WidgetRegistry,
    text: &mut TextRenderer,
    style: &StyleTable,
    id: WidgetId,
    cache: &mut MeasureCache,
) -> Result<(f32, f32)> {
    if let Some(cached) = cache.get(&id) {
        return Ok(*cached);
    }
    let (w, h) = measure_uncached(registry, text, style, id, cache)?;
    cache.insert(id, (w, h));
    Ok((w, h))
}

fn measure_uncached(
    registry: &WidgetRegistry,
    text: &mut TextRenderer,
    style: &StyleTable,
    id: WidgetId,
    cache: &mut MeasureCache,
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
            let line_h = text.line_height(widget_style.font_size);
            let p = widget_style.padding;
            if data.multiline {
                // Multilínea (v0.3): ancho de la línea más larga, alto por
                // número de líneas (sin wrap: v0.3 recorta el desborde).
                let lines = shown.split('\n').count();
                let mut width = 0.0f32;
                for line in shown.split('\n') {
                    width = width.max(text.layout(line, widget_style.font_size).width);
                }
                Ok((
                    width.max(40.0) + p.left + p.right,
                    lines as f32 * line_h + p.top + p.bottom,
                ))
            } else {
                Ok((
                    layout.width.max(120.0) + p.left + p.right,
                    line_h + p.top + p.bottom,
                ))
            }
        }
        WidgetKind::Image => {
            let pending = &registry.image(id)?.pending;
            Ok(match pending {
                Some(image) => (image.width as f32, image.height as f32),
                None => (0.0, 0.0),
            })
        }
        WidgetKind::Box | WidgetKind::Panel => {
            let container = registry.container(id)?.clone();
            let p = container.padding;
            let mut main = 0.0f32;
            let mut cross = 0.0f32;
            let children: Vec<WidgetId> = registry.children(id).collect();
            for child in &children {
                let (w, h) = measure_cached(registry, text, style, *child, cache)?;
                if container.vertical {
                    main += h;
                    cross = cross.max(w);
                } else {
                    main += w;
                    cross = cross.max(h);
                }
            }
            let gaps = container.spacing * children.len().saturating_sub(1) as f32;
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
/// hijos en orden, con padding, spacing y borde de `Panel` descontado
/// del área interna (v0.2).
///
/// Los hijos con `fill` (v0.2) se estiran: en el eje principal reparten el
/// espacio sobrante; en el cruzado toman el ancho/alto interno del
/// contenedor.
fn position(
    registry: &mut WidgetRegistry,
    style: &StyleTable,
    id: WidgetId,
    rect: Rect,
    cache: &MeasureCache,
) -> Result<()> {
    registry.node_mut(id)?.rect = rect;
    let kind = registry.node(id)?.kind;
    if !matches!(kind, WidgetKind::Box | WidgetKind::Panel) {
        return Ok(()); // hoja: rect asignado por el padre
    }
    let container = registry.container(id)?.clone();
    let p = container.padding;
    // #9: el borde del Panel también se descuenta del área de hijos.
    let border = if kind == WidgetKind::Panel {
        style
            .get(
                rivet_style::WidgetKind::Panel,
                rivet_style::WidgetState::NORMAL,
            )
            .border_width
    } else {
        0.0
    };
    let inner = Rect::new(
        rect.x + p.left + border,
        rect.y + p.top + border,
        (rect.w - p.left - p.right - 2.0 * border).max(0.0),
        (rect.h - p.top - p.bottom - 2.0 * border).max(0.0),
    );
    let children: Vec<WidgetId> = registry.children(id).collect();
    if children.is_empty() {
        if container.scroll {
            let c = registry.container_mut(id)?;
            c.content_h = 0.0;
            c.viewport_h = inner.h;
            c.scroll_y = 0.0;
        }
        return Ok(());
    }
    // Scroll (v0.3): los hijos se colocan sobre el área de CONTENIDO
    // (tamaño preferido del contenedor), no sobre el viewport; el render
    // desplaza el subtree por `scroll_y` y recorta al rect.
    let content_pref = cache.get(&id).copied().unwrap_or((inner.w, inner.h));
    let children_extent = (content_pref.1 - p.top - p.bottom).max(0.0);
    let layout_inner = if container.scroll {
        Rect::new(inner.x, inner.y, inner.w, children_extent.max(1.0))
    } else {
        inner
    };
    let fills: Vec<(bool, bool)> = children
        .iter()
        .map(|&child| {
            registry
                .node(child)
                .map(|node| (node.fill_x, node.fill_y))
                .unwrap_or((false, false))
        })
        .collect();
    let (main_avail, cross_avail) = if container.vertical {
        (layout_inner.h, layout_inner.w)
    } else {
        (layout_inner.w, layout_inner.h)
    };
    // Tamaños preferidos del caché + reparto del sobrante en el eje
    // principal entre los hijos con fill en ese eje.
    let gaps = container.spacing * children.len().saturating_sub(1) as f32;
    let used: f32 = children
        .iter()
        .map(|&child| {
            let (w, h) = cache.get(&child).copied().unwrap_or((1.0, 1.0));
            if container.vertical {
                h
            } else {
                w
            }
        })
        .sum::<f32>()
        + gaps;
    let main_fillers = fills
        .iter()
        .filter(
            |&&(fill_x, fill_y)| {
                if container.vertical {
                    fill_y
                } else {
                    fill_x
                }
            },
        )
        .count();
    let extra = if main_fillers > 0 {
        ((main_avail - used).max(0.0)) / main_fillers as f32
    } else {
        0.0
    };
    let mut offset = if container.vertical {
        layout_inner.y
    } else {
        layout_inner.x
    };
    for (index, &child) in children.iter().enumerate() {
        let (w, h) = cache.get(&child).copied().unwrap_or((1.0, 1.0));
        let (fill_x, fill_y) = fills[index];
        let child_rect = if container.vertical {
            let h = h + if fill_y { extra } else { 0.0 };
            let w = if fill_x { cross_avail } else { w };
            Rect::new(layout_inner.x, offset, w.max(1.0), h.max(1.0))
        } else {
            let w = w + if fill_x { extra } else { 0.0 };
            let h = if fill_y { cross_avail } else { h };
            Rect::new(offset, layout_inner.y, w.max(1.0), h.max(1.0))
        };
        // Un hijo con scroll es un *viewport*: nunca crece más allá del
        // espacio restante del padre (su contenido desborda dentro y se
        // recorta), v0.3.
        let child_scrollable = registry.container(child).map(|c| c.scroll).unwrap_or(false);
        let child_rect = if child_scrollable && !container.scroll {
            let remaining = if container.vertical {
                (inner.bottom() - child_rect.y).max(1.0)
            } else {
                (inner.right() - child_rect.x).max(1.0)
            };
            if container.vertical && child_rect.h > remaining {
                Rect::new(child_rect.x, child_rect.y, child_rect.w, remaining)
            } else if !container.vertical && child_rect.w > remaining {
                Rect::new(child_rect.x, child_rect.y, remaining, child_rect.h)
            } else {
                child_rect
            }
        } else {
            child_rect
        };
        if container.vertical {
            offset += child_rect.h + container.spacing;
        } else {
            offset += child_rect.w + container.spacing;
        }
        position(registry, style, child, child_rect, cache)?;
    }
    // Estado de scroll actualizado tras el layout: métricas + clamp.
    if container.scroll {
        let c = registry.container_mut(id)?;
        c.content_h = children_extent;
        c.viewport_h = inner.h;
        c.scroll_y = c.scroll_y.clamp(0.0, (c.content_h - c.viewport_h).max(0.0));
    }
    Ok(())
}
