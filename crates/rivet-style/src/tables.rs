//! Flat style tables: the compiler output and the render's O(1) lookup
//! path (doc §2/§5).
//!
//! Switching themes swaps the whole table (O(1)); derived values are
//! never recomputed at runtime (ADR-06).

use crate::color::Color;

/// Tipos de widget con fila dedicada en la tabla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum WidgetKind {
    /// Botón pulsable.
    Button,
    /// Etiqueta de texto estático.
    Label,
    /// Campo de entrada de texto.
    TextInput,
    /// Fondo/ventana.
    Window,
    /// Panel o contenedor.
    Panel,
    /// Casilla de verificación.
    Checkbox,
    /// Deslizador.
    Slider,
    /// Barra de desplazamiento.
    Scrollbar,
    /// Tooltip emergente.
    Tooltip,
    /// Icono (tinte por estado).
    Icon,
}

/// Número de tipos con fila dedicada (para indexar).
pub const WIDGET_KIND_COUNT: usize = 10;

/// Estados como **bitset** (doc §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WidgetState(u8);

impl WidgetState {
    /// Estado base, sin banderas.
    pub const NORMAL: WidgetState = WidgetState(0);
    /// Puntero encima.
    pub const HOVER: WidgetState = WidgetState(1 << 0);
    /// Presionado.
    pub const ACTIVE: WidgetState = WidgetState(1 << 1);
    /// Con foco de teclado.
    pub const FOCUSED: WidgetState = WidgetState(1 << 2);
    /// Deshabilitado.
    pub const DISABLED: WidgetState = WidgetState(1 << 3);

    /// Une banderas de estado.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        WidgetState(self.0 | other.0)
    }

    /// `true` si contiene todas las banderas de `other`.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Bits crudos.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// Filas por widget: una por estado resuelto, con prioridad documentada
/// (§6: Disabled > Active > Focused > Hover > Normal).
pub const STATE_ROW_COUNT: usize = 5;

/// Índice de fila a partir del bitset (resolución de prioridad).
#[must_use]
pub fn state_row(state: WidgetState) -> usize {
    if state.contains(WidgetState::DISABLED) {
        4
    } else if state.contains(WidgetState::ACTIVE) {
        3
    } else if state.contains(WidgetState::FOCUSED) {
        2
    } else if state.contains(WidgetState::HOVER) {
        1
    } else {
        0
    }
}

/// Márgenes internos del widget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Insets {
    /// Margen superior.
    pub top: f32,
    /// Margen derecho.
    pub right: f32,
    /// Margen inferior.
    pub bottom: f32,
    /// Margen izquierdo.
    pub left: f32,
}

impl Insets {
    /// Los cuatro márgenes iguales.
    #[must_use]
    pub const fn all(v: f32) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// Vertical y horizontal (sintaxis `padding: 8 12`).
    #[must_use]
    pub const fn vertical_horizontal(v: f32, h: f32) -> Self {
        Self {
            top: v,
            bottom: v,
            right: h,
            left: h,
        }
    }

    /// Los cuatro explícitos (t, r, b, l).
    #[must_use]
    pub const fn trbl(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Cero en los cuatro lados.
    #[must_use]
    pub const fn zero() -> Self {
        Self::all(0.0)
    }
}

/// Curva de interpolación de transiciones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ease {
    /// Lineal.
    #[default]
    Linear,
    /// Suavizada (cúbica, *ease-in-out*).
    Ease,
}

impl Ease {
    /// Aplica la curva a `t`∈[0,1].
    #[must_use]
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::Ease => t * t * (3.0 - 2.0 * t), // smoothstep
        }
    }
}

/// Transición animable del widget (interpola canvas, doc §6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    /// Duración en milisegundos.
    pub duration_ms: u32,
    /// Curva.
    pub ease: Ease,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            duration_ms: 0,
            ease: Ease::Linear,
        }
    }
}

/// Estilo plano y resuelto de un widget en un estado: solo datos, sin
/// referencias ni funciones (todo se precalculó al compilar).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WidgetStyle {
    /// Fondo del widget.
    pub background: Color,
    /// Texto/primer plano.
    pub foreground: Color,
    /// Color del borde.
    pub border_color: Color,
    /// Grosor del borde.
    pub border_width: f32,
    /// Radio de las esquinas.
    pub border_radius: f32,
    /// Márgenes internos.
    pub padding: Insets,
    /// Tamaño de fuente en píxeles.
    pub font_size: f32,
    /// Transición al cambiar de estado.
    pub transition: Transition,
}

