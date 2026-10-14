//! Core error types.
//!
//! Every fallible public operation in the ecosystem returns
//! [`Result<T>`](crate::error::Result) = `Result<T, FoundationError>`.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Alias de `Result` para toda la crate y el ecosistema.
pub type Result<T> = std::result::Result<T, FoundationError>;

/// Clases de error del [`Slab`](crate::slab::Slab).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlabErrorKind {
    /// El índice codificado en el ID está fuera de rango.
    InvalidId,
    /// La generación del ID no coincide: el slot fue reutilizado.
    StaleGeneration,
}

/// Clases de error del [`EventLoop`](crate::event_loop::EventLoop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLoopErrorKind {
    /// Fallo al crear el descriptor `epoll` o auxiliares.
    CreateFailed,
    /// Fallo al registrar un descriptor o temporizador.
    RegistrationFailed,
    /// Fallo al cancelar un temporizador o eliminar un registro.
    RemovalFailed,
    /// Fallo en `epoll_wait`.
    WaitFailed,
    /// Fallo al preparar la entrega de señales (`signalfd`).
    SignalSetupFailed,
    /// Fallo al despertar el bucle (`eventfd`).
    WakeFailed,
}

/// Clases de error de configuración.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigErrorKind {
    /// La ruta indicada no existe y no hay valores por defecto aplicables.
    NotFound,
    /// El TOML no se pudo parsear o viola el esquema.
    ParseFailed,
    /// Fallo al crear/configurar el `inotify` de hot-reload.
    WatchFailed,
}

/// Clases de error de logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoggingErrorKind {
    /// No se pudo abrir el archivo de log.
    OpenFailed,
    /// No se pudo escribir en el canal de salida.
    WriteFailed,
}

/// Error unificado del núcleo Rivet.
///
/// Los errores recuperables se devuelven como `Result`; las condiciones que
/// indican un bug del programador provocan `panic!` (ver `docs/rivet-base.md` §6).
#[derive(Debug)]
pub enum FoundationError {
    /// Operación inválida sobre el slab (ID inválido o generación obsoleta).
    Slab { kind: SlabErrorKind, id: u64 },
    /// Fallo del bucle de eventos.
    EventLoop {
        kind: EventLoopErrorKind,
        source: Option<io::Error>,
    },
    /// Fallo de carga/vigilancia de configuración.
    Config {
        kind: ConfigErrorKind,
        path: PathBuf,
        #[allow(dead_code)]
        source: Option<toml::de::Error>,
    },
    /// Fallo de sistema (syscall) sin categoría más específica.
    System { source: io::Error },
    /// Fallo del subsistema de logging.
    Logging {
        kind: LoggingErrorKind,
        source: Option<io::Error>,
    },
}

impl fmt::Display for FoundationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FoundationError::Slab { kind, id } => {
                let why = match kind {
                    SlabErrorKind::InvalidId => "índice fuera de rango",
                    SlabErrorKind::StaleGeneration => "generación obsoleta",
                };
                write!(f, "slab: ID {id:#018x} inválido ({why})")
            }
            FoundationError::EventLoop { kind, source } => {
                let why = match kind {
                    EventLoopErrorKind::CreateFailed => "creación de epoll/auxiliares",
                    EventLoopErrorKind::RegistrationFailed => "registro de descriptor/temporizador",
                    EventLoopErrorKind::RemovalFailed => "eliminación de registro",
                    EventLoopErrorKind::WaitFailed => "epoll_wait",
                    EventLoopErrorKind::SignalSetupFailed => "configuración de signalfd",
                    EventLoopErrorKind::WakeFailed => "despertar el bucle (eventfd)",
                };
                write!(f, "bucle de eventos: fallo en {why}")?;
                if let Some(src) = source {
                    write!(f, ": {src}")?;
                }
                Ok(())
            }
            FoundationError::Config { kind, path, .. } => {
                let why = match kind {
                    ConfigErrorKind::NotFound => "ruta no encontrada",
                    ConfigErrorKind::ParseFailed => "parseo TOML",
                    ConfigErrorKind::WatchFailed => "vigilancia inotify",
                };
                write!(f, "config: {why} en {}", path.display())
            }
            FoundationError::System { source } => write!(f, "error de sistema: {source}"),
            FoundationError::Logging { kind, source } => {
                let why = match kind {
                    LoggingErrorKind::OpenFailed => "apertura de archivo de log",
                    LoggingErrorKind::WriteFailed => "escritura de log",
                };
                write!(f, "logging: fallo en {why}")?;
                if let Some(src) = source {
                    write!(f, ": {src}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for FoundationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FoundationError::Slab { .. } => None,
            FoundationError::EventLoop { source, .. } => source.as_ref().map(|e| e as _),
            FoundationError::Config { source, .. } => source.as_ref().map(|e| e as _),
            FoundationError::System { source } => Some(source as _),
            FoundationError::Logging { source, .. } => source.as_ref().map(|e| e as _),
        }
    }
}

impl From<io::Error> for FoundationError {
    fn from(source: io::Error) -> Self {
        FoundationError::System { source }
    }
}

impl FoundationError {
    /// Constructor auxiliar para errores del bucle de eventos.
    pub(crate) fn event_loop(kind: EventLoopErrorKind, source: Option<io::Error>) -> Self {
        FoundationError::EventLoop { kind, source }
    }

    /// Constructor auxiliar para errores de configuración.
    pub(crate) fn config(kind: ConfigErrorKind, path: impl Into<PathBuf>) -> Self {
        FoundationError::Config {
            kind,
            path: path.into(),
            source: None,
        }
    }
}
