//! Errors of `rivet-ui`.

use std::fmt;

/// Resultado de las operaciones falibles.
pub type Result<T> = std::result::Result<T, UiError>;

/// Error del subsistema de UI.
#[derive(Debug)]
pub enum UiError {
    /// ID de widget inválido (destruido o generación obsoleta).
    InvalidWidget(u64),
    /// Operación de árbol inválida (p. ej. ancestro descendiente).
    InvalidTree(String),
    /// La operación requiere un tipo distinto de widget.
    WrongKind { id: u64, expected: &'static str },
    /// Fallo de un subsistema inferior propagado.
    Subsystem(String),
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UiError::InvalidWidget(id) => write!(f, "ui: widget {id:#018x} inválido"),
            UiError::InvalidTree(msg) => write!(f, "ui: árbol: {msg}"),
            UiError::WrongKind { id, expected } => {
                write!(f, "ui: widget {id:#018x} no es {expected}")
            }
            UiError::Subsystem(msg) => write!(f, "ui: subsistema: {msg}"),
        }
    }
}

impl std::error::Error for UiError {}