impl Default for WidgetStyle {
    fn default() -> Self {
        Self {
            background: Color::rgba(40, 40, 44, 255),
            foreground: Color::rgba(224, 224, 224, 255),
            border_color: Color::rgba(70, 70, 76, 255),
            border_width: 1.0,
            border_radius: 4.0,
            padding: Insets::all(6.0),
            font_size: 14.0,
            transition: Transition::default(),
        }
    }
}

impl WidgetStyle {
    /// Interpolación lineal entre dos estados (para las transiciones).
    #[must_use]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Self {
            background: self.background.mix(other.background, t),
            foreground: self.foreground.mix(other.foreground, t),
            border_color: self.border_color.mix(other.border_color, t),
            border_width: l(self.border_width, other.border_width),
            border_radius: l(self.border_radius, other.border_radius),
            padding: Insets {
                top: l(self.padding.top, other.padding.top),
                right: l(self.padding.right, other.padding.right),
                bottom: l(self.padding.bottom, other.padding.bottom),
                left: l(self.padding.left, other.padding.left),
            },
            font_size: l(self.font_size, other.font_size),
            transition: self.transition,
        }
    }
}

/// Tabla plana: `WIDGET_KIND_COUNT` filas de `STATE_ROW_COUNT` estilos.
///
/// La consulta es un doble índice a arrays: **O(1)** en el camino caliente
/// del render (doc §12).
pub struct StyleTable {
    /// Nombre del tema con el que se compiló.
    theme: String,
    rows: Vec<[WidgetStyle; STATE_ROW_COUNT]>,
}

impl StyleTable {
    /// Consulta el estilo de un tipo en un estado (resuelve prioridad).
    #[must_use]
    pub fn get(&self, kind: WidgetKind, state: WidgetState) -> &WidgetStyle {
        let kind_index = (kind as u16 as usize).min(WIDGET_KIND_COUNT - 1);
        &self.rows[kind_index][state_row(state)]
    }

    /// Nombre del tema compilado.
    #[must_use]
    pub fn theme(&self) -> &str {
        &self.theme
    }
}

/// Constructor interno usado por el compilador.
pub(crate) fn build_table(theme: String, rows: Vec<[WidgetStyle; STATE_ROW_COUNT]>) -> StyleTable {
    debug_assert_eq!(rows.len(), WIDGET_KIND_COUNT);
    StyleTable { theme, rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prioridad_de_estados() {
        assert_eq!(state_row(WidgetState::NORMAL), 0);
        assert_eq!(state_row(WidgetState::HOVER), 1);
        assert_eq!(state_row(WidgetState::FOCUSED), 2);
        assert_eq!(state_row(WidgetState::ACTIVE), 3);
        assert_eq!(state_row(WidgetState::DISABLED), 4);
        // Disabled gana sobre el resto (doc §6).
        assert_eq!(
            state_row(
                WidgetState::HOVER
                    .union(WidgetState::ACTIVE)
                    .union(WidgetState::DISABLED)
            ),
            4
        );
        // Active gana sobre focused/hover.
        assert_eq!(
            state_row(WidgetState::FOCUSED.union(WidgetState::ACTIVE)),
            3
        );
    }

    #[test]
    fn ease_y_lerp() {
        assert!((Ease::Ease.apply(0.5) - 0.5).abs() < 1e-6);
        assert_eq!(Ease::Linear.apply(0.25), 0.25);
        let a = WidgetStyle::default();
        let mut b = a;
        b.background = Color::WHITE;
        let mid = a.lerp(&b, 0.5);
        assert_eq!(mid.background, a.background.mix(Color::WHITE, 0.5));
    }

    #[test]
    fn insets_shorthands() {
        assert_eq!(Insets::all(4.0), Insets::trbl(4.0, 4.0, 4.0, 4.0));
        assert_eq!(
            Insets::vertical_horizontal(8.0, 12.0),
            Insets::trbl(8.0, 12.0, 8.0, 12.0)
        );
    }
}
