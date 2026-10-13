//! Errores de `rivet-canvas`.

use std::fmt;

/// Resultado de las operaciones falibles de canvas.
pub type Result<T> = std::result::Result<T, CanvasError>;

/// Error del subsistema de render.
#[derive(Debug)]
pub enum CanvasError {
    /// Fallo al crear un recurso de textura.
    TextureCreation(String),
    /// ID de textura inválido (eliminada o generación obsoleta).
    InvalidTexture(u64),
    /// Fallo al compilar/enlazar un shader.
    ShaderCompilation {
        /// Etapa o programa que falló.
        stage: &'static str,
        /// Log del driver GL.
        log: String,
    },
    /// Fallo al crear el contexto EGL de backend.
    ContextCreation(String),
}

impl fmt::Display for CanvasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanvasError::TextureCreation(msg) => write!(f, "textura: {msg}"),
            CanvasError::InvalidTexture(id) => write!(f, "textura: ID {id:#018x} inválido"),
            CanvasError::ShaderCompilation { stage, log } => {
                write!(f, "shader {stage}: {log}")
            }
            CanvasError::ContextCreation(msg) => write!(f, "contexto gráfico: {msg}"),
        }
    }
}

impl std::error::Error for CanvasError {}
