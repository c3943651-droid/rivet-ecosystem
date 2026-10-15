//! Vector primitives: circles and rounded rectangles via
//! *fan-decomposition* (own implementation, no tessellation crates).
//!
//! Each shape is emitted as a triangle fan (central vertex + perimeter)
//! into the active flat batch: O(n) CPU cost, a single draw call
//! per batch thanks to [`RenderQueue`] batching.

use crate::color::Color;
use crate::geometry::Rect;
use crate::queue::RenderQueue;

/// Segmentos mínimos por arco completo.
const MIN_ARC_SEGMENTS: usize = 8;

impl RenderQueue {
    /// Círculo relleno (aproximado por abanico de triángulos).
    pub fn circle(&mut self, layer: u32, cx: f32, cy: f32, radius: f32, color: Color) {
        self.mark_damage(Rect::new(
            cx - radius,
            cy - radius,
            radius * 2.0,
            radius * 2.0,
        ));
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
        self.mark_damage(Rect::new(
            cx - radius_x,
            cy - radius_y,
            radius_x * 2.0,
            radius_y * 2.0,
        ));
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
        self.mark_damage(rect);
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
        // Perímetro en sentido horario VISUAL (Y hacia abajo), empezando
        // en el borde superior tras la esquina superior-izquierda:
        //   top → arco TR → borde derecho → arco BR → arco BL → arco TL
        // (el arco TL va al FINAL: termina donde empieza el perímetro).
        // v0.3-fix: antes se emitía segundo y el punto de cierre era
        // (left+radius, bottom), lo que dejaba un hueco triangular en la
        // esquina inferior-izquierda y solapaba el abanico.
        push_point(batch, left + radius, top);
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
        // Arco superior-izquierda (180°→270°), centro (left+r, top+r):
        // termina en (left+radius, top) y cierra con el punto inicial.
        arc_points(
            batch,
            (left + radius, top + radius),
            radius,
            180.0,
            270.0,
            arc,
            &mut push_point,
        );
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

    /// Punto dentro de un triángulo (barycentric, con epsilon).
    fn in_triangle(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
        let sign = |p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)| {
            (p2.0 - p1.0) * (p3.1 - p1.1) - (p2.1 - p1.1) * (p3.0 - p1.0)
        };
        let d1 = sign(p, a, b);
        let d2 = sign(p, b, c);
        let d3 = sign(p, c, a);
        let neg = d1 < -0.01 && d2 < -0.01 && d3 < -0.01;
        let pos = d1 > 0.01 && d2 > 0.01 && d3 > 0.01;
        neg || pos
    }

    #[test]
    fn rounded_rect_perimetro_monotono_y_sin_huecos() {
        let rect = Rect::new(10.0, 20.0, 100.0, 40.0);
        let radius = 8.0f32;
        let mut q = RenderQueue::new();
        q.rounded_rect(0, rect, radius, Color::WHITE);
        let batch = &q.batches()[0];
        let verts: Vec<(f32, f32)> = batch.vertices.chunks(8).map(|c| (c[0], c[1])).collect();
        let (cx, cy) = verts[0]; // centro del abanico
        let perim = &verts[1..];

        // 1) El perímetro recorre el centro en UN sentido, sin picos:
        //    cada paso angular es pequeño y del mismo signo; total ≈ 2π.
        let mut prev = (perim[0].0 - cx).atan2(perim[0].1 - cy);
        let mut total = 0.0f32;
        let (mut positives, mut negatives) = (0usize, 0usize);
        for &(x, y) in &perim[1..] {
            let angle = (x - cx).atan2(y - cy);
            let mut delta = angle - prev;
            while delta > std::f32::consts::PI {
                delta -= std::f32::consts::TAU;
            }
            while delta < -std::f32::consts::PI {
                delta += std::f32::consts::TAU;
            }
            total += delta;
            if delta > 0.001 {
                positives += 1;
            }
            if delta < -0.001 {
                negatives += 1;
            }
            prev = angle;
        }
        assert!(
            (total.abs() - std::f32::consts::TAU).abs() < 0.05,
            "vuelta completa en un sentido (total={total})"
        );
        assert!(
            positives == 0 || negatives == 0,
            "sin inversiones de sentido: pasos +{positives}/-{negatives} (picos en el perímetro)"
        );

        // 2) Cobertura: todo punto interior de la forma está en algún
        //    triángulo del abanico (regresión del hueco en la esquina
        //    inferior-izquierda de v0.2).
        let tris: Vec<[(f32, f32); 3]> = batch
            .indices
            .chunks(3)
            .map(|i| {
                [
                    verts[i[0] as usize],
                    verts[i[1] as usize],
                    verts[i[2] as usize],
                ]
            })
            .collect();
        let inside_shape = |x: f32, y: f32| {
            if x < rect.x || y < rect.y || x > rect.right() || y > rect.bottom() {
                return false;
            }
            // Fuera de las esquinas: dentro del círculo de su centro.
            let corners = [
                (rect.x + radius, rect.y + radius),
                (rect.right() - radius, rect.y + radius),
                (rect.x + radius, rect.bottom() - radius),
                (rect.right() - radius, rect.bottom() - radius),
            ];
            let in_x_band = x >= rect.x + radius && x <= rect.right() - radius;
            let in_y_band = y >= rect.y + radius && y <= rect.bottom() - radius;
            in_x_band
                || in_y_band
                || corners.iter().any(|&(ccx, ccy)| {
                    let (dx, dy) = (x - ccx, y - ccy);
                    dx * dx + dy * dy <= radius * radius
                })
        };
        let mut holes = 0usize;
        let mut checked = 0usize;
        let mut y = rect.y + 0.5;
        while y < rect.bottom() {
            let mut x = rect.x + 0.5;
            while x < rect.right() {
                if inside_shape(x, y) {
                    checked += 1;
                    if !tris.iter().any(|t| in_triangle((x, y), t[0], t[1], t[2])) {
                        holes += 1;
                    }
                }
                x += 1.0;
            }
            y += 1.0;
        }
        assert!(checked > 2000, "muestreo suficiente ({checked})");
        assert_eq!(holes, 0, "el abanico cubre la forma completa");
    }
}
