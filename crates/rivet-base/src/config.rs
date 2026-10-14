//! Typed TOML configuration with default values and hot-reload.
//!
//! `docs/rivet-base.md` §5: priority `~/.config/rivet/config.toml` +
//! defaults (configurable path); hot-reload via `inotify` registered in the
//! event loop (100 ms cooldown). If parsing fails, the previous
//! configuration is kept and a warning is issued (the watcher delivers the `Err`).

use std::cell::RefCell;
use std::fs;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{ConfigErrorKind, FoundationError, Result};
use crate::event_loop::EventLoop;
use crate::logging::Level;

/// Sección `[core]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreConfig {
    /// Capacidad inicial preasignada del slab del contexto.
    pub slab_initial_capacity: usize,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            slab_initial_capacity: 1024,
        }
    }
}

/// Sección `[event_loop]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EventLoopConfig {
    /// Espera máxima de `epoll_wait` en ms; `-1` bloquea indefinidamente.
    pub wait_timeout_ms: Option<u64>,
    /// Tamaño del lote de despacho por iteración.
    pub max_events_per_iteration: usize,
}

impl Default for EventLoopConfig {
    fn default() -> Self {
        Self {
            wait_timeout_ms: None,
            max_events_per_iteration: 64,
        }
    }
}

/// Sección `[logging]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Nivel máximo de log.
    pub level: Level,
    /// Archivo de log opcional (además de stderr).
    pub file: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: Level::Info,
            file: None,
        }
    }
}

/// Sección `[paths]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    /// Directorio de configuración; `None` = `~/.config/rivet`.
    pub config_dir: Option<PathBuf>,
}

/// Configuración raíz de Rivet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Ajustes del núcleo.
    pub core: CoreConfig,
    /// Ajustes del bucle de eventos.
    pub event_loop: EventLoopConfig,
    /// Ajustes de logging.
    pub logging: LoggingConfig,
    /// Rutas configurables.
    pub paths: PathsConfig,
}

impl Config {
    /// Carga la configuración desde `path`; si no existe, devuelve defaults.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)?;
        Self::parse(&raw, path)
    }

    /// Parsea TOML crudo (visible para tests y hot-reload).
    pub fn parse(raw: &str, path: &Path) -> Result<Self> {
        toml::from_str(raw).map_err(|e| FoundationError::Config {
            kind: ConfigErrorKind::ParseFailed,
            path: path.to_path_buf(),
            source: Some(e),
        })
    }
}

/// Vigila un archivo de configuración y recarga en caliente.
///
/// Se registra un `inotify` sobre el directorio padre (los editores suelen
/// reemplazar el archivo por rename) en el [`EventLoop`] indicado. Con
/// cooldown de 100 ms; si el TOML falla, el callback recibe `Err` y **la
/// configuración anterior se conserva**.
pub struct ConfigWatcher {
    inotify_fd: RawFd,
    path: PathBuf,
    #[allow(dead_code)]
    watch_descriptor: i32,
}

impl ConfigWatcher {
    /// Crea el watcher y lo registra en `event_loop`.
    ///
    /// `on_reload` se llama con la configuración releída (`Ok`) o con el
    /// error de parseo (`Err`).
    pub fn new<F>(path: &Path, event_loop: &mut EventLoop, on_reload: F) -> Result<Self>
    where
        F: FnMut(Result<Config>) + 'static,
    {
        // SAFETY: syscall con flags válidos; comprobamos el resultado.
        let inotify_fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) };
        if inotify_fd < 0 {
            return Err(FoundationError::config(ConfigErrorKind::WatchFailed, path));
        }
        // Vigila el directorio padre para sobrevivir a replaces por rename.
        let (watch_dir, file_name) = split_watch_target(path);
        // SAFETY: syscall con argumentos válidos; comprobamos el resultado.
        let wd = unsafe {
            libc::inotify_add_watch(
                inotify_fd,
                watch_dir.to_string_lossy().as_ptr().cast(),
                libc::IN_MODIFY | libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_CREATE,
            )
        };
        if wd < 0 {
            // SAFETY: fd creado por nosotros y aún abierto.
            unsafe { libc::close(inotify_fd) };
            return Err(FoundationError::config(ConfigErrorKind::WatchFailed, path));
        }

        let path = path.to_path_buf();
        let on_reload: Rc<RefCell<dyn FnMut(Result<Config>)>> = Rc::new(RefCell::new(on_reload));
        let handler_path = path.clone();
        let handler_reload = Rc::clone(&on_reload);
        // Cooldown compartido entre invocaciones del manejador.
        let mut last_fire: Option<Instant> = None;
        event_loop
            .register_fd(
                inotify_fd,
                crate::event_loop::interest::READABLE,
                Box::new(move |_fd, _flags, _dispatcher| {
                    process_inotify(
                        inotify_fd,
                        &handler_path,
                        &file_name,
                        &mut last_fire,
                        &handler_reload,
                    );
                }),
            )
            .map_err(|_| FoundationError::config(ConfigErrorKind::WatchFailed, path.clone()))?;

        Ok(Self {
            inotify_fd,
            path,
            watch_descriptor: wd,
        })
    }

    /// Ruta vigilada.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        // SAFETY: inotify fd creado por nosotros; el bucle también lo cierra
        // no: el bucle nunca cierra fds de terceros, así que lo cerramos aquí.
        // El unregister en el bucle es responsabilidad del dueño del bucle
        // (normalmente se destruyen juntos).
        unsafe { libc::close(self.inotify_fd) };
    }
}

