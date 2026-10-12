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

## Licencia

MIT OR Apache-2.0 — ver [`LICENSE-MIT`](LICENSE-MIT) y [`LICENSE-APACHE`](LICENSE-APACHE).
