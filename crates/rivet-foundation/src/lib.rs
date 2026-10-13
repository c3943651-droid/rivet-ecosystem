//! Rivet Foundation: núcleo del ecosistema.
//!
//! Slab con generaciones (IDs de 64 bits: 32 índice + 32 generación),
//! bucle de eventos con `epoll`/`timerfd`/`signalfd`, configuración TOML con
//! hot-reload, tipos de error, logging y utilidades comunes.
//!
//! Nivel más bajo del ecosistema: no depende de ninguna otra crate Rivet.
#![forbid(unsafe_op_in_unsafe_fn)]

pub mod config;
pub mod context;
pub mod error;
pub mod event_loop;
pub mod logging;
pub mod slab;
pub mod utils;

pub use config::{Config, ConfigWatcher};
pub use context::{Context, ContextBuilder};
pub use error::{FoundationError, Result};
pub use event_loop::EventLoop;
pub use slab::Slab;
pub use utils::clock::{Clock, MockClock, SystemClock};
pub use utils::ids::{id_generation, id_index, make_id};
pub use utils::utf8::truncate_utf8;
