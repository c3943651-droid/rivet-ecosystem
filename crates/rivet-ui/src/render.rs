//! Drawing: traverses the tree in order (painter) and emits commands to the
//! canvas queue, looking up the style by (kind, state) — all O(1) per
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
    render_node(registry, text, style, canvas, queue, root, (0.0, 0.0))
}

fn canvas_color(color: Color) -> CanvasColor {
    // El blending del canvas es PREMULTIPLICADO (ONE, 1-SRC_ALPHA) y los
    // colores de estilo son straight: premultiplicar rgb×a aquí para que
    // los translúcidos (placeholder…) mezclen bien (v0.3-fix).
    let a = color.a as f32 / 255.0;
    CanvasColor::rgba(
        (color.r as f32 / 255.0) * a,
        (color.g as f32 / 255.0) * a,
        (color.b as f32 / 255.0) * a,
        a,
    )
}

fn render_node(
    registry: &mut WidgetRegistry,
    text: &mut TextRenderer,
    style: &StyleTable,
    canvas: &mut rivet_canvas::GraphicsContext,
    queue: &mut RenderQueue,
    id: WidgetId,
    offset: (f32, f32),
) -> Result<()> {
    let (node_state, kind, raw_rect) = {
        let node = registry.node(id)?;
        (node.state, node.kind, node.rect)
    };
    if node_state & state::VISIBLE == 0 {
        return Ok(());
    }
    // Scroll (v0.3): el desplazamiento acumulado de los ancestros se
    // resta a la posición (el clip del contenedor recorta el sobrante).
    let rect = Rect::new(
        raw_rect.x - offset.0,
        raw_rect.y - offset.1,
        raw_rect.w,
        raw_rect.h,
    );
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
            let (data_text, cursor, multiline) = {
                let data = registry.text_input(id)?;
                (
                    if data.text.is_empty() {
                        data.placeholder.clone()
                    } else {
                        data.text.clone()
                    },
                    data.cursor.min(data.text.len()),
                    data.multiline,
                )
            };
            let is_placeholder = registry.text_input(id)?.text.is_empty();
            let p = widget_style.padding;
            let mut fg = widget_style.foreground;
            if is_placeholder {
                fg = fg.with_alpha_f(0.5);
            }
            let size = widget_style.font_size;
            let line_h = text.line_height(size);
            let x0 = rect.x + p.left;
            let y0 = rect.y + p.top;
            // Multilínea (v0.3): las líneas se apilan; el cursor se
            // posiciona por (línea, columna). Sin wrap horizontal.
            if multiline {
                let mut line_y = y0;
                let mut caret = None;
                let mut consumed = 0usize;
                for line in data_text.split('\n') {
                    let layout = text.layout(line, size);
                    text.draw(
                        &mut DrawTarget { canvas, queue },
                        DrawJob {
                            layout: &layout,
                            x: x0,
                            y: line_y,
                            size,
                            color: fg,
                        },
                    );
                    // ¿El cursor está en esta línea?
                    if caret.is_none() && cursor <= consumed + line.len() {
                        let col = cursor.saturating_sub(consumed);
                        let col = col.min(line.len());
                        let caret_offset = if col == 0 {
                            0.0
                        } else {
                            line.get(..col)
                                .map_or(0.0, |prefix| text.layout(prefix, size).width)
                        };
                        caret = Some((x0 + caret_offset + 1.0, line_y));
                    }
                    consumed += line.len() + 1; // +1 del '\n'
                    line_y += line_h;
                }
                if let Some((caret_x, caret_y)) = caret {
                    if node_state & state::FOCUSED != 0 {
                        queue.rect(
                            0,
                            Rect::new(caret_x, caret_y, 1.0, line_h),
                            canvas_color(widget_style.foreground),
                        );
                    }
                }
            } else {
                let layout = text.layout(&data_text, size);
                text.draw(
                    &mut DrawTarget { canvas, queue },
                    DrawJob {
                        layout: &layout,
                        x: x0,
                        y: y0,
                        size,
                        color: fg,
                    },
                );
                // Careta en la posición del cursor (visible con foco).
                if node_state & state::FOCUSED != 0 && !is_placeholder {
                    let caret_offset = if cursor == 0 {
                        0.0
                    } else {
                        // `get` devuelve None si el corte cae fuera de una
                        // frontera de char: careta al inicio en ese caso.
                        data_text
                            .get(..cursor)
                            .map_or(0.0, |prefix| text.layout(prefix, size).width)
                    };
                    let caret_x = x0 + caret_offset + 1.0;
                    queue.rect(
                        0,
                        Rect::new(caret_x, y0, 1.0, line_h),
                        canvas_color(widget_style.foreground),
                    );
                }
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
    // Los contenedores recortan el desbordamiento a su rect (v0.2) y los
    // con scroll desplazan su subtree por `scroll_y` (v0.3).
    let children: Vec<WidgetId> = registry.children(id).collect();
    let scroll_y = match kind {
        WidgetKind::Box | WidgetKind::Panel => registry
            .container(id)
            .map(|c| if c.scroll { c.scroll_y } else { 0.0 })
            .unwrap_or(0.0),
        _ => 0.0,
    };
    let clips_children =
        matches!(kind, WidgetKind::Box | WidgetKind::Panel) && rect.w > 0.0 && rect.h > 0.0;
    if clips_children {
        queue.push_clip(rect);
    }
    let child_offset = (offset.0, offset.1 + scroll_y);
    for child in children {
        render_node(registry, text, style, canvas, queue, child, child_offset)?;
    }
    if clips_children {
        queue.pop_clip();
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
