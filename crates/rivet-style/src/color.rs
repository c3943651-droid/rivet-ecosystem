//! Own RGBA color for the style subsystem (pure Rust, no color crates).
//! Straight alpha in the representation; color derivation
//! (`lighten`/`darken`/`alpha`/`mix`) happens at theme **compilation**
//! (ADR-06), never per frame.

use std::fmt;

/// Color RGBA con componentes `u8` (alfa recta).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    /// Rojo.
    pub r: u8,
    /// Verde.
    pub g: u8,
    /// Azul.
    pub b: u8,
    /// Alfa (255 = opaco).
    pub a: u8,
}

impl Color {
    /// Negro opaco.
    pub const BLACK: Color = Color::rgba(0, 0, 0, 255);
    /// Blanco opaco.
    pub const WHITE: Color = Color::rgba(255, 255, 255, 255);
    /// Totalmente transparente.
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);

    /// Construye un color.
    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Color opaco.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Parsea `#RRGGBB` o `#RRGGBBAA` (con o sin `#`).
    ///
    /// # Errors
    ///
    /// [`StyleError::InvalidValue`] si el hex es inválido.
    pub fn parse_hex(hex: &str) -> Result<Self, crate::error::StyleError> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        let digit = |c: u8| -> Result<u8, crate::error::StyleError> {
            (c as char).to_digit(16).map(|d| d as u8).ok_or_else(|| {
                crate::error::StyleError::invalid_value(format!("hex inválido: {hex}"))
            })
        };
        let bytes = hex.as_bytes();
        match bytes.len() {
            // #RGB (shorthand CSS): cada dígito se duplica.
            3 => Ok(Self {
                r: digit(bytes[0])? * 17,
                g: digit(bytes[1])? * 17,
                b: digit(bytes[2])? * 17,
                a: 255,
            }),
            6 | 8 => {
                let pairs = bytes.len() / 2;
                let mut out = [0u8; 4];
                for i in 0..pairs {
                    let hi = digit(bytes[i * 2])?;
                    let lo = digit(bytes[i * 2 + 1])?;
                    out[i] = hi * 16 + lo;
                }
                if pairs == 3 {
                    out[3] = 255; // #RRGGBB: opaco.
                }
                Ok(Self {
                    r: out[0],
                    g: out[1],
                    b: out[2],
                    a: out[3],
                })
            }
            _ => Err(crate::error::StyleError::invalid_value(format!(
                "hex de {} dígitos (se esperaban 3, 6 u 8)",
                bytes.len()
            ))),
        }
    }

    /// ACLARA el color `percent`% hacia blanco (por canal, alfa intacto).
    #[must_use]
    pub fn lighten(self, percent: f32) -> Self {
        self.mix_channel_wise(Self::WHITE.with_alpha(self.a), percent / 100.0)
    }

    /// OSCURECE el color `percent`% hacia negro.
    #[must_use]
    pub fn darken(self, percent: f32) -> Self {
        self.mix_channel_wise(Self::BLACK.with_alpha(self.a), percent / 100.0)
    }

    /// Escala el alfa (0..1) manteniendo RGB.
    #[must_use]
    pub fn with_alpha_f(self, alpha: f32) -> Self {
        let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        Self { a, ..self }
    }

    /// Mezcla con otro color `t`∈[0,1] por canal lineal.
    #[must_use]
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Self {
            r: lerp(self.r, other.r),
            g: lerp(self.g, other.g),
            b: lerp(self.b, other.b),
            a: lerp(self.a, other.a),
        }
    }

    fn mix_channel_wise(self, other: Self, t: f32) -> Self {
        self.mix(other, t)
    }

    /// Copia con otro alfa.
    #[must_use]
    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "#{:02x}{:02x}{:02x}{:02x}",
            self.r, self.g, self.b, self.a
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_formatos() {
        assert_eq!(
            Color::parse_hex("#3a7bd5").unwrap(),
            Color::rgb(0x3a, 0x7b, 0xd5)
        );
        assert_eq!(Color::parse_hex("#000").unwrap(), Color::BLACK);
        assert_eq!(Color::parse_hex("#f80").unwrap(), Color::rgb(255, 136, 0));
        assert_eq!(
            Color::parse_hex("1e1e1e80").unwrap(),
            Color::rgba(0x1e, 0x1e, 0x1e, 0x80)
        );
        assert!(Color::parse_hex("#12").is_err());
        assert!(Color::parse_hex("zzzzzz").is_err());
    }

    #[test]
    fn derivaciones() {
        let base = Color::rgb(100, 100, 100);
        let lighter = base.lighten(50.0);
        assert_eq!(lighter.r, 178, "100 + (255-100)·0.5 redondeado");
        assert_eq!(lighter.a, 255, "el alfa no cambia");
        let darker = base.darken(50.0);
        assert_eq!(darker.r, 50);
        let faded = base.with_alpha_f(0.5);
        assert_eq!(faded.a, 128);
        let mixed = Color::BLACK.mix(Color::WHITE, 0.5);
        assert_eq!(mixed, Color::rgb(128, 128, 128));
    }
}
