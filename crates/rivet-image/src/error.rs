//! Errores de `rivet-image`.

use std::fmt;

/// Resultado de las operaciones falibles.
pub type Result<T> = std::result::Result<T, ImageError>;

/// Error del subsistema de imágenes.
#[derive(Debug)]
pub enum ImageError {
    /// Error de E/S al leer el archivo.
    Io(std::io::Error),
    /// El decodificador rechazó los bytes (formato inválido o corrupto).
    Decode(String),
    /// Formato no soportado por v1 (ver doc §8).
    UnsupportedFormat(String),
    /// Problema con el tema de iconos (index.theme inválido…).
    Theme(String),
    /// Icono o archivo no encontrado.
    NotFound(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::Io(source) => write!(f, "imagen: E/S: {source}"),
            ImageError::Decode(msg) => write!(f, "imagen: decode: {msg}"),
            ImageError::UnsupportedFormat(fmt_) => {
                write!(f, "imagen: formato no soportado: {fmt_}")
            }
            ImageError::Theme(msg) => write!(f, "tema de iconos: {msg}"),
            ImageError::NotFound(what) => write!(f, "no encontrado: {what}"),
        }
    }
}

impl std::error::Error for ImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ImageError::Io(source) => Some(source as _),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ImageError {
    fn from(source: std::io::Error) -> Self {
        ImageError::Io(source)
    }
}
