# rivet — Definitive documentation (facade crate)

**Facade** crate of the Rivet ecosystem: re-exports the public API of all subsystems under a single name so that applications can write `use rivet::prelude::*;`.

- **Version:** 2.0 (definitive)
- **Date:** 2026-10-11
- **Depends on (and re-exports):** `rivet-foundation`, `rivet-window`, `rivet-canvas`, `rivet-typography`, `rivet-image`, `rivet-style`, `rivet-ui`.
- **Umbrella document:** `../RIVET.md`

---

## 1. Purpose

Rivet is not a monolithic framework: it consists of **independent crates** that can be used separately. `rivet` is a thin convenience layer that:

1. **Re-exports** the core types and traits.
2. Provides a **prelude** and an entry point (`Application`).
3. Hides dependency assembly (context, event loop, module registration).

It adds no logic of its own: it does **not** hide the crates, which remain directly accessible.

## 2. Structure

```rust
// crates/rivet/src/lib.rs
pub use rivet_foundation as foundation;
pub use rivet_window as window;
pub use rivet_canvas as canvas;
pub use rivet_typography as typography;
pub use rivet_image as image;
pub use rivet_style as style;
pub use rivet_ui as ui;

pub mod prelude {
    // App lifecycle.
    pub use rivet_ui::{Application, Ui};
    // Widget tree and registry.
    pub use rivet_ui::registry::WidgetRegistry;
    pub use rivet_ui::widget::{state, WidgetId, WidgetKind, WidgetNode};
    // Events and input.
    pub use rivet_ui::event::{EventDispatcher, UiInput};
    pub use rivet_ui::focus::FocusManager;
    pub use rivet_window::WindowEvent;
    // Drawing.
    pub use rivet_canvas::{Color, Rect, RenderQueue, TextureId};
    pub use rivet_ui::TextRenderer;
    // Styling.
    pub use rivet_style::{compile as compile_style, StyleTable, WidgetState, EXAMPLE_DSL};
    // Core (custom loops).
    pub use rivet_foundation::context::{Context, ContextBuilder};
    pub use rivet_foundation::event_loop::EventLoop;
    // Images and icons.
    pub use rivet_image::{IconTheme, ImageData};
    // Typography.
    pub use rivet_typography::FontDb;
}
```

The widgets themselves (panels, labels, buttons) are created through the
registry on a [`Ui`] instance; they are not exported as constructor types
in the prelude.

## 3. Usage

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

The complete, runnable version is `crates/rivet/examples/hello.rs`
(`cargo run -p rivet --example hello`).

## 4. Facade or separate crates?

- **Use the facade** if you build a complete application.
- **Use the individual crates** if you only need, e.g., the slab (`rivet-foundation`) or the image decoder (`rivet-image`) without pulling in the UI.

## 5. Dependencies

- It depends on the entire ecosystem; **no layer depends on `rivet`** (acyclic graph). `rivet` sits at the top.

## 6. Scope

**v1:** re-exports + prelude + `Application`.

**Out:** does not include tooling (`rivet-style` CLI) or seed applications (panel, launcher, AnimaEngine).

## 7. Applied decisions

- **Thin** facade: preserves the ecosystem's modularity and the "build vs. borrow" principle.
- The name `rivet` matches the project name; it serves as the standard entry point.

## 8. Metrics

The facade cost must be **~0**: it is re-exports only, with no
initialization code of its own. No runtime measurement is published for
this crate; the requirement is structural (the code adds no executable
code beyond `pub use` and the `VERSION` constant).
