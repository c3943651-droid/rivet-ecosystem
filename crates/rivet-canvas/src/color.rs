//! Premultiplied RGBA color.
//!
//! `docs/rivet-canvas.md` §4: pipeline colors are **premultiplied**
//! (rgb already scaled by alpha); GL blending uses `ONE, ONE_MINUS_SRC_ALPHA`.

use std::fmt;

/// Color RGBA con componentes en `0.0..=1.0`, **premultiplicado por alfa**.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// Rojo premultiplicado.
    pub r: f32,
    /// Verde premultiplicado.
    pub g: f32,
    /// Azul premultiplicado.
    pub b: f32,
    /// Alfa.
    pub a: f32,
}

impl Color {
    /// Color blanco opaco.
    pub const WHITE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
    /// Totalmente transparente.
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);
    /// Negro opaco.
    pub const BLACK: Color = Color::rgba(0.0, 0.0, 0.0, 1.0);

    /// Construye un color premultiplicado. No hace conversión.
    #[must_use]
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Construye un color desde componentes **rectos** (no premultiplicados),
    /// multiplicando rgb por alfa.
    #[must_use]
    pub fn straight(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: r * a,
            g: g * a,
            b: b * a,
            a,
        }
    }

    /// Color opaco (alfa = 1).
    #[must_use]
    pub fn opaque(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    /// Copia con otro alfa (re-premultiplica los canales).
    #[must_use]
    pub fn with_alpha(self, a: f32) -> Self {
        if self.a <= f32::EPSILON {
            Self::TRANSPARENT
        } else {
            let k = a / self.a;
            Self::rgba(self.r * k, self.g * k, self.b * k, a)
        }
    }

    /// Interpolación lineal premultiplicada.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::rgba(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
            self.a + (other.a - self.a) * t,
        )
    }

    /// Bytes RGBA8 premultiplicados (para subir texturas).
    #[must_use]
    pub fn to_rgba8(self) -> [u8; 4] {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [c(self.r), c(self.g), c(self.b), c(self.a)]
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rgba({:.3}, {:.3}, {:.3}, {:.3})",
            self.r, self.g, self.b, self.a
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_premultiplica() {
        let c = Color::straight(1.0, 0.5, 0.25, 0.5);
        assert_eq!(c, Color::rgba(0.5, 0.25, 0.125, 0.5));
    }

    #[test]
    fn with_alpha_repremultiplica() {
        let c = Color::straight(1.0, 1.0, 1.0, 0.5);
        let half = c.with_alpha(0.25);
        assert!((half.r - 0.25).abs() < 1e-6);
        assert!((half.a - 0.25).abs() < 1e-6);
    }

    #[test]
    fn lerp_y_clamp() {
        let mid = Color::BLACK.lerp(Color::WHITE, 0.5);
        assert_eq!(mid, Color::rgba(0.5, 0.5, 0.5, 1.0));
        assert_eq!(Color::WHITE.to_rgba8(), [255, 255, 255, 255]);
    }
}
