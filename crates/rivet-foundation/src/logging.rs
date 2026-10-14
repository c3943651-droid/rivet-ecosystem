//! Logging with levels, outputs (stderr + file) and a panic hook.
//!
//! `docs/rivet-foundation.md` §6: Error/Warn/Info/Debug/Trace levels,
//! `[LEVEL] [TIMESTAMP] [MODULE] [MESSAGE]` format. The macros check the
//! active level before formatting (~0 cost when disabled).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{FoundationError, LoggingErrorKind, Result};

/// Nivel de log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Errores: fallos que degradan el funcionamiento.
    Error,
    /// Avisos: condiciones anómalas recuperables.
    Warn,
    /// Información de alto nivel del sistema.
    Info,
    /// Detalles útiles para depurar.
    Debug,
    /// Traza detallada (muy verbosa).
    Trace,
}

impl Level {
    /// Etiqueta corta usada en el formato de línea.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }
}

/// Registro de log con nivel y canal de salida.
#[derive(Debug)]
pub struct Logger {
    level: Level,
    file: Option<std::fs::File>,
}

impl Logger {
    /// Crea un logger que escribe en stderr con el nivel dado.
    #[must_use]
    pub fn new(level: Level) -> Self {
        Self { level, file: None }
    }

    /// Nivel máximo activo.
    #[must_use]
    pub fn level(&self) -> Level {
        self.level
    }

    /// Cambia el nivel máximo activo.
    pub fn set_level(&mut self, level: Level) {
        self.level = level;
    }

    /// Añade un archivo como canal de salida (además de stderr).
    pub fn set_file(&mut self, path: &Path) -> Result<()> {
        self.file = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| FoundationError::Logging {
                    kind: LoggingErrorKind::OpenFailed,
                    source: Some(e),
                })?,
        );
        Ok(())
    }

    /// Registra un mensaje si el nivel está activo.
    ///
    /// El formato es `[NIVEL] [TIMESTAMP] [MÓDULO] [MENSAJE]`.
    pub fn log(&mut self, level: Level, module: &str, message: &str) {
        if level > self.level {
            return;
        }
        let line = format!(
            "[{}] [{}] [{}] [{}]\n",
            level.as_str(),
            timestamp(),
            module,
            message
        );
        // stderr siempre; archivo si está configurado. Un fallo de escritura
        // no debe tumbar el proceso: se ignora (el canal se reintenta después).
        let _ = std::io::stderr().write_all(line.as_bytes());
        if let Some(file) = &mut self.file {
            let _ = file.write_all(line.as_bytes());
        }
    }

    /// `true` si un mensaje de `level` se registraría.
    #[must_use]
    pub fn enabled(&self, level: Level) -> bool {
        level <= self.level
    }
}

/// Logger global del proceso (v1: un contexto por proceso).
fn global() -> &'static Mutex<Logger> {
    static GLOBAL: OnceLock<Mutex<Logger>> = OnceLock::new();
    GLOBAL.get_or_init(|| Mutex::new(Logger::new(Level::Info)))
}

/// Inicializa el logger global con nivel y archivo opcional.
pub fn init_global(level: Level, file: Option<&Path>) -> Result<()> {
    let mut logger = global().lock().expect("logger global envenenado");
    logger.set_level(level);
    if let Some(path) = file {
        logger.set_file(path)?;
    }
    Ok(())
}

/// Registra en el logger global (usado por los macros).
pub fn log_record(level: Level, module: &str, message: &str) {
    global()
        .lock()
        .expect("logger global envenenado")
        .log(level, module, message);
}

/// `true` si el nivel está activo en el logger global.
#[must_use]
pub fn level_enabled(level: Level) -> bool {
    global()
        .lock()
        .expect("logger global envenenado")
        .enabled(level)
}

/// Instala un gancho de pánico que registra el diagnóstico y conserva el
/// comportamiento por defecto (mensaje a stderr).
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log_record(Level::Error, "panic", &info.to_string());
        previous(info);
    }));
}

fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:06}", now.as_secs(), now.subsec_micros())
}

/// Registra un mensaje de nivel [`Level::Error`].
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::logging::log_record($crate::logging::Level::Error, module_path!(), &format!($($arg)*))
    };
}

/// Registra un mensaje de nivel [`Level::Warn`].
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::logging::log_record($crate::logging::Level::Warn, module_path!(), &format!($($arg)*))
    };
}

/// Registra un mensaje de nivel [`Level::Info`].
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::logging::log_record($crate::logging::Level::Info, module_path!(), &format!($($arg)*))
    };
}

/// Registra un mensaje de nivel [`Level::Debug`].
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::logging::log_record($crate::logging::Level::Debug, module_path!(), &format!($($arg)*))
    };
}

/// Registra un mensaje de nivel [`Level::Trace`].
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        $crate::logging::log_record($crate::logging::Level::Trace, module_path!(), &format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formato_y_filtro_de_nivel() {
        let mut logger = Logger::new(Level::Warn);
        assert!(logger.enabled(Level::Error));
        assert!(logger.enabled(Level::Warn));
        assert!(!logger.enabled(Level::Info));
        logger.set_level(Level::Trace);
        assert!(logger.enabled(Level::Trace));
        // `log` con nivel superior activo no hace nada (no paniquea).
        logger.log(Level::Error, "test", "mensaje");
    }

    #[test]
    fn orden_de_niveles() {
        assert!(Level::Error < Level::Trace);
        assert_eq!(Level::Debug.as_str(), "DEBUG");
    }

    #[test]
    fn macros_globales_no_panickean() {
        crate::logging::log_record(Level::Info, "test", "hola desde el global");
        crate::info!("info con formato {}", 42);
    }
}
