//! 2D geometry: rectangles, affine transforms, and quad tessellation.
//!
//! Coordinates in pixels with **Y pointing down** (top-left origin);
//! the renderer projection flips Y for GL.

use crate::color::Color;

/// Rectángulo en píxeles, Y hacia abajo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Esquina superior izquierda, X.
    pub x: f32,
    /// Esquina superior izquierda, Y.
    pub y: f32,
    /// Ancho.
    pub w: f32,
    /// Alto.
    pub h: f32,
}

impl Rect {
    /// Rectángulo desde posición y tamaño.
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Borde derecho.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    /// Borde inferior.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Punto central.
    #[must_use]
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    /// Intersección con otro rectángulo (vacío si no se solapan).
    #[must_use]
    pub fn intersect(&self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        Self::new(x, y, (r - x).max(0.0), (b - y).max(0.0))
    }

    /// `true` si el área es cero o negativa.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// `true` si contiene el punto.
    #[must_use]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
}

/// Transformación afín 2D: `[a, b, c, d, tx, ty]`.
///
/// Se aplica como `x' = a·x + c·y + tx`, `y' = b·x + d·y + ty`
/// (la misma convención que Cairo/SVG).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Escala/rotación X.
    pub a: f32,
    /// Rotación/sesgo Y por X.
    pub b: f32,
    /// Sesgo X por Y.
    pub c: f32,
    /// Escala/rotación Y.
    pub d: f32,
    /// Traslación X.
    pub tx: f32,
    /// Traslación Y.
    pub ty: f32,
}

impl Transform {
    /// Transformación identidad.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Traslación.
    #[must_use]
    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            tx,
            ty,
            ..Self::identity()
        }
    }

    /// Escala uniforme (y no uniforme con sy).
    #[must_use]
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::identity()
        }
    }

    /// Rotación en radianes.
    #[must_use]
    pub fn rotate(rad: f32) -> Self {
        let (s, c) = rad.sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Aplica la transformación a un punto.
    #[must_use]
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }

    /// Composición: `self` aplicada **después** de `other`.
    ///
    /// `self.then(other)` = primero `self`, luego `other`.
    #[must_use]
    pub fn then(&self, other: Self) -> Self {
        Self {
            a: other.a * self.a + other.c * self.b,
            b: other.b * self.a + other.d * self.b,
            c: other.a * self.c + other.c * self.d,
            d: other.b * self.c + other.d * self.d,
            tx: other.a * self.tx + other.c * self.ty + other.tx,
            ty: other.b * self.tx + other.d * self.ty + other.ty,
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}

/// Añade un quad (2 triángulos, 4 vértices) a los buffers intercalados.
///
/// Formato de vértice (intercalado): `pos.xy, uv.xy, color.rgba` (8 × f32).
/// Los índices se generan con el offset del lote actual.
pub(crate) fn push_quad(
    vertices: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    rect: Rect,
    uv: Rect,
    color: Color,
    transform: &Transform,
) {
    let base = (vertices.len() / 8) as u32;
    let (x0, y0) = transform.apply(rect.x, rect.y);
    let (x1, y1) = transform.apply(rect.right(), rect.y);
    let (x2, y2) = transform.apply(rect.x, rect.bottom());
    let (x3, y3) = transform.apply(rect.right(), rect.bottom());
    let u0 = uv.x;
    let u1 = uv.right();
    let v0 = uv.y;
    let v1 = uv.bottom();
    let [r, g, b, a] = [color.r, color.g, color.b, color.a];
    vertices.extend_from_slice(&[
        x0, y0, u0, v0, r, g, b, a, x1, y1, u1, v0, r, g, b, a, x2, y2, u0, v1, r, g, b, a, x3, y3,
        u1, v1, r, g, b, a,
    ]);
    indices.extend_from_slice(&[base, base + 2, base + 1, base + 1, base + 2, base + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_geometria() {
        let r = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(r.right(), 40.0);
        assert_eq!(r.bottom(), 60.0);
        assert_eq!(r.center(), (25.0, 40.0));
        assert!(r.contains(25.0, 40.0));
        assert!(!r.contains(5.0, 40.0));
    }

    #[test]
    fn interseccion() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.intersect(b), Rect::new(5.0, 5.0, 5.0, 5.0));
        let c = Rect::new(20.0, 20.0, 5.0, 5.0);
        assert!(a.intersect(c).is_empty());
    }

    #[test]
    fn transform_translate_rotate() {
        let t = Transform::translate(10.0, 20.0);
        assert_eq!(t.apply(1.0, 2.0), (11.0, 22.0));

        let r90 = Transform::rotate(std::f32::consts::FRAC_PI_2);
        let (x, y) = r90.apply(1.0, 0.0);
        assert!((x - 0.0).abs() < 1e-6 && (y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn composicion_then() {
        // Escalar 2x y luego trasladar 10: (1,1) → (2,2) → (12,12).
        let t = Transform::scale(2.0, 2.0).then(Transform::translate(10.0, 10.0));
        assert_eq!(t.apply(1.0, 1.0), (12.0, 12.0));
    }

    #[test]
    fn quad_genera_4_vertices_y_6_indices() {
        let mut v = Vec::new();
        let mut i = Vec::new();
        push_quad(
            &mut v,
            &mut i,
            Rect::new(0.0, 0.0, 8.0, 8.0),
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::WHITE,
            &Transform::identity(),
        );
        push_quad(
            &mut v,
            &mut i,
            Rect::new(1.0, 1.0, 2.0, 2.0),
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::WHITE,
            &Transform::identity(),
        );
        assert_eq!(v.len(), 8 * 8);
        assert_eq!(i, vec![0, 2, 1, 1, 2, 3, 4, 6, 5, 5, 6, 7]);
    }
}
