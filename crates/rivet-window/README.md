# rivet-window

Native Wayland client of the [Rivet](https://github.com/c3943651-droid/rivet-ecosystem)
GUI toolkit, written against the wire protocol directly (scanned code,
no generated bindings to maintain by hand). It owns surfaces, input, and
buffer presentation.

## What it provides

- **Client connection**: `Wayland::connect()` performs the socket
  handshake and the `wl_registry` global negotiation.
- **Surfaces**: `xdg_shell` toplevel windows and `wlr_layer_shell`
  panels/overlays (for shell components such as bars and launchers).
- **Input**: pointer, keyboard (via `xkbcommon`), touch, and gesture
  events, delivered as a flat `Vec<WindowEvent>`.
- **Buffers**: `wl_shm` buffer pools backed by `memfd_create` +
  `mmap`, with multi-slot double/triple buffering.
- **HiDPI**: per-output scale factor tracking.
- **Integration**: `Wayland::register_in(&mut EventLoop)` plugs the
  compositor fd into the `rivet-base` epoll loop.

## Usage

```rust
use rivet_base::event_loop::EventLoop;
use rivet_window::{Wayland, WindowEvent, WindowOptions};

let mut wayland = Wayland::connect()?;
let mut loop_ = EventLoop::new(None, 64)?;

let window = wayland.create_window(
    WindowOptions::new(800, 600).title("Demo"),
)?;
wayland.register_in(&mut loop_)?;

// In the frame loop:
for event in wayland.take_events() {
    match event {
        WindowEvent::Close { .. } => { /* quit */ }
        WindowEvent::PointerMove { x, y, .. } => { /* ... */ }
        _ => {}
    }
}
```

## Notes

- Presentation uses `wl_shm` v1 (a CPU copy per frame). Zero-copy
  DMA-BUF is planned, gated on measurement (ADR-02 in
  [`docs/RIVET.md`](../../docs/RIVET.md)).
- Depends only on `rivet-base` plus the `wayland-client` and
  `xkbcommon` crates.
- `rivet-canvas` renders into these buffers but does **not** depend on
  this crate: `window` produces, `canvas` consumes.

## License

MIT OR Apache-2.0.
