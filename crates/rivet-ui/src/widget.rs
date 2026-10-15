//! Widget tree nodes: the common `WidgetNode` (ADR-03) and `WidgetId`.
//!
//! The node is small and homogeneous (relationships, state, geometry);
//! kind-specific data lives in the per-kind slabs of the
//! [`WidgetRegistry`](crate::registry::WidgetRegistry).

use rivet_canvas::Rect;

/// Identificador de widget: 32 bits índice + 32 bits generación
/// (coherente con `rivet-base`, doc §4).
pub type WidgetId = u64;

/// Bits de estado de un widget (doc §6).
pub mod state {
    /// Visible y participante en hit-test.
    pub const VISIBLE: u8 = 1 << 0;
    /// El puntero está encima.
    pub const HOVER: u8 = 1 << 1;
    /// Presionado.
    pub const ACTIVE: u8 = 1 << 2;
    /// Con foco de teclado.
    pub const FOCUSED: u8 = 1 << 3;
    /// Deshabilitado (no recibe eventos).
    pub const DISABLED: u8 = 1 << 4;
}

/// Tipos de widget v1 (doc §11). El catálogo crece en v1.x.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    /// Contenedor `Box` (horizontal o vertical; la dirección vive en su slab).
    Box,
    /// Botón pulsable con callback `on_click`.
    Button,
    /// Etiqueta de texto.
    Label,
    /// Imagen (textura de canvas).
    Image,
    /// Campo de entrada de texto (una línea).
    TextInput,
    /// Panel/superficie: contenedor con borde (base del panel/lanzador).
    Panel,
}

/// Nodo común: lo que todos los widgets comparten.
///
/// Hijos por **enlaces intrusivos** (ADR-04): sin `Vec` por nodo; alta/baja
/// de hijos O(1); el orden de hijos es el orden de inserción.
#[derive(Debug, Clone)]
pub struct WidgetNode {
    /// Padre (`None` en la raíz).
    pub parent: Option<WidgetId>,
    /// Primer hijo.
    pub first_child: Option<WidgetId>,
    /// Último hijo.
    pub last_child: Option<WidgetId>,
    /// Hermano siguiente.
    pub next_sibling: Option<WidgetId>,
    /// Hermano anterior.
    pub prev_sibling: Option<WidgetId>,
    /// Tipo del widget.
    pub kind: WidgetKind,
    /// Bits de estado ([`state`]).
    pub state: u8,
    /// Geometría resuelta por layout.
    pub rect: Rect,
    /// Estirar horizontalmente en el padre (v0.2, fill).
    pub fill_x: bool,
    /// Estirar verticalmente en el padre (v0.2, fill).
    pub fill_y: bool,
    /// Solicita redibujado (invalidación).
    pub dirty: bool,
}

impl WidgetNode {
    /// `true` si el bit de estado está activo.
    #[must_use]
    pub fn has_state(&self, bit: u8) -> bool {
        self.state & bit != 0
    }
}
