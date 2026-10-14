# rivet-window — Definitive documentation

**Window and Wayland surface** subsystem of Rivet.

- **Version:** 2.0 (definitive)
- **Date:** 2026-10-11
- **Layer:** 2 (communication with the window system)
- **Depends on:** `rivet-base`
- **Consumed by:** `rivet-canvas`, `rivet-ui`
- **Umbrella document:** `../RIVET.md`

---

## 1. Purpose

`rivet-window` manages the lifecycle of windows and surfaces on **Wayland** (with **`libxkbcommon`** for keyboard). It hands `rivet-canvas` a **surface ready for rendering** and **buffers**; it does **not** draw, and it knows nothing about pixels or widgets. It is the ecosystem's boundary with the compositor.

Separation rule (ADR in `../RIVET.md`): **`rivet-window` hands surface/buffer to `rivet-canvas`; `rivet-canvas` does NOT depend on `rivet-window`.** Rendering receives an abstract `wgpu`/EGL surface, not a window object.

## 2. Components

| Module | Responsibility |
|---|---|
| `connection` | Connection to the compositor, `wl_registry`, handshake. |
| `surface` | `wl_surface` + role (toplevel / layer-shell / popup / subsurface). |
| `buffer` | Pixel buffer: `wl_shm` (v1) and DMA-BUF (v1.x). |
| `input` | Keyboard (`xkbcommon`), pointer, touch, gestures. |
| `window` | `Window` object: state, size, decorations, visibility. |
| `display` | Outputs, HiDPI/scale, vsync, modes. |
| `frame` | Frame synchronization (`wl_callback`). |
| `clipboard` | Selection and *drag & drop*. |
| `error` | `WindowError`. |

## 3. Connection and handshake

1. `wl_display_connect` → `wl_display_get_registry` → listen for `wl_registry.global`/`global_remove`.
2. `wl_display_roundtrip` to enumerate globals.
3. Acquire **bindings** (`wl_compositor`, `wl_shm`, `wl_seat`, `xdg_wm_base`, `zxdg_output_manager_v1`, `zwp_layer_shell_v1`, `wp_viewporter`, `zwp_linux_dmabuf_v1` if present).
4. Check the minimum version of each interface; degrade gracefully if an optional one is missing (e.g. without `xdg_wm_base` there are no toplevels).
5. Integrate the Wayland fd into the `epoll` event loop of `rivet-base`.

## 4. Surface role

- **Toplevel** (`xdg_toplevel`): normal window; title, size, states (maximized, fullscreen, activated).
- **Layer-shell** (`zwp_layer_shell_v1`): anchored surfaces — **base for the panel and launcher** of the seed applications.
- **Popup** (`xdg_popup`): menus and tooltips, positioned via `xdg_positioner`.
- **Subsurface** (`wl_subsurface`): composition inside another surface.

## 5. Buffers — **important correction**

There are two paths, and they are not equivalent:

- **`wl_shm` (v1):** buffer in shared memory. The client writes pixels on the CPU and the compositor consumes them. **This is NOT zero-copy**: there is one copy per frame. Simple and universal; it is the v1 baseline.
- **DMA-BUF (`zwp_linux_dmabuf_v1`, v1.x):** GPU buffer shared between client and compositor. **This is the zero-copy path.** It is exported from GL with **`EGL_MESA_image_dma_buf_export`** (NOT `EGL_EXT_image_dma_buf_import`, which imports *into* GL). It requires **explicit synchronization** (`linux-drm-syncobj-v1`) to avoid *tearing*/race conditions.

Decision (ADR-02): **v1 uses `wl_shm`**; migration to DMA-BUF lands in **v1.x**, **driven by measurement** (only if `wl_shm` throughput limits the framerate target), not preemptively.

Presentation cycle: take a free buffer → draw (done by canvas) → `wl_surface.attach` + `damage` + `commit` → the compositor presents it → it is returned via `wl_buffer.release`.

