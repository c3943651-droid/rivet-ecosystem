//! Primitivas vectoriales: círculos y rectángulos redondeados por
//! *fan-decomposition* (implementación propia, sin crates de teselación).
//!
//! Cada forma se emite como abanico de triángulos (vértice central +
//! perímetro) al lote plano activo: coste O(n) en CPU, un solo draw call
//! por lote gracias al batching de [`RenderQueue`].

use crate::color::Color;
use crate::geometry::Rect;
use crate::queue::RenderQueue;

/// Segmentos mínimos por arco completo.
const MIN_ARC_SEGMENTS: usize = 8;

impl RenderQueue {
    /// Círculo relleno (aproximado por abanico de triángulos).
    pub fn circle(&mut self, layer: u32, cx: f32, cy: f32, radius: f32, color: Color) {
        self.ellipse(layer, cx, cy, radius, radius, color);
    }

    /// Elipse rellena (semiejes independientes).
    pub fn ellipse(
        &mut self,
        layer: u32,
        cx: f32,
        cy: f32,
        radius_x: f32,
        radius_y: f32,
        color: Color,
    ) {
        assert!(radius_x > 0.0 && radius_y > 0.0, "ellipse: radio > 0");
        // Resolución proporcional al perímetro (~8 segmentos por radio).
        let segments =
            ((2.0 * radius_x.max(radius_y)).ceil() as usize).clamp(MIN_ARC_SEGMENTS, 256);
        let [r, g, b, a] = [color.r, color.g, color.b, color.a];
        let batch = self.batch_for(layer, crate::queue::ShaderKind::Flat, None);
        let base = (batch.vertices.len() / 8) as u32;
        // Centro del abanico.
        batch
            .vertices
            .extend_from_slice(&[cx, cy, 0.0, 0.0, r, g, b, a]);
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let (sin, cos) = (t * std::f32::consts::TAU).sin_cos();
            batch.vertices.extend_from_slice(&[
                cx + cos * radius_x,
                cy + sin * radius_y,
                0.0,
                0.0,
                r,
                g,
                b,
                a,
            ]);
        }
        for i in 0..segments as u32 {
            batch
                .indices
                .extend_from_slice(&[base, base + 1 + i, base + 2 + i]);
        }
        self.count_vertices(segments);
    }

    /// Rectángulo con esquinas redondeadas.
    ///
    /// `radius` se limita a `min(w, h) / 2`. Con `radius` igual a ese
    /// máximo equivale a una elipse inscrita.
    pub fn rounded_rect(&mut self, layer: u32, rect: Rect, radius: f32, color: Color) {
        let radius = radius.clamp(0.0, rect.w.min(rect.h) * 0.5);
        if radius <= f32::EPSILON {
            self.rect(layer, rect, color);
            return;
        }
        // Perímetro parametrizado: esquinas con arcos, bordes rectos.
        let (left, top) = (rect.x, rect.y);
        let (right, bottom) = (rect.right(), rect.bottom());
        // Segmentos por esquina según el radio (mínimo 4).
        let arc = (radius * 0.75).ceil() as usize * 4;
        let arc = arc.clamp(4, 64);
        let [r, g, b, a] = [color.r, color.g, color.b, color.a];
        let batch = self.batch_for(layer, crate::queue::ShaderKind::Flat, None);
        let base = (batch.vertices.len() / 8) as u32;
        let (cx, cy) = rect.center();

        let mut push_point = |batch: &mut crate::queue::Batch, x: f32, y: f32| {
            batch
                .vertices
                .extend_from_slice(&[x, y, 0.0, 0.0, r, g, b, a]);
        };

        push_point(batch, cx, cy);
        // Punto de partida: borde superior tras la esquina superior izquierda.
        push_point(batch, left + radius, top);
        // Arco superior-izquierda (de 180° a 270°), centro (left+r, top+r).
        arc_points(
            batch,
            (left + radius, top + radius),
            radius,
            180.0,
            270.0,
            arc,
            &mut push_point,
        );
        // Arco superior-derecha (270°→360°), centro (right-r, top+r).
        arc_points(
            batch,
            (right - radius, top + radius),
            radius,
            270.0,
            360.0,
            arc,
            &mut push_point,
        );
        push_point(batch, right, bottom - radius);
        // Arco inferior-derecha (0°→90°), centro (right-r, bottom-r).
        arc_points(
            batch,
            (right - radius, bottom - radius),
            radius,
            0.0,
            90.0,
            arc,
            &mut push_point,
        );
        // Arco inferior-izquierda (90°→180°), centro (left+r, bottom-r).
        arc_points(
            batch,
            (left + radius, bottom - radius),
            radius,
            90.0,
            180.0,
            arc,
            &mut push_point,
        );
        push_point(batch, left + radius, bottom);
        // Cierre implícito al primer punto del perímetro.

        // Abanico: centro → perímetro[i] → perímetro[i+1].
        let perimeter = (batch.vertices.len() / 8) as u32 - 1 - base;
        for i in 0..perimeter {
            let a0 = base + 1 + i;
            let a1 = base + 1 + (i + 1) % perimeter;
            batch.indices.extend_from_slice(&[base, a0, a1]);
        }
        self.count_vertices(perimeter as usize);
    }
}

/// Añade los puntos de un arco (ángulos en grados, Y hacia abajo: los
/// ángulos crecen en sentido horario visual).
fn arc_points(
    batch: &mut crate::queue::Batch,
    center: (f32, f32),
    radius: f32,
    from_deg: f32,
    to_deg: f32,
    segments: usize,
    push: &mut impl FnMut(&mut crate::queue::Batch, f32, f32),
) {
    for i in 1..=segments {
        let t = i as f32 / segments as f32;
        let deg = from_deg + (to_deg - from_deg) * t;
        let rad = deg * std::f32::consts::PI / 180.0;
        // Y hacia abajo: sin positivo = abajo en pantalla, coherente con la
        // proyección ortográfica del renderer.
        let (sin, cos) = rad.sin_cos();
        push(batch, center.0 + cos * radius, center.1 + sin * radius);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circulo_genera_abanico_cerrado() {
        let mut q = RenderQueue::new();
        q.circle(0, 10.0, 10.0, 5.0, Color::WHITE);
        assert_eq!(q.batches().len(), 1);
        let batch = &q.batches()[0];
        // 1 centro + (segments + 1) perímetro (cierre incluido).
        let segments = (2.0 * 5.0f32).ceil() as usize;
        let verts = batch.vertices.len() / 8;
        assert_eq!(verts, segments + 2);
        assert_eq!(batch.indices.len() / 3, segments);
        // El abanico cierra: todos los índices dentro del rango.
        assert!(batch.indices.iter().all(|&i| i < verts as u32));
    }

    #[test]
    fn rounded_rect_con_radio_cero_es_rect() {
        let mut q = RenderQueue::new();
        q.rounded_rect(0, Rect::new(0.0, 0.0, 10.0, 10.0), 0.0, Color::WHITE);
        assert_eq!(q.vertex_count(), 6, "rect normal: un quad");
    }

    #[test]
    fn rounded_rect_limite_radio() {
        let mut q = RenderQueue::new();
        // Radio mayor que la mitad: se limita, no paniquea.
        q.rounded_rect(0, Rect::new(0.0, 0.0, 20.0, 10.0), 99.0, Color::WHITE);
        assert!(
            q.vertex_count() > 6,
            "con esquinas redondeadas es más que un quad"
        );
    }
}