fn split_watch_target(path: &Path) -> (PathBuf, String) {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.toml".into());
    let watch_dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    (watch_dir, file_name)
}

fn process_inotify(
    fd: RawFd,
    config_path: &Path,
    file_name: &str,
    last_fire: &mut Option<Instant>,
    on_reload: &Rc<RefCell<dyn FnMut(Result<Config>)>>,
) {
    let mut buf = [0u8; 4096];
    // SAFETY: lectura en buffer propio del tamaño indicado.
    let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
    if n <= 0 {
        return;
    }
    // Estructura inotify_event: wd(i32) mask(u32) cookie(u32) len(u32) name.
    let mut relevant = false;
    let mut off = 0usize;
    while off + 16 <= n as usize {
        let len = u32::from_le_bytes(
            buf[off + 12..off + 16]
                .try_into()
                .expect("inotify: 4 bytes"),
        ) as usize;
        let name = &buf[off + 16..(off + 16 + len).min(n as usize)];
        let name = String::from_utf8_lossy(
            &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())],
        );
        if name == file_name {
            relevant = true;
        }
        off += 16 + len;
    }
    if !relevant {
        return;
    }
    // Cooldown: evita ráfagas de escritura (editores, formateadores).
    if last_fire.is_some_and(|t| t.elapsed() < Duration::from_millis(100)) {
        return;
    }
    *last_fire = Some(Instant::now());
    let result = Config::load(config_path);
    (on_reload.borrow_mut())(result);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cuando_no_existe() {
        let cfg = Config::load(Path::new("/no/existe/nunca.toml")).unwrap();
        assert_eq!(cfg.core.slab_initial_capacity, 1024);
        assert_eq!(cfg.event_loop.max_events_per_iteration, 64);
        assert_eq!(cfg.logging.level, Level::Info);
    }

    #[test]
    fn parse_toml_completo() {
        let raw = r#"
[core]
slab_initial_capacity = 64

[event_loop]
wait_timeout_ms = 100
max_events_per_iteration = 16

[logging]
level = "debug"
file = "/tmp/rivet-test.log"
"#;
        let cfg = Config::parse(raw, Path::new("test.toml")).unwrap();
        assert_eq!(cfg.core.slab_initial_capacity, 64);
        assert_eq!(cfg.event_loop.wait_timeout_ms, Some(100));
        assert_eq!(cfg.event_loop.max_events_per_iteration, 16);
        assert_eq!(cfg.logging.level, Level::Debug);
        assert_eq!(cfg.logging.file, Some(PathBuf::from("/tmp/rivet-test.log")));
    }

    #[test]
    fn toml_invalido_da_parse_failed() {
        let err = Config::parse("esto [no es << toml", Path::new("bad.toml")).unwrap_err();
        assert!(matches!(
            err,
            FoundationError::Config {
                kind: ConfigErrorKind::ParseFailed,
                ..
            }
        ));
    }

    #[test]
    fn toml_incompleto_toma_defaults_por_seccion() {
        let cfg =
            Config::parse("[core]\nslab_initial_capacity = 7\n", Path::new("p.toml")).unwrap();
        assert_eq!(cfg.core.slab_initial_capacity, 7);
        assert_eq!(cfg.event_loop.max_events_per_iteration, 64);
    }
}
