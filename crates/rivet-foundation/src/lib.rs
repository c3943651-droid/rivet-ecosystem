//! Rivet Foundation: ecosystem core.
//!
//! Generational slab (64-bit IDs: 32-bit index + 32-bit generation),
//! event loop with `epoll`/`timerfd`/`signalfd`, TOML configuration with
//! hot-reload, error types, logging and common utilities.
//!
//! Lowest level of the ecosystem: it does not depend on any other Rivet crate.
#![forbid(unsafe_op_in_unsafe_fn)]

pub mod channel;
pub mod config;
pub mod context;
pub mod error;
pub mod event_loop;
pub mod logging;
pub mod slab;
pub mod utils;

pub use channel::{channel, Receiver, Sender};
pub use config::{Config, ConfigWatcher};
pub use context::{Context, ContextBuilder};
pub use error::{FoundationError, Result};
pub use event_loop::EventLoop;
pub use slab::Slab;
pub use utils::clock::{Clock, MockClock, SystemClock};
pub use utils::ids::{id_generation, id_index, make_id};
pub use utils::utf8::truncate_utf8;
