//! Rivet: fachada del ecosistema.
//!
//! Re-exporta `rivet-foundation`, `rivet-window`, `rivet-canvas`,
//! `rivet-typography`, `rivet-image`, `rivet-style` y `rivet-ui`.
//! Equivalente al "paraguas" de `#include <gtk/gtk.h>`.

pub use rivet_canvas as canvas;
pub use rivet_foundation as foundation;
pub use rivet_image as image;
pub use rivet_style as style;
pub use rivet_typography as typography;
pub use rivet_ui as ui;
pub use rivet_window as window;
