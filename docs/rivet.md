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
    pub use rivet_ui::Application;
    pub use rivet_ui::{Button, Label, TextInput, /* ... */};
    pub use rivet_canvas::{Color, Rect};
    pub use rivet_foundation::Context;
}
```

## 3. Usage

```rust
use rivet::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new()
        .window("Mi App", 800, 600)
        .ui(|ui| {
            ui.add(Label::new("Hola, Rivet"));
            if ui.button("Salir").clicked() { /* ... */ }
        })
        .run()
}
```

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

The facade cost must be **~0** (re-exports only; no added runtime). It is verified that it introduces no initialization code of its own.
