//! Rivet Canvas: render 2D por GPU (OpenGL 3.3 Core + EGL).
//!
//! Shaders, geometría, texturas y efectos (blur por FBO ping-pong).
//! Escribe en el buffer que `rivet-window` presenta; **no** depende de
//! `rivet-window` (frontera fija: window = protocolo, canvas = render).
//!
//! Pipeline por frame: llenar [`RenderQueue`] → `GraphicsContext::render` →
//! `GraphicsContext::present`.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod blur;
pub mod color;
pub mod error;
pub mod geometry;
pub mod queue;
pub mod renderer;
pub mod shader;
pub mod stats;
pub mod texture;

pub use color::Color;
pub use error::{CanvasError, Result};
pub use geometry::{Rect, Transform};
pub use queue::{Batch, BlurCommand, RenderQueue, ShaderKind};
pub use renderer::GraphicsContext;
pub use stats::FrameStats;
pub use texture::{TextureId, TextureManager, TextureRegion, TextureSource};

/// Primitiva dibujable por un canvas (§4 de la doc).
pub trait Drawable {
    /// Añade los comandos de esta primitiva a la cola.
    fn draw(&self, layer: u32, queue: &mut RenderQueue);
}
