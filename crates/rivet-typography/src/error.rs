//! Errores de `rivet-typography`.

use std::fmt;

/// Resultado de las operaciones falibles.
pub type Result<T> = std::result::Result<T, TypographyError>;

/// Error del subsistema tipográfico.
#[derive(Debug)]
pub enum TypographyError {
    /// No se pudo leer el archivo de fuente.
    Io(std::io::Error),
    /// El binario no es una fuente TrueType/OpenType válida.
    InvalidFont(String),
    /// No hay fuentes disponibles (ni sistema ni embebidas).
    NoFonts,
    /// El ID solicitado no existe en la base.
    UnknownFont(FontId),
}

impl fmt::Display for TypographyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypographyError::Io(source) => write!(f, "fuente: E/S: {source}"),
            TypographyError::InvalidFont(msg) => write!(f, "fuente inválida: {msg}"),
            TypographyError::NoFonts => write!(f, "sin fuentes disponibles"),
            TypographyError::UnknownFont(id) => write!(f, "fuente desconocida: {:?}", id),
        }
    }
}

impl std::error::Error for TypographyError {}

impl From<std::io::Error> for TypographyError {
    fn from(source: std::io::Error) -> Self {
        TypographyError::Io(source)
    }
}

/// Identificador de una cara tipográfica en la [`FontDb`](crate::db::FontDb).
pub type FontId = fontdb::ID;
