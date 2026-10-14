//! Rivet Window: native Wayland client.
//!
//! Manages `wl_registry`, `xdg_shell` (toplevels), input (`wl_seat` +
//! `libxkbcommon`), `wl_shm` buffers (memfd + mmap) and frame callbacks,
//! integrated with the `epoll` event loop of `rivet-base`.
//!
//! It does **not** render: it hands the surface/buffer to `rivet-canvas` (canvas does
//! not depend on window — a fixed boundary of the ecosystem).
//!
//! v1: toplevel + layer-shell + wl_shm + keyboard/pointer/touch/gestures +
//! text clipboard/DnD + integer HiDPI.
//! v1.x horizon: DMA-BUF, client-side decorations.

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
