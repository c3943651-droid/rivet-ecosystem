# Rivet

[![CI](https://github.com/c3943651-droid/rivet-ecosystem/actions/workflows/ci.yml/badge.svg)](https://github.com/c3943651-droid/rivet-ecosystem/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![MSRV](https://img.shields.io/badge/MSRV-1.75-orange.svg)](#stability-and-support)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Wayland-lightgrey.svg)](#requirements)

A GUI toolkit for Linux, written end-to-end in Rust, targeting Wayland.

## Overview

Rivet is a set of libraries for building desktop applications: windows,
widgets, text rendering, images, styling, and the event loop that connects
them. Each library has a single responsibility and can be used on its own;
the `rivet` facade re-exports the whole set for application code.

The GUI layer contains no C dependencies. Where a problem is solved by a
mature pure-Rust crate, that crate is used; functionality specific to a
toolkit (widget storage, layout, rasterization, styling, the Wayland client)
is implemented in this repository. The full dependency policy is documented
in [`docs/RIVET.md` §6](docs/RIVET.md).

Performance figures are only published alongside measurement data; the
measurement protocol and the current results are in
[`docs/RIVET.md` §14](docs/RIVET.md).

## Requirements

- Linux with a Wayland session (KWin, sway, GNOME, or another compositor
  supporting `xdg_shell`).
- Mesa with EGL (hardware GPU or llvmpipe).
- Rust 1.75 or newer.

## Installation

The crates are not yet published to crates.io. Add the facade as a git
dependency:

```toml
[dependencies]
rivet = { git = "https://github.com/c3943651-droid/rivet-ecosystem", branch = "develop" }
```

Individual layers can be used without the facade, for example
`rivet-canvas` (GPU 2D renderer) or `rivet-typography` (font rasterization)
on their own.

## Quick start

A window with a label and a button:

```rust
use rivet::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Application::new("Hello, Rivet", 360, 200)?;

    {
        let ui = app.ui_mut();
        let root = ui.registry.create_panel(true);
        ui.registry.set_root(root)?;

        let title = ui.registry.create_label("Hello, Rivet");
        let close = ui.registry.create_button("Quit", |ui| ui.quit());

        ui.registry.add_child(root, title)?;
        ui.registry.add_child(root, close)?;
    }

    app.run()?;
    Ok(())
}
```

The same example is available in
[`crates/rivet/examples/hello.rs`](crates/rivet/examples/hello.rs) and runs
with `cargo run -p rivet --example hello`.

## Stability and support

Version 0.1.0. The initial scope is implemented and passing CI, but the
project is young and the API should be expected to change before 1.0.

- Wayland only. There is no X11 support.
- The widget catalog is small: containers, buttons, labels, images, and
  single-line text inputs. Scrolling containers, menus, and multiline text
  are not implemented yet.
- Presentation uses `wl_shm` (a per-frame CPU copy); DMA-BUF zero-copy is
  planned, gated on measurement ([ADR-02](docs/RIVET.md)).
- Every change to `develop` passes `cargo fmt --check`,
  `clippy -D warnings`, and the full test suite in CI.

## Architecture

| Crate | Responsibility |
|---|---|
| `rivet-foundation` | Generational slab, epoll event loop, TOML config, logging |
| `rivet-window` | Wayland client: toplevel and layer-shell surfaces, input, clipboard, drag-and-drop, HiDPI |
| `rivet-canvas` | GPU 2D renderer (OpenGL 3.3 Core + EGL): primitives, batching, textures, effects |
| `rivet-typography` | TTF/OTF parsing, rasterization with subpixel positioning, kerning, glyph cache and atlas |
| `rivet-image` | Image decoding (PNG/JPEG/WebP/SVG/XPM) and freedesktop icon themes |
| `rivet-style` | Style DSL compiled to flat tables; themes and widget states |
| `rivet-ui` | Widgets, layout, hit-testing, focus, and `Application` |
| `rivet` | Facade with prelude |

Dependency direction: `window` produces surfaces and presents buffers;
`canvas` renders into them. `canvas` does not depend on `window`.

## Documentation

- [`docs/RIVET.md`](docs/RIVET.md) — architecture, closed design decisions
  (ADR-01..08), dependency policy, and the measurement registry.
- `docs/<crate>.md` — per-crate documentation.
- Rust API docs: `cargo doc --open -p rivet`.

## License

MIT OR Apache-2.0 — see [`LICENSE-MIT`](LICENSE-MIT) and
[`LICENSE-APACHE`](LICENSE-APACHE). The bundled Inter Regular font is
distributed under the SIL Open Font License (see
`crates/rivet-typography/assets/`).
