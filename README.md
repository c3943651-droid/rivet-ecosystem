# Rivet

Toolkit de interfaces gráficas para Linux, escrito en Rust de arriba a abajo.
Sirve para construir aplicaciones de escritorio: ventanas, botones, texto,
imágenes, estilos y el bucle de eventos que los conecta.

No es un escritorio ni un gestor de ventanas. Es la capa de librerías sobre
la que otros construyen software.

## Estado

Versión 0.1.0. El ecosistema está completo y funcional en su alcance
inicial, pero es un proyecto joven:

- Solo **Linux con Wayland**. No hay soporte para X11 ni otros sistemas.
- El catálogo de widgets es pequeño (contenedores, botones, etiquetas,
  imágenes, campos de texto). No hay aún scroll, menús ni texto multilínea.
- Todavía no está publicado en crates.io: se consume como dependencia de
  git.

Cada cambio pasa por `cargo fmt`, `clippy -D warnings` y la suite de
tests antes de llegar a `develop`.

## Requisitos

- Linux con sesión Wayland (compositor moderno: KWin, sway, GNOME…).
- Mesa con EGL (GPU hardware o llvmpipe).

## Uso

Añade la dependencia por git:

```toml
[dependencies]
rivet = { git = "https://github.com/c3943651-droid/rivet-ecosystem", branch = "develop" }
```

Una ventana con texto y un botón:

```rust
use rivet::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Application::new("Hola Rivet", 360, 200)?;

    {
        let ui = app.ui_mut();
        let root = ui.registry.create_panel(true);
        ui.registry.set_root(root)?;

        let title = ui.registry.create_label("Hola, Rivet");
        let close = ui.registry.create_button("Salir", |ui| ui.quit());

        ui.registry.add_child(root, title)?;
        ui.registry.add_child(root, close)?;
    }

    app.run()?;
    Ok(())
}
```

El ejemplo completo está en `crates/rivet/examples/hello.rs` y se ejecuta
con `cargo run -p rivet --example hello`.

## Crates

Cada crate tiene una responsabilidad y puede usarse por separado; `rivet`
es solo una fachada que las re-exporta juntas.

| Crate | Qué hace |
|---|---|
| `rivet-foundation` | Slab con generaciones, bucle de eventos (epoll), configuración TOML, logging |
| `rivet-window` | Cliente Wayland: ventanas, entradas (teclado/puntero/touch), clipboard, arrastre, HiDPI |
| `rivet-canvas` | Dibujo 2D por GPU (OpenGL 3.3 + EGL): primitivas, batching, texturas, efectos |
| `rivet-typography` | Fuentes TTF/OTF: rasterizado con subpíxel, kerning, caché y atlas de glifos |
| `rivet-image` | Decodificación de imágenes (PNG/JPEG/WebP/SVG/XPM) y temas de iconos |
| `rivet-style` | DSL de estilos compilado a tablas planas, temas, estados de widget |
| `rivet-ui` | Widgets, layout, hit-testing, foco y `Application` |
| `rivet` | Fachada con prelude |

## Documentación

- [`docs/RIVET.md`](docs/RIVET.md) — decisiones de diseño, arquitectura y
  registro de métricas del ecosistema.
- `docs/<crate>.md` — documentación específica de cada crate.
- [`crates/rivet/examples/hello.rs`](crates/rivet/examples/hello.rs) — ejemplo mínimo.

## Licencia

MIT OR Apache-2.0 — ver [`LICENSE-MIT`](LICENSE-MIT) y
[`LICENSE-APACHE`](LICENSE-APACHE). La fuente Inter Regular va incluida
bajo su licencia OFL (ver `crates/rivet-typography/assets/`).
