# Rivet

**Conjunto de librerías en Rust para crear software gráfico (GUI)**, en el mismo rol que GTK y Qt. No es un escritorio ni un gestor de ventanas: es el toolkit — las librerías sobre las que se construye software.

- **Rust 100% nativo** en la capa GUI (sin GTK, Qt, Cairo, Pango ni FreeType).
- **Una responsabilidad por crate**; todo lo demás es composición.
- **Medido, no prometido**: ninguna cifra de rendimiento se publica sin medición real ([protocolo](docs/RIVET.md#14-métricas-y-medición)).

Documentación autoritativa: [`docs/RIVET.md`](docs/RIVET.md).

## Crates

| Crate | Responsabilidad |
|---|---|
| `rivet-foundation` | Slab con generaciones, bucle de eventos (epoll), config TOML, errores, utilidades |
| `rivet-window` | Cliente Wayland: `xdg_shell`, `layer_shell`, popups, subsurfaces, entrada, buffers, HiDPI |
| `rivet-canvas` | Render 2D por GPU (OpenGL 3.3 Core + EGL), shaders, geometría, efectos |
| `rivet-typography` | Parseo TTF/OTF, rasterizado, caché de glifos, métricas |
| `rivet-image` | Carga/decodificación de imágenes (PNG/JPEG/SVG/WebP), iconos y temas |
| `rivet-style` | Estilos precompilados en tablas planas, estados, paletas/temas |
| `rivet-ui` | Widgets por composición, layout, eventos, foco |
| `rivet` | Fachada que re-exporta el ecosistema |

## Estado

Las 8 crates están implementadas y verificadas (fmt + clippy `-D warnings`
+ tests en cada push a `develop`). Incluye: rasterizado de glifos con
cobertura analítica y subpíxel, kerning `kern`+GPOS, atlas con subidas
incrementales, temas de iconos freedesktop, DSL de estilos compilado a
tablas O(1), gestos/DnD/clipboard, widgets con hit-test/foco/layout y
`Application` end-to-end (Wayland + GL 3.3 + `wl_shm`).

Ejemplo mínimo (`cargo run -p rivet --example hello`, requiere sesión
Wayland):

```rust
use rivet::prelude::*;

let mut app = Application::new("Hola Rivet", 360, 200)?;
let ui = app.ui_mut();
let root = ui.registry.create_panel(true);
ui.registry.set_root(root)?;
let title = ui.registry.create_label("Hola, Rivet");
let close = ui.registry.create_button("Salir", |ui| ui.quit());
ui.registry.add_child(root, title)?;
ui.registry.add_child(root, close)?;
app.run()?;
```

## Licencia

MIT OR Apache-2.0 — ver [`LICENSE-MIT`](LICENSE-MIT) y [`LICENSE-APACHE`](LICENSE-APACHE).
