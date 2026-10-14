# rivet-gui

Facade crate of the [Rivet](https://github.com/c3943651-droid/rivet-ecosystem)
GUI toolkit for Linux/Wayland. It re-exports the public API of the
whole ecosystem under one dependency so applications can write
`use rivet_gui::prelude::*;` and get to work.

The crate is a thin convenience layer: it adds no logic, and every
underlying crate remains usable on its own.

## The crates behind the facade

| Crate | Responsibility |
|---|---|
| [`rivet-base`](https://crates.io/crates/rivet-base) | Generational slab, epoll event loop, TOML config, logging |
| [`rivet-window`](https://crates.io/crates/rivet-window) | Native Wayland client: surfaces, input, `wl_shm` buffers, HiDPI |
| [`rivet-canvas`](https://crates.io/crates/rivet-canvas) | GPU 2D renderer (OpenGL 3.3 Core + EGL) |
| [`rivet-typography`](https://crates.io/crates/rivet-typography) | Font database, rasterization, glyph cache and atlas |
| [`rivet-image`](https://crates.io/crates/rivet-image) | Image decoding (PNG/JPEG/WebP/SVG/XPM) and icon themes |
| [`rivet-style`](https://crates.io/crates/rivet-style) | Style DSL compiled to flat O(1) tables |
| [`rivet-ui`](https://crates.io/crates/rivet-ui) | Widgets, layout, events, focus, `Application` |

## Usage

```rust
use rivet_gui::prelude::*;

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

Run the packaged example in a Wayland session:

```sh
cargo run -p rivet-gui --example hello
```

## Notes

- The crate is named `rivet-gui` because the bare `rivet` name on
  crates.io belongs to an unrelated project.
- Import paths use the crate names: `rivet_gui`, `rivet_base`, etc.
  The facade also re-exports the crates as modules
  (`rivet_gui::ui`, `rivet_gui::canvas`, ...) for fine-grained access.
- Linux + Wayland only, OpenGL 3.3 via EGL. Pre-1.0: the API should be
  expected to change.

## License

MIT OR Apache-2.0.
