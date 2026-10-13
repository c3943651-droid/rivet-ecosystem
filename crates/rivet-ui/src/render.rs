//! Dibujo: recorre el árbol en orden (pintor) y emite comandos a la cola
//! de canvas consultando el estilo por (tipo, estado) — todo O(1) por
//! widget (doc §9).

use rivet_canvas::{Color as CanvasColor, Rect, RenderQueue};
use rivet_style::{Color, WidgetKind as StyleKind, WidgetState as StyleState};

use crate::error::Result;
use crate::registry::WidgetRegistry;
use crate::text::{DrawJob, DrawTarget, TextRenderer};
use crate::widget::{state, WidgetId, WidgetKind};
use crate::StyleTable;

/// Renderiza el árbol completo de `root` a la cola.
pub fn render_tree(
    registry: &mut WidgetRegistry,
    text: &mut TextRenderer,
    style: &StyleTable,
    canvas: &mut rivet_canvas::GraphicsContext,
    queue: &mut RenderQueue,
    root: WidgetId,
) -> Result<()> {
    if !registry.is_alive(root) {
        return Ok(());
    }
    render_node(registry, text, style, canvas, queue, root)
}

fn canvas_color(color: Color) -> CanvasColor {
    CanvasColor::rgba(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    )
}

fn render_node(
    registry: &mut WidgetRegistry,
    text: &mut TextRenderer,
    style: &StyleTable,
    canvas: &mut rivet_canvas::GraphicsContext,
    queue: &mut RenderQueue,
    id: WidgetId,
) -> Result<()> {
    let (node_state, kind, rect) = {
        let node = registry.node(id)?;
        (node.state, node.kind, node.rect)
    };
    if node_state & state::VISIBLE == 0 {
        return Ok(());
    }
    let style_state = style_state_of(node_state);
    let widget_style = style.get(style_kind_of(kind), style_state);

    // Fondo + borde del propio widget (contenedores decoran; hojas también).
    if kind != WidgetKind::Label {
        // Borde: rect redondeado exterior con color de borde…
        if widget_style.border_width > 0.0 && kind == WidgetKind::Panel {
            queue.rounded_rect(
                0,
                rect,
                widget_style.border_radius,
                canvas_color(widget_style.border_color),
            );
            // …y fondo interior encajado.
            let inset = widget_style.border_width;
            queue.rounded_rect(
                0,
                Rect::new(
                    rect.x + inset,
                    rect.y + inset,
                    rect.w - inset * 2.0,
                    rect.h - inset * 2.0,
                ),
                (widget_style.border_radius - inset).max(0.0),
                canvas_color(widget_style.background),
            );
        } else {
            queue.rounded_rect(
                0,
                rect,
                widget_style.border_radius,
                canvas_color(widget_style.background),
            );
        }
    }

    // Contenido por tipo.
    match kind {
        WidgetKind::Label => {
            let data = registry.label(id)?.clone();
            let x = rect.x;
            let y = rect.y;
            let fg = widget_style.foreground;
            let size = widget_style.font_size;
            let layout = text.layout(&data.text, size);
            text.draw(
                &mut DrawTarget { canvas, queue },
                DrawJob {
                    layout: &layout,
                    x,
                    y,
                    size,
                    color: fg,
                },
            );
        }
        WidgetKind::Button => {
            let data_text = registry.button(id)?.text.clone();
            let label = text.layout(&data_text, widget_style.font_size);
            let (tw, th) = (label.width, text.line_height(widget_style.font_size));
            let p = widget_style.padding;
            let x = rect.x + (rect.w - tw).max(0.0) * 0.5;
            let y = rect.y + p.top + (rect.h - p.top - p.bottom - th).max(0.0) * 0.5;
            let fg = widget_style.foreground;
            let size = widget_style.font_size;
            text.draw(
                &mut DrawTarget { canvas, queue },
                DrawJob {
                    layout: &label,
                    x,
                    y,
                    size,
                    color: fg,
                },
            );
        }
        WidgetKind::TextInput => {
            let shown = {
                let data = registry.text_input(id)?;
                if data.text.is_empty() {
                    data.placeholder.clone()
                } else {
                    data.text.clone()
                }
            };
            let p = widget_style.padding;
            let layout = text.layout(&shown, widget_style.font_size);
            let mut fg = widget_style.foreground;
            if shown == registry.text_input(id)?.placeholder
                && registry.text_input(id)?.text.is_empty()
            {
                fg = fg.with_alpha_f(0.5);
            }
            let size = widget_style.font_size;
            let y = rect.y + p.top;
            text.draw(
                &mut DrawTarget { canvas, queue },
                DrawJob {
                    layout: &layout,
                    x: rect.x + p.left,
                    y,
                    size,
                    color: fg,
                },
            );
            // Careta (visible con foco).
            if node_state & state::FOCUSED != 0 {
                let caret_x = rect.x + p.left + layout.width + 1.0;
                queue.rect(
                    0,
                    Rect::new(caret_x, y, 1.0, text.line_height(size)),
                    canvas_color(widget_style.foreground),
                );
            }
        }
        WidgetKind::Image => {
            let mut upload = None;
            {
                let data = registry.image(id)?;
                if let Some(pending) = &data.pending {
                    upload = Some((pending.width, pending.height, pending.pixels.clone()));
                }
            }
            if let Some((w, h, pixels)) = upload {
                if let Ok(texture) = canvas.create_texture(w, h, &pixels) {
                    if let Ok(data) = registry.image_mut(id) {
                        data.texture = Some(texture);
                        data.pending = None;
                    }
                }
            }
            let texture = registry.image(id)?.texture;
            if let Some(texture) = texture {
                let uv = Rect::new(0.0, 0.0, 1.0, 1.0);
                let transform = rivet_canvas::Transform::identity();
                queue.textured_rect(0, rect, texture, uv, canvas_color(Color::WHITE), &transform);
            }
        }
        WidgetKind::Box | WidgetKind::Panel => {}
    }

    // Hijos al final (delante en z); recolectados para soltar el borrow.
    let children: Vec<WidgetId> = registry.children(id).collect();
    for child in children {
        render_node(registry, text, style, canvas, queue, child)?;
    }
    Ok(())
}

fn style_state_of(bits: u8) -> StyleState {
    let mut state = StyleState::NORMAL;
    if bits & state::DISABLED != 0 {
        state = state.union(StyleState::DISABLED);
    }
    if bits & state::ACTIVE != 0 {
        state = state.union(StyleState::ACTIVE);
    }
    if bits & state::FOCUSED != 0 {
        state = state.union(StyleState::FOCUSED);
    }
    if bits & state::HOVER != 0 {
        state = state.union(StyleState::HOVER);
    }
    state
}

fn style_kind_of(kind: WidgetKind) -> StyleKind {
    match kind {
        WidgetKind::Box => StyleKind::Panel,
        WidgetKind::Panel => StyleKind::Panel,
        WidgetKind::Button => StyleKind::Button,
        WidgetKind::Label => StyleKind::Label,
        WidgetKind::Image => StyleKind::Icon,
        WidgetKind::TextInput => StyleKind::TextInput,
    }
}
