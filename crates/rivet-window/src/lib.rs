//! Rivet Window: cliente Wayland nativo.
//!
//! Gestiona `wl_registry`, `xdg_shell` (toplevels), entrada (`wl_seat` +
//! `libxkbcommon`), buffers `wl_shm` (memfd + mmap) y frame callbacks,
//! integrados con el bucle `epoll` de `rivet-foundation`.
//!
//! **No** renderiza: entrega superficie/buffer a `rivet-canvas` (canvas no
//! depende de window — frontera fija del ecosistema).
//!
//! v1: toplevel + layer-shell + wl_shm + teclado/puntero/touch/gestos +
//! clipboard/DnD de texto + HiDPI entero.
//! Horizonte v1.x: DMA-BUF, decoraciones del lado del cliente.

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod error;
pub mod event;
pub mod shm;
pub mod state;
pub mod window;

pub use error::{Result, WindowError};
pub use event::{GesturePhase, KeyEvent, Modifiers, MouseButton, WindowEvent, WindowId};
pub use shm::ShmPixels;
pub use window::{
    KeyboardInteractivity, Layer, LayerAnchors, LayerSurfaceOptions, Wayland, Window, WindowOptions,
};
