# rivet — Documentación definitiva (crate fachada)

Crate **fachada** del ecosistema Rivet: reexporta la API pública de todos los subsistemas bajo un único nombre para que las apps escriban `use rivet::prelude::*;`.

- **Versión:** 2.0 (definitiva)
- **Fecha:** 2026-10-11
- **Depende de (y reexporta):** `rivet-foundation`, `rivet-window`, `rivet-canvas`, `rivet-typography`, `rivet-image`, `rivet-style`, `rivet-ui`.
- **Documento paraguas:** `../RIVET.md`

---

## 1. Propósito

Rivet no es un framework monolítico: son **crates independientes** que pueden usarse por separado. `rivet` es una capa fina de conveniencia que:

1. **Reexporta** los tipos y traits principales.
2. Ofrece un **prelude** y un punto de entrada (`Application`).
3. Oculta el ensamblado de dependencias (contexto, bucle de eventos, registro de módulos).

No añade lógica propia: **no esconde las crates**, que siguen siendo accesibles directamente.

## 2. Estructura

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

## 3. Uso

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

## 4. ¿Fachada o crates por separado?

- **Usa la fachada** si construyes una app completa.
- **Usa las crates sueltas** si solo necesitas, p. ej., el slab (`rivet-foundation`) o el decoder de imágenes (`rivet-image`) sin arrastrar la UI.

## 5. Dependencias

- Depende de todo el ecosistema; **ninguna capa depende de `rivet`** (grafo acíclico). `rivet` está en la cima.

## 6. Alcance

**v1:** reexportaciones + prelude + `Application`.

**Fuera:** no incluye herramientas (`rivet-style` CLI), ni apps semilla (panel, lanzador, AnimaEngine).

## 7. Decisiones aplicadas

- Fachada **fina**: preserva la modularidad y el "build vs. borrow" del ecosistema.
- El nombre `rivet` coincide con el del proyecto; sirve de puerta de entrada estándar.

## 8. Métricas

El coste de la fachada debe ser **~0** (solo reexportaciones; sin runtime añadido). Se verifica que no introduce código de inicialización propio.
