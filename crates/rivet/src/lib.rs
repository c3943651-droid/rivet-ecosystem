//! Rivet: fachada del ecosistema.
//!
//! Re-exporta `rivet-foundation`, `rivet-window`, `rivet-canvas`,
//! `rivet-typography`, `rivet-image`, `rivet-style` y `rivet-ui`.
//! Equivalente al "paraguas" de `#include <gtk/gtk.h>`.
//!
//! Para apps completas usa el [`prelude`]; las crates siguen
//! accesibles por separado (p. ej. solo el slab de `foundation` o el
//! decodificador de `image` sin arrastrar la UI).
//!
//! ```
//! use rivet::prelude::*;
//!
//! // La UI se compone por registro (sin herencia):
//! let mut app = Application::new("Demo", 400, 300).unwrap();
//! let ui = app.ui_mut();
//! let root = ui.registry.create_panel(true);
//! ui.registry.set_root(root).unwrap();
//! let label = ui.registry.create_label("Hola Rivet");
//! ui.registry.add_child(root, label).unwrap();
//! ```
//!
//! El bucle completo: `app.run()` — eventos de Wayland → UI → render GL
//! → buffer `wl_shm` presentado al compositor (v1).

pub use rivet_canvas as canvas;
pub use rivet_foundation as foundation;
pub use rivet_image as image;
pub use rivet_style as style;
pub use rivet_typography as typography;
pub use rivet_ui as ui;
pub use rivet_window as window;

/// API pública habitual para apps: `use rivet::prelude::*;`.
pub mod prelude {
    // Ciclo de app (ui).
    pub use rivet_ui::{Application, Ui};

    // Widgets y árbol.
    pub use rivet_ui::registry::WidgetRegistry;
    pub use rivet_ui::widget::{state, WidgetId, WidgetKind, WidgetNode};

    // Eventos y entrada.
    pub use rivet_ui::event::{EventDispatcher, UiInput};
    pub use rivet_ui::focus::FocusManager;
    pub use rivet_window::WindowEvent;

    // Dibujo.
    pub use rivet_canvas::{Color, Rect, RenderQueue, TextureId};
    pub use rivet_ui::TextRenderer;

    // Estilos.
    pub use rivet_style::{compile as compile_style, StyleTable, WidgetState, EXAMPLE_DSL};

    // Núcleo (creación de apps avanzadas con bucle propio).
    pub use rivet_foundation::context::{Context, ContextBuilder};
    pub use rivet_foundation::event_loop::EventLoop;

    // Imágenes e iconos.
    pub use rivet_image::{IconTheme, ImageData};

    // Tipografía.
    pub use rivet_typography::FontDb;
}

/// Versión del ecosistema (workspace unificado).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn fachada_reexporta_todo() {
        // Un toque de cada crate para validar que el paraguas compila.
        let _color: crate::canvas::Color = crate::canvas::Color::WHITE;
        let _queue: crate::canvas::RenderQueue = crate::canvas::RenderQueue::new();
        let _slab: crate::foundation::Slab<u8> = crate::foundation::Slab::new();
        let _style: crate::style::Color = crate::style::Color::BLACK;
        let _image: crate::image::ImageData = crate::image::missing_icon(8, 1);
        let _db: crate::typography::FontDb = crate::typography::FontDb::new();
        let _registry: crate::ui::registry::WidgetRegistry = crate::ui::WidgetRegistry::new();
        let _event: crate::window::WindowEvent = crate::window::WindowEvent::Close { window: 0 };
        assert_eq!(crate::VERSION, env!("CARGO_PKG_VERSION"));
    }
}