## 6. Input

- **Keyboard:** `wl_keyboard` + **`libxkbcommon`** for *keymap*, modifiers, layout, repeat. The `keymap` sent by the compositor is used.
- **Pointer:** `wl_pointer` (enter/leave, motion, buttons, scroll axis/discrete, `axis_value120`), cursors (`wl_cursor_theme`).
- **Touch:** `wl_touch` (down/up/motion/cancel), multi-touch.
- **Gestures:** `zwp_pointer_gestures_v1` (swipe/pinch/hold), `zwp_pointer_constraints_v1`, `zwp_relative_pointer_v1`.
- Events are translated to a **Rivet-defined type** and routed to `rivet-ui`; `rivet-window` handles neither focus nor *hit-testing*.

## 7. Outputs, HiDPI, and synchronization

- `zxdg_output_manager_v1`: logical geometry, per-output scale, name, `done`.
- **HiDPI:** respect integer `scale` and `fractional_scale_v1`; surfaces sized as multiples of the scale for sharpness. Logical vs. physical size is handled here.
- **VSync:** `eglSwapInterval(display, 1)` for presentation throttling (GL). **Do not** use `EGL_SWAP_BEHAVIOR_PRESERVED` for this.
- **Frame callback:** `wl_surface.frame` → `wl_callback.done` to pace redrawing and avoid over-drawing.

## 8. Clipboard and drag & drop

- `wl_data_device_manager`: selection (copy/paste) and DnD.
- Own MIME types (`text/plain;charset=utf-8`, `text/uri-list`, image).
- Transfer via `wl_data_source`/`wl_data_offer` with `pipe` (or `memfd`) managed in the event loop.

## 9. Public API (indicative)

```rust
pub struct Connection;           // handshake and globals
pub struct Window {               // state, role, size, visibility
    pub fn new(ctx: &Context, opts: WindowOptions) -> Result<Self, WindowError>;
    pub fn surface(&self) -> &Surface;
    pub fn set_size(&mut self, logical: Size) -> Result<(), WindowError>;
    pub fn commit(&mut self) -> Result<(), WindowError>;
}
pub struct Surface;               // wl_surface + role
pub struct Buffer;                // shm or dmabuf
pub struct InputEvent;            // unified enum
pub enum WindowError { ... }
```

## 10. Dependencies

- **Rust:** Wayland binding (`wayland-client`/`wayland-protocols` — Rust) or minimal FFI to `libwayland`; **`libxkbcommon`** (system C, allowed).
- **GL:** only the *display*/EGL needed for the context (shared with canvas); no pixels here.
- No GUI C libraries; no external toolkit.

## 11. Scope

**v1:** connection + registry, toplevel, `wl_shm`, keyboard/pointer/touch, outputs and basic HiDPI, frame callback, basic clipboard/DnD, layer-shell for panel/launcher.

**v1.x (measurement-driven):** DMA-BUF + `linux-drm-syncobj-v1`, advanced `fractional_scale`, gestures, server-side vs client-side decorations.

**Out:** X11 (Wayland only), remote sessions, explicit multi-GPU.

## 12. Decisions and applied corrections

- **`wl_shm` is not zero-copy** (copy involved); zero-copy = **DMA-BUF**. The contrary claim has been withdrawn.
- DMA-BUF export with **`EGL_MESA_image_dma_buf_export`** (import would be `EGL_EXT_image_dma_buf_import`).
- **VSync via `eglSwapInterval`**, not `EGL_SWAP_BEHAVIOR_PRESERVED`.
- **`rivet-canvas` does not depend on `rivet-window`** (ADR in `../RIVET.md`).
- No "0% CPU" promise (ADR-08).

## 13. Metrics (targets, to be measured — `../RIVET.md` §12)

Event→frame latency, per-frame cost with `wl_shm` vs DMA-BUF, RAM usage of a 1080p buffer, handshake time, presentation overhead. Method: `perf`, ftrace, own frame counters.
