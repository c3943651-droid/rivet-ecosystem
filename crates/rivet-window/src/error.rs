//! Errores de `rivet-window`.

use std::fmt;

/// Resultado de las operaciones falibles de `rivet-window`.
pub type Result<T> = std::result::Result<T, WindowError>;

/// Error del subsistema de ventanas.
#[derive(Debug)]
pub enum WindowError {
    /// No se pudo conectar al compositor Wayland (`$WAYLAND_DISPLAY`).
    Connect(String),
    /// El compositor no ofrece un global requerido (p. ej. `xdg_wm_base`).
    MissingGlobal(String),
    /// La ventana aún no recibió su primer `configure` del compositor.
    NotConfigured,
    /// No hay buffer libre; reintenta en el próximo evento `Frame`.
    WouldBlock,
    /// Tamaño o parámetros de buffer inválidos.
    InvalidBuffer(String),
    /// Fallo de una syscall de sistema (memfd, mmap, ftruncate…).
    System(std::io::Error),
}

impl fmt::Display for WindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowError::Connect(msg) => write!(f, "wayland: {msg}"),
            WindowError::MissingGlobal(global) => {
                write!(f, "wayland: el compositor no ofrece `{global}`")
            }
            WindowError::NotConfigured => {
                write!(f, "wayland: la ventana aún no está configurada")
            }
            WindowError::WouldBlock => write!(f, "wayland: sin buffers libres"),
            WindowError::InvalidBuffer(msg) => write!(f, "buffer: {msg}"),
            WindowError::System(source) => write!(f, "sistema: {source}"),
        }
    }
}

impl std::error::Error for WindowError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WindowError::System(source) => Some(source as _),
            _ => None,
        }
    }
}

impl From<std::io::Error> for WindowError {
    fn from(source: std::io::Error) -> Self {
        WindowError::System(source)
    }
}
