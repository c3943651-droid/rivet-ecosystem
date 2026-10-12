//! Rivet Foundation: núcleo del ecosistema.
//!
//! Slab con generaciones (IDs de 64 bits: 32 índice + 32 generación),
//! bucle de eventos con `epoll`, configuración TOML, tipos de error y utilidades.

#![forbid(unsafe_op_in_unsafe_fn)]
