# rivet-canvas — Documentación definitiva

Subsistema de **renderizado gráfico 2D por GPU** de Rivet.

- **Versión:** 2.0 (definitiva)
- **Fecha:** 2026-10-11
- **Capa:** 3 (renderizado)
- **Depende de:** `rivet-foundation` (y `rivet-typography`, `rivet-image` como fuentes de textura). **NO depende de `rivet-window`.**
- **Consumido por:** `rivet-ui`
- **Reemplaza a:** parte de GTK/Cairo/Skia (la capa de dibujo).
- **Documento paraguas:** `../RIVET.md`

---

## 1. Propósito

`rivet-canvas` es el motor de dibujo. Recibe una **superficie abstracta** (EGL/Wayland surface que le entrega otro módulo), una escena a dibujar y produce píxeles. Dibuja **formas, texto e imágenes** con aceleración por GPU (OpenGL). No crea ventanas ni gestiona eventos.

## 2. Arquitectura (pipeline de render)

`GraphicsContext` orquesta el frame en fases:

1. **Preparación** — validar contexto EGL, recoger `RenderQueue`.
2. **Actualización de recursos:** `ShaderManager` (compila cachea programas), `TextureManager` (sube/actualiza texturas), `GeometryAssembler` (tesela primitivas → vértices).
3. **Ordenación:** ordenar la cola por capa/estado para minimizar cambios de estado (reducir `glBind*`, *state thrashing*).
4. **Render:** ejecutar lotes por shader/textura; FBO intermedios para efectos.
5. **Presentación:** `PresentSynchronizer` hace `eglSwapBuffers` (throttle vsync) y recicla buffers.

Gestores: `GraphicsContext`, `ShaderManager`, `TextureManager`, `GeometryAssembler`, `RenderQueue`, `PresentSynchronizer`, y `FrameStats` para telemetría.

## 3. Backend gráfico (ADR-01)

- **Baseline: OpenGL 3.3 Core** (suficiente y universal en Linux).
- **Compute shaders: opcional en runtime**, con **fallback** a FBO ping-pong. OpenGL 3.3 **NO tiene compute**: los *shaders de cómputo* requieren GL **4.3+** o `ARB_compute_shader`. En v1 no se asume compute.
- **Vulkan: horizonte v2+** (no en v1).
- Contexto EGL: crear con **`EGL_CONTEXT_MAJOR_VERSION` / `EGL_CONTEXT_MINOR_VERSION`** (NO `EGL_CONTEXT_OPENGL_VERSION`).
- VSync: **`eglSwapInterval(display, 1)`**.
- Efectos (blur, sombras): en v1 con **FBO ping-pong** (gaussiano separable). Blur "en tiempo real" es factible a 60 FPS con FBOs a media resolución; se mide.

## 4. API pública (orientativa)

```rust
pub struct Canvas;                 // contexto de dibujo de una superficie
pub struct Texture;                // recurso de textura
pub struct Shader;
pub struct Mesh;                   // geometría
pub struct Color { r, g, b, a }    // premultiplicado
pub struct Rect { x, y, w, h }
pub trait Drawable { fn draw(&self, canvas: &mut Canvas); }
pub trait TextureSource { fn texture(&self) -> &Texture; }
```

Flujo: `begin_frame` → emitir comandos (rects, texto, texturas) → `render` → `present`.

## 5. Tipografía y texto en canvas

- Consume `rivet-typography` (glifos rasterizados como **atlas de textura**).
- **VRAM por glifo ~32px:** unos **KB por glifo** (máscara/gris, no MB). Un atlas de cientos de glifos ocupa un margen modesto; se cachea por (fuente, tamaño, subpíxel).
- Corrección: se retira cualquier cifra de MB por glifo.

## 6. Imágenes

- Consume `rivet-image` (decode PNG/JPEG/WebP/SVG → textura).
- Subida a GPU con `glTexSubImage2D`; gestiona *upload* asíncrono/por lotes cuando aplica.

## 7. Presentación y buffers

- Canvas **no posee** la superficie: recibe un *surface abstraction* (de `rivet-window` u otro origen) e intercambia buffers.
- Con `wl_shm` (v1) el resultado se copia al buffer compartido (hay copia, ver `rivet-window` §5). Con DMA-BUF (v1.x) se exporta con **`EGL_MESA_image_dma_buf_export`**.
- `PresentSynchronizer` pacea con `eglSwapInterval` y el frame callback del compositor.

## 8. Telemetría

`FrameStats`: tiempo de CPU por fase, tiempo de GPU (si hay `EXT_disjoint_timer_query`), contadores de *draw calls*, texturas subidas, glifos rasterizados. Base del **protocolo de medición** (`../RIVET.md` §14).

## 9. Dependencias

- **Rust:** binding GL/EGL y math (`glam`/similar).
- **C de sistema (allowlist):** driver GL/EGL (`libEGL`, `libGL`).
- **Sin GTK/Qt.**

## 10. Alcance

**v1:** GL 3.3 Core, rects/formas básicas, texto por atlas, texturas, blending, transformaciones 2D, FBO para composición y blur, `FrameStats`.

**v1.x (por medición):** DMA-BUF export, compute opcional, MSAA, capas y efectos avanzados.

**Fuera de v1:** Vulkan, 3D, ray tracing, post-proceso pesado.

## 11. Decisiones y correcciones aplicadas

- Export DMA-BUF = **`EGL_MESA_image_dma_buf_export`** (corregido).
- Compute **opcional** y **solo GL 4.3+**; v1 usa FBO ping-pong para blur (ADR-07).
- Contexto con `EGL_CONTEXT_MAJOR_VERSION`/`MINOR_VERSION`.
- VSync con `eglSwapInterval`.
- VRAM por glifo en **KB** (no MB).
- Se elimina la promesa "0% de CPU durante el render": el render **cuesta** CPU/GPU y se mide (ADR-08).
- **canvas no depende de window.**

## 12. Métricas (objetivos, a medir — `../RIVET.md` §14)

FPS sostenidos en reposo/anima, ms de CPU por frame, ms de GPU por frame, draw calls, pico de VRAM, tiempo de subida de textura, coste del blur. Método: `FrameStats` + `perf` + queries de GPU.
