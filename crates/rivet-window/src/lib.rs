//! Rivet Window: cliente Wayland nativo.
//!
//! Gestiona socket, `wl_registry`, `xdg_shell`, `layer_shell`, popups,
//! subsurfaces, entrada (`wl_seat`), buffers (`memfd_create`/`wl_shm`, DMA-BUF)
//! e HiDPI. Crea y presenta la superficie/buffer; **no** renderiza.
