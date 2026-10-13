//! Contexto del ecosistema: punto de integración con inyección explícita.
//!
//! `docs/rivet-foundation.md` §8: cada crate recibe `&Context`/`&mut Context`
//! (sin singletons). Se crea con [`ContextBuilder::build`]; el flujo típico es
//! construir contexto → cargar config → registrar módulos → `event_loop().run()`.

use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::config::Config;
use crate::error::Result;
use crate::event_loop::EventLoop;
use crate::logging::Logger;
use crate::slab::Slab;
use crate::utils::clock::SystemClock;

/// Valores del slab del contexto: objetos de dominio de cualquier crate.
type Stored = Box<dyn Any + Send + Sync>;

/// Contexto compartido del ecosistema.
///
/// Contiene el almacén de objetos (slab), el bucle de eventos, la
/// configuración (con hot-reload), el logger y el reloj.
///
/// Nota: el `EventLoop` usa `Rc` internamente, así que `Context` no es `Send`;
/// en v1 todo el ecosistema vive en un único hilo.
pub struct Context {
    slab: Slab<Stored>,
    event_loop: EventLoop,
    config: RwLock<Config>,
    logger: Logger,
    clock: SystemClock,
}

impl Context {
    /// Almacén de objetos del ecosistema (solo lectura).
    pub fn slab(&self) -> &Slab<Stored> {
        &self.slab
    }

    /// Almacén de objetos del ecosistema (mutable).
    pub fn slab_mut(&mut self) -> &mut Slab<Stored> {
        &mut self.slab
    }

    /// Bucle de eventos.
    pub fn event_loop(&mut self) -> &mut EventLoop {
        &mut self.event_loop
    }

    /// Configuración activa (guard de lectura).
    pub fn config(&self) -> std::sync::RwLockReadGuard<'_, Config> {
        self.config.read().expect("config envenenada")
    }

    /// Reemplaza la configuración activa (p. ej. tras un hot-reload).
    pub fn set_config(&self, config: Config) {
        *self.config.write().expect("config envenenada") = config;
    }

    /// Logger del contexto.
    #[must_use]
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// Reloj monotónico del contexto.
    #[must_use]
    pub fn clock(&self) -> &SystemClock {
        &self.clock
    }

    /// Guarda un objeto tipado en el slab y devuelve su ID.
    pub fn store<T>(&mut self, value: T) -> u64
    where
        T: Any + Send + Sync,
    {
        self.slab.insert(Box::new(value))
    }

    /// Recupera un objeto tipado por ID (falla si el tipo no coincide).
    pub fn retrieve<T>(&self, id: u64) -> Option<&T>
    where
        T: Any + Send + Sync,
    {
        self.slab.get(id)?.downcast_ref::<T>()
    }

    /// Recupera mutablemente un objeto tipado por ID.
    pub fn retrieve_mut<T>(&mut self, id: u64) -> Option<&mut T>
    where
        T: Any + Send + Sync,
    {
        self.slab.get_mut(id)?.downcast_mut::<T>()
    }

    /// Elimina y devuelve un objeto tipado por ID.
    pub fn take<T>(&mut self, id: u64) -> Option<T>
    where
        T: Any + Send + Sync,
    {
        self.slab
            .remove(id)?
            .downcast::<T>()
            .ok()
            .map(|boxed| *boxed)
    }

    /// Ejecuta el bucle de eventos hasta que algo llame a `quit`.
    pub fn run(&mut self) -> Result<()> {
        self.event_loop.run()
    }

    /// Marca el bucle para terminar.
    pub fn quit(&mut self) {
        self.event_loop.quit();
    }
}

/// Constructor de [`Context`].
#[derive(Debug, Clone)]
pub struct ContextBuilder {
    config_path: Option<PathBuf>,
}

impl ContextBuilder {
    /// Crea un builder con la ruta de configuración por defecto.
    #[must_use]
    pub fn new() -> Self {
        Self { config_path: None }
    }

    /// Ruta del archivo de configuración a cargar.
    #[must_use]
    pub fn config_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.config_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Construye el contexto: carga config, inicia el logger global y crea
    /// el bucle de eventos.
    pub fn build(self) -> Result<Context> {
        let path = self.config_path.unwrap_or_else(default_config_path);
        let config = Config::load(&path)?;
        crate::logging::init_global(config.logging.level, config.logging.file.as_deref())?;

        let event_loop = EventLoop::new(
            config.event_loop.wait_timeout_ms,
            config.event_loop.max_events_per_iteration,
        )?;
        let slab = Slab::with_capacity(config.core.slab_initial_capacity);

        Ok(Context {
            slab,
            event_loop,
            logger: Logger::new(config.logging.level),
            config: RwLock::new(config),
            clock: SystemClock::new(),
        })
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// `~/.config/rivet/config.toml` (con fallback a `/tmp` si no hay `$HOME`).
fn default_config_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".config").join("rivet").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn builder_crea_contexto_funcional() {
        let mut ctx = ContextBuilder::new()
            .config_path("/no/existe/rivet-foundation-test.toml")
            .build()
            .unwrap();
        assert_eq!(ctx.config().core.slab_initial_capacity, 1024);

        let id = ctx.store(41u32);
        *ctx.retrieve_mut::<u32>(id).unwrap() += 1;
        assert_eq!(ctx.retrieve::<u32>(id), Some(&42));
        assert_eq!(ctx.take::<u32>(id), Some(42));
        assert_eq!(ctx.retrieve::<u32>(id), None);
    }

    #[test]
    fn contexto_ejecuta_el_bucle() {
        let mut ctx = ContextBuilder::new()
            .config_path("/no/existe/rivet-foundation-test.toml")
            .build()
            .unwrap();
        ctx.event_loop()
            .add_timer(Duration::from_millis(5), None, Box::new(|d| d.quit()))
            .unwrap();
        ctx.run().unwrap();
    }
}
