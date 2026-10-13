//! Errores de `rivet-style`.

use std::fmt;

/// Resultado de las operaciones falibles.
pub type Result<T> = std::result::Result<T, StyleError>;

/// Error del subsistema de estilos.
#[derive(Debug, Clone)]
pub enum StyleError {
    /// Error léxico con línea aproximada.
    Lex { line: usize, message: String },
    /// Error de sintaxis con línea aproximada.
    Parse { line: usize, message: String },
    /// Referencia a un widget/tipo desconocido.
    UnknownWidget(String),
    /// Tema solicitado no declarado en el DSL.
    UnknownTheme(String),
    /// Valor/propiedad inválidos.
    InvalidValue(String),
    /// Widget declarado dos veces.
    DuplicateWidget(String),
    /// Referencia a variable o propiedad sin resolver.
    Unresolved(String),
}

impl StyleError {
    pub(crate) fn invalid_value(message: impl Into<String>) -> Self {
        StyleError::InvalidValue(message.into())
    }
}

impl fmt::Display for StyleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StyleError::Lex { line, message } => {
                write!(f, "estilo (línea {line}): léxico: {message}")
            }
            StyleError::Parse { line, message } => {
                write!(f, "estilo (línea {line}): sintaxis: {message}")
            }
            StyleError::UnknownWidget(name) => write!(f, "estilo: widget desconocido {name:?}"),
            StyleError::UnknownTheme(name) => write!(f, "estilo: tema no declarado {name:?}"),
            StyleError::InvalidValue(message) => write!(f, "estilo: valor inválido: {message}"),
            StyleError::DuplicateWidget(name) => write!(f, "estilo: widget duplicado {name:?}"),
            StyleError::Unresolved(name) => write!(f, "estilo: referencia sin resolver {name:?}"),
        }
    }
}

impl std::error::Error for StyleError {}
