# rivet-ui

Widget layer of the [Rivet](https://github.com/c3943651-droid/rivet-ecosystem)
GUI toolkit: a retained widget tree with layout, hit-testing, focus,
event routing, and styled rendering — plus `Application`, the
end-to-end loop that wires Wayland, OpenGL, and the widget tree
together.

## What it provides

- **Widget registry** (`WidgetRegistry`): composition over inheritance.
  A common `WidgetNode` (children, flags, geometry) plus one slab per
  kind for the payload — no enum padding, no `dyn Any`.
- **Widgets**: boxes/panels (containers), labels, buttons with
  callbacks, images, and single-line text inputs with change callbacks.
- **Layout**: measure and position passes (box model, per-kind
  measure via `rivet-typography`).
- **Events**: hit-testing, capture/bubble routing, pointer and key
  translation from `rivet-window` events.
- **Focus**: click and Tab navigation with the standard order.
- **Rendering**: per-widget style lookup (`rivet-style`) and ordered
  command emission to `rivet-canvas`, with the glyph atlas via
  `TextRenderer`.
- **`Application`**: connects Wayland, opens a window, drives the
  event loop, redraws, and presents.

## Usage

```rust
use rivet_ui::Application;

let mut app = Application::new("Demo", 400, 300)?;

{
    let ui = app.ui_mut();
    let root = ui.registry.create_panel(true);
    ui.registry.set_root(root)?;

    let label = ui.registry.create_label("Hello, Rivet");
    let close = ui.registry.create_button("Quit", |ui| ui.quit());
    ui.registry.add_child(root, label)?;
    ui.registry.add_child(root, close)?;
}

app.run()?;
```

For a complete runnable file see
[`examples/hello.rs`](../rivet-gui/examples/hello.rs) in the facade
crate, or use the [`rivet-gui`](https://crates.io/crates/rivet-gui)
facade.

## Notes

- Depends on `rivet-base`, `rivet-window`, `rivet-canvas`,
  `rivet-typography`, `rivet-image`, and `rivet-style`.
- Widget IDs are 64-bit (index + generation), consistent with the
  `rivet-base` slab.
- v1 does not include scrolling containers, menus, or multiline text;
  the backlog lives in [`docs/RIVET.md` §10](../../docs/RIVET.md).

## License

MIT OR Apache-2.0.
