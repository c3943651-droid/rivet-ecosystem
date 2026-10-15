# rivet-canvas — Definitive documentation

Rivet's **GPU-accelerated 2D graphics rendering** subsystem.

- **Version:** 2.0 (definitive)
- **Date:** 2026-10-11
- **Layer:** 3 (rendering)
- **Depends on:** `rivet-base` (and `rivet-typography`, `rivet-image` as texture sources). **Does NOT depend on `rivet-window`.**
- **Consumed by:** `rivet-ui`
- **Umbrella document:** `../RIVET.md`

---

## 1. Purpose

`rivet-canvas` is the drawing engine. It receives an **abstract surface** (an EGL/Wayland surface provided by another module), a scene to draw, and produces pixels. It draws **shapes, text, and images** with GPU acceleration (OpenGL). It does not create windows or handle events.

## 2. Architecture (render pipeline)

`GraphicsContext` orchestrates the frame in phases:

1. **Preparation** — validate the EGL context, collect the `RenderQueue`.
2. **Resource update:** `ShaderManager` (compiles and caches programs), `TextureManager` (uploads/updates textures), `GeometryAssembler` (tessellates primitives → vertices).
3. **Sorting:** sort the queue by layer/state to minimize state changes (reduce `glBind*` calls, *state thrashing*).
4. **Render:** execute batches by shader/texture; intermediate FBOs for effects.
5. **Presentation:** `PresentSynchronizer` performs `eglSwapBuffers` (vsync throttle) and recycles buffers.

Managers: `GraphicsContext`, `ShaderManager`, `TextureManager`, `GeometryAssembler`, `RenderQueue`, `PresentSynchronizer`, and `FrameStats` for telemetry.

## 3. Graphics backend (ADR-01)

- **Baseline: OpenGL 3.3 Core** (sufficient and universal on Linux).
- **Compute shaders: optional at runtime**, with **fallback** to FBO ping-pong. OpenGL 3.3 does **not** have compute: *compute shaders* require GL **4.3+** or `ARB_compute_shader`. v1 does not assume compute.
- **Vulkan: v2+ horizon** (not in v1).
- EGL context: create with **`EGL_CONTEXT_MAJOR_VERSION` / `EGL_CONTEXT_MINOR_VERSION`** (NOT `EGL_CONTEXT_OPENGL_VERSION`).
- VSync: **`eglSwapInterval(display, 1)`**.
- Effects (blur, shadows): in v1 with **FBO ping-pong** (separable gaussian). Real-time blur is feasible at 60 FPS with half-resolution FBOs; this is measured.

## 4. Public API (indicative)

```rust
pub struct Canvas;                 // drawing context for a surface
pub struct Texture;                // texture resource
pub struct Shader;
pub struct Mesh;                   // geometry
pub struct Color { r, g, b, a }    // premultiplied
pub struct Rect { x, y, w, h }
pub trait Drawable { fn draw(&self, canvas: &mut Canvas); }
pub trait TextureSource { fn texture(&self) -> &Texture; }
```

Flow: `begin_frame` → emit commands (rects, text, textures) → `render` → `present`.

**v0.3-fix:** `TextureManager::create/update_sub` clear the binding cache (direct `gl.bind_texture` desynced it: a new atlas page bound silently, so later "cache-hit" batches sampled the wrong texture — ghost/doubled text). `resize` resets `current_program` and texture bindings so the new projection is re-sent after window resizes (before: stale matrix → displaced/black text until the shader changed).

**v0.2 (clipping):** `RenderQueue::push_clip(Rect)`/`pop_clip()` maintain a stack of intersected clip rects; batches record the active clip and the renderer applies a GL scissor per batch (degenerate clips are skipped). `GraphicsContext::read_pixels_into` reads into a caller-owned buffer (zero-alloc presentation path).

## 5. Typography and text in canvas

- Consumes `rivet-typography` (rasterized glyphs as a **texture atlas**).
- **VRAM per ~32px glyph:** on the order of **KB per glyph** (mask/grayscale, not MB). An atlas of hundreds of glyphs occupies a modest amount; it is cached by (font, size, subpixel).
- Correction: any per-glyph MB figure is withdrawn.

## 6. Images

- Consumes `rivet-image` (decode PNG/JPEG/WebP/SVG → texture).
- Upload to GPU with `glTexSubImage2D`; uses asynchronous/batched *upload* where applicable.

## 7. Presentation and buffers

- Canvas does **not** own the surface: it receives a *surface abstraction* (from `rivet-window` or another source) and swaps buffers.
- With `wl_shm` (v1) the result is copied to the shared buffer (a copy exists, see `rivet-window` §5). With DMA-BUF (v1.x) it is exported with **`EGL_MESA_image_dma_buf_export`**.
- `PresentSynchronizer` paces with `eglSwapInterval` and the compositor's frame callback.

## 8. Telemetry

`FrameStats`: CPU time per phase, GPU time (if `EXT_disjoint_timer_query` is available), *draw call* counters, textures uploaded, glyphs rasterized. Basis of the **measurement protocol** (`../RIVET.md` §12).

## 9. Dependencies

- **Rust:** GL/EGL bindings and math (`glam`/similar).
- **System C (allowlist):** GL/EGL drivers (`libEGL`, `libGL`).
- No GUI C libraries.

## 10. Scope

**v1:** GL 3.3 Core, basic rects/shapes, text via atlas, textures, blending, 2D transforms, FBOs for composition and blur, `FrameStats`.

**v1.x (based on measurement):** DMA-BUF export, optional compute, MSAA, advanced layers and effects.

**Out of v1:** Vulkan, 3D, ray tracing, heavy post-processing.

## 11. Decisions and corrections applied

- DMA-BUF export = **`EGL_MESA_image_dma_buf_export`** (corrected).
- Compute is **optional** and **GL 4.3+ only**; v1 uses FBO ping-pong for blur (ADR-07).
- Context created with `EGL_CONTEXT_MAJOR_VERSION`/`MINOR_VERSION`.
- VSync with `eglSwapInterval`.
- VRAM per glyph in **KB** (not MB).
- The "0% CPU during render" claim is removed: rendering **costs** CPU/GPU and is measured (ADR-08).
- **canvas does not depend on window.**

## 12. Metrics (targets, to be measured — `../RIVET.md` §12)

Sustained FPS at rest/under animation, CPU ms per frame, GPU ms per frame, draw calls, VRAM peak, texture upload time, blur cost. Method: `FrameStats` + `perf` + GPU queries.
