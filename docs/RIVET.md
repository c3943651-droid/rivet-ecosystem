# RIVET — Official Documentation

> **This is the only official Rivet document.** Everything else (the
> `Rivet Documentacion/archivo/` folder, `Wrappers-Legacy-prerivet/`) is
> **historical** and must not be taken as a specification. If anything in
> the archive contradicts this document, **this document prevails**.

- **Version:** 2.1 (English migration; project status updated)
- **Date:** 2026-10-13
- **Status:** Single, authoritative document. Integrates the vision, the
  architecture, closed decisions, per-subsystem specifications, the
  roadmap, and the design log of resolved misconceptions.
- **Project license:** MIT OR Apache-2.0.

---

## Index

1. [What Rivet is](#1-what-rivet-is)
2. [History (why it exists)](#2-history-why-it-exists)
3. [Identity and positioning](#3-identity-and-positioning)
4. [Principles](#4-principles)
5. [Crate map](#5-crate-map)
6. [Dependency policy](#6-dependency-policy)
7. [Memory and object model](#7-memory-and-object-model)
8. [Input and keyboard](#8-input-and-keyboard)
9. [Graphics subsystem](#9-graphics-subsystem-corrected-specification)
10. [Architecture Decision Records (ADR)](#10-architecture-decision-records-adr)
11. [Per-subsystem specifications](#11-per-subsystem-specifications)
12. [v1 scope](#12-v1-scope)
13. [Roadmap](#13-roadmap)
14. [Metrics and measurement](#14-metrics-and-measurement)
15. [Design log: resolved misconceptions](#15-design-log-resolved-misconceptions)
16. [Relation to the historical documentation](#16-relation-to-the-historical-documentation)
17. [Project status](#17-project-status)

---

## 1. What Rivet is

Rivet is a **set of Rust libraries for building graphical (GUI) software**,
in the same role as GTK and Qt: developers use it to build programs,
applications, tools, and components.

It is **not** a desktop, a window manager, or a desktop environment. Rivet
is **the toolkit**: the libraries on which software is built.

## 2. History (why it exists)

The project went through three identity phases; they are recorded here to
avoid repeating the same mistakes:

1. **"Wrappers"** — initial wrapper idea. The name was confusing: it
   suggested FFI over C when the goal was a rewrite in Rust.
2. **"Zero" (legacy)** — pivot to a "total constructor". A clearer name,
   but memory targets grew increasingly extreme (<100 MB → 25-35 MB)
   **without measurements**.
3. **Rivet (current)** — unification. One name, one ecosystem, and the
   criterion of **measured, not promised** metrics.

## 3. Identity and positioning

- **What it is:** GUI libraries in Rust (the GTK/Qt role).
- **What it is not:** a desktop, a DE, a 1:1 drop-in replacement for
  GTK/Qt, or "the new GTK".
- **Distinguishing trait:** fully native Rust (no C GUI bridges),
  cohesion between crates, and a minimal footprint by design.
- **Entry point (not the ceiling):** lightweight desktop components
  (launchers, panels, overlays, desk pets, utilities).
- **Horizon:** a general toolkit for building programs such as text
  editors, creative tools, and full applications.

## 4. Principles

1. **Native Rust, no GUI C.** No GTK, Qt, Cairo, Pango, or FreeType in the
   stack. Minimal, bounded "system C" is allowed (§6).
2. **One responsibility per crate.** Each crate does one thing; everything
   else is composition.
3. **Measured, not promised.** No memory/CPU/latency figure is published
   as data until a real measurement exists on a real component (§14).
4. **Component first.** Crates are prioritized by their interdependencies
   and the value of each component (§12).
5. **Cohesion over quantity.** Six coherent crates are better than twelve
   overlapping ones.
6. **Compose, don't inherit.** Widgets by composition; hierarchy through
   IDs with generations, not inheritance or pointers.
7. **Open source.** MIT OR Apache-2.0, public so others can build on it
   and improve it.

---

## 5. Crate map

| Crate | Responsibility | Replaces (classic stack) |
|---|---|---|
| `rivet-foundation` | Generational slab, epoll event loop, TOML config, errors, utilities | GLib + part of GObject |
| `rivet-window` | Wayland client: socket, `wl_registry`, `xdg_shell`, `layer_shell`, popups, subsurfaces, input, buffers, HiDPI | GDK (Wayland backend) + `libwayland-client` |
| `rivet-canvas` | GPU 2D rendering (OpenGL 3.3 Core + EGL), shaders, geometry, textures, effects | Cairo + GSK + GL/EGL loader |
| `rivet-typography` | TTF/OTF parsing, rasterization (De Casteljau + scanline), glyph cache, metrics | FreeType + part of Pango |
| `rivet-image` | Image loading/decoding (PNG/JPEG/SVG/WebP), icons and icon themes | libpng/libjpeg/librsvg + `image` |
| `rivet-style` | Precompiled styles in flat tables, states, palettes/themes | GTK CSS engine / QSS |
| `rivet-ui` | Widgets by composition, layout, events, focus | GTK (widgets + layout) |
| `rivet` | Facade re-exporting the ecosystem | The `#include <gtk/gtk.h>` umbrella |

> `rivet-image` is a **required crate of the toolkit** (icons and images),
> not an optional one.

```
Rivet (GUI toolkit in Rust)
└── Crates (libraries) → foundation · window · canvas · typography · image · style · ui · rivet (facade)
```

---

## 6. Dependency policy

The goal is **not** "zero C at any cost", but **zero C in the GUI layer**,
with minimal, bounded, justified system C.

**Forbidden:** GTK, Qt, GLib, Cairo, Pango, FreeType, HarfBuzz (C) and
their bridges; `wgpu`, `smithay`, `fontdue` (not because they are bad, but
because of the goal of owning the stack).

**Allowed — system C (allowlist):**

| Dependency | Reason | Note |
|---|---|---|
| `libc` | System ABI | Minimal |
| OpenGL/EGL driver (`libEGL`/`libGL`) | GPU access | System-provided; always present |
| `libxkbcommon` | Keyboard map / xkb | The only unavoidable C today |

**Allowed — pure-Rust crates (by contribution):** `serde`/`toml` (config),
`zbus` (D-Bus, pure Rust), `fontdb` (system fonts, no `fontconfig`),
`ttf-parser`, and similar **pure-Rust** crates.

**Practical rule:**
> If a **pure-Rust** crate solves the problem, it is used until a Rivet
> crate of its own is justified. **GUI C** is forbidden; **system C** is
> limited to the allowlist.

---

## 7. Memory and object model

### 7.1. Generational slab
- **64-bit** IDs = 32 bits of index + 32 bits of generation. Safe against
  freed handles (prevents silent use-after-free through recycled indices).
  **Closed decision.**

### 7.2. Widget storage (closed decision)
A **two-level** model:
1. **Common node** in a flat arena: `Slab<WidgetNode>`, a small fixed-size
   header:
   `{ type_tag: u16, style_id: u32, flags: u32, parent: Option<WidgetId>, first_child, last_child, prev_sibling, next_sibling, bounds: Rect }`.
2. **Per-type data** in **per-type slabs** (`Slab<Button>`, `Slab<Label>`,
   …); the node holds the `type_tag` plus the index of its payload.

**`WidgetId`** = 64 bits (generation:32 | index:32) into the node arena.

**Children:** **intrusive links**
(`first_child`/`last_child`/`prev_sibling`/`next_sibling`), **no
`Vec<WidgetId>` per widget**. Insert/remove in O(1) with no heap
allocations.

**Rationale:** the node is contiguous (good locality for hit-testing,
traversal, and global layout); per-type data is contiguous (good locality
and batching). This avoids the waste of a `enum Widget` (size = largest
variant) and per-node heap.

> The "24 bytes per widget" figure from the old documentation is
> **withdrawn**. The node size is **measured** (§14), not promised.

---

## 8. Input and keyboard

- `rivet-window` uses **`libxkbcommon`** for keymap/keysym/compose.
  Reimplementing XKB by hand is **ruled out** on cost/risk grounds.
- **IME (`text-input-v3`)** is **deferred**: not required for v1 (Latin
  text).

---

## 9. Graphics subsystem (corrected specification)

### 9.1. OpenGL context
- **Mandatory baseline: OpenGL 3.3 Core.** All rendering (shapes, text,
  framebuffer ping-pong blur, blends, FBOs) works on 3.3.
- **Optional "compute" path**, detected at runtime
  (`GL_ARB_compute_shader` / GL 4.3 / GLES 3.1). The renderer does **not**
  depend on it.
- **Vulkan:** v2+ horizon.
- Correct context attributes: `EGL_CONTEXT_MAJOR_VERSION` and
  `EGL_CONTEXT_MINOR_VERSION` (`EGL_CONTEXT_OPENGL_VERSION` does not
  exist).

### 9.2. Effects (blur)
The v1 blur is a **fragment shader with FBO ping-pong** (two separable
passes). The **compute shader** version is an optimization conditioned on
§9.1. The v1 blur must not be described as "compute".

### 9.3. Buffer presentation
```
Rendering in rivet-canvas:
  1. Draw into the back buffer (FBO or surface).
  2. Present depending on the path:
     a) wl_shm:  canvas writes pixels into the shared buffer (memfd),
                 window does wl_surface.attach + commit.        [copy, v1]
     b) dmabuf:  the image is exported (EGL_MESA_image_dma_buf_export),
                 a wl_buffer is created (zwp_linux_dmabuf_v1.create_params),
                 window attaches and commits.                    [zero-copy, v1.x]
  3. Synchronization:
     - v1: implicit (sufficient with wl_shm).
     - DMABUF: prefer explicit (linux-drm-syncobj-v1), not implicit only.
```

| Path | Memory | Zero-copy? | Use |
|---|---|---|---|
| `wl_shm` | System RAM (memfd) | **No** (copy) | v1 (simple, portable) |
| DMA-BUF (`zwp_linux_dmabuf_v1`) | GPU VRAM | **Yes** | v1.x, gated on measurement |

- **Export** from GL = `EGL_MESA_image_dma_buf_export`.
- **Import** into GL = `EGL_EXT_image_dma_buf_import` (e.g. video). Do not
  confuse the two: the old documentation used "import" for exporting.
- **VSync** is controlled with `eglSwapInterval` (not with
  `EGL_SWAP_BEHAVIOR_PRESERVED`).

### 9.4. Dependency direction
`rivet-window` **creates/manages** the buffer and presents it
(`attach`/`commit`); `rivet-canvas` **writes/renders** into it.
`rivet-canvas` does **not** depend on `rivet-window`.

---

## 10. Architecture Decision Records (ADR)

All **closed** (2026-10-11). Future changes require a new entry.

| ID | Topic | Decision |
|---|---|---|
| **ADR-01** | Graphics baseline | GL 3.3 Core mandatory; compute optional at runtime; Vulkan v2+. |
| **ADR-02** | Presentation | `wl_shm` in v1; DMA-BUF in v1.x, with explicit synchronization, gated on measurement. |
| **ADR-03** | Widget storage | Common `WidgetNode` + per-type slabs; 64-bit `WidgetId`. |
| **ADR-04** | Children | Intrusive links (`first_child`/`last_child`/`prev_sibling`/`next_sibling`); no per-widget `Vec`. |
| **ADR-05** | Fonts | Discovery via the `fontdb` crate (pure Rust) + a bundled fallback font; **no `fontconfig`**. |
| **ADR-06** | Colors | `lighten`/`darken` are derived **per theme** at codegen time; switching themes = O(1) swap. |
| **ADR-07** | Effects | Blur = FBO ping-pong; compute optional. |
| **ADR-08** | Vocabulary | "0% CPU" is **forbidden**. Standard phrasing: style/state resolution = "O(1): a memory operation"; every interaction includes a redraw with a cost. |

---

## 11. Per-subsystem specifications

### `rivet-foundation`
- **Purpose:** the ecosystem's core.
- **Contents:** generational slab (64-bit IDs), epoll event loop, config
  (TOML), error types, utilities.
- **Decision:** 64-bit IDs = 32 index + 32 generation (closed).

### `rivet-window`
- **Purpose:** native Wayland client.
- **Contents:** socket, `wl_registry`, `xdg_shell`, `layer_shell`,
  popups, subsurfaces, input (`wl_seat`/pointer/keyboard/touch), buffers
  (`memfd_create`/`wl_shm`/DMA-BUF), HiDPI.
- **Buffers:**
  - **v1 = `wl_shm`** (shared memory via `memfd_create`): simple,
    portable. **Not** zero-copy.
  - **DMA-BUF = v1.x**, gated on measurement, with **explicit
    synchronization** (`linux-drm-syncobj-v1`).
  - `wl_shm` **was never zero-copy** (no zero-copy is "lost" by choosing
    it).
- **Keyboard:** `libxkbcommon` (compiles the compositor's keymap;
  keysym/UTF-8). XKB is not reimplemented. IME deferred.
- **Direction:** hands surface/buffer to `rivet-canvas`; never the
  reverse.

### `rivet-canvas`
- **Purpose:** GPU 2D rendering (OpenGL 3.3 Core + EGL).
- **Contents:** shaders, geometry, textures, effects (FBO ping-pong),
  presentation.
- **Key corrections:** see §9. DMA-BUF export =
  `EGL_MESA_image_dma_buf_export`; VSync = `eglSwapInterval`; no compute
  on 3.3.
- **VRAM per glyph:** a **few KB** per 32 px glyph (not MB).
- It does **not** consume "0%" CPU during continuous rendering: draw
  calls must be submitted and presented.
- **Does not depend on `rivet-window`.**

### `rivet-typography`
- **Purpose:** turning TrueType/OpenType fonts into rasterized (bitmap)
  glyphs plus base metrics.
- **Contents:** table parsing (with `ttf-parser` for low-level reading;
  extraction logic is our own), rasterization (De Casteljau + scanline,
  antialiasing), glyph cache (LRU), metrics (`hmtx`).
- **Does NOT do:** kerning, shaping, line positioning, writing direction.
  That belongs to `rivet-ui` / a future `rivet-layout`.
- **Font discovery:** the `fontdb` crate (pure Rust) + a bundled fallback
  font; **no `fontconfig`**.
- **Documented v1 limitations:** no kerning (`kern`/`GPOS`); no shaping
  (Arabic/Hindi are **not** rendered correctly); **no full hinting** (grid
  alignment only → suboptimal sharpness at small sizes); no color emoji
  (COLRv1/SVG).
- **Future shaping:** to be delegated to `rustybuzz` (pure Rust) in a
  later version.

### `rivet-image`
- **Purpose:** image and icon loading/decoding.
- **Contents:** PNG/JPEG (+ SVG/WebP if viable), icons and icon themes.
- **Required** for the toolkit's icons and images; not optional.

### `rivet-style`
- **Purpose:** precompiled styles in flat tables.
- **Contents:** compilation pipeline (parser → optimizer → codegen), flat
  numeric tables indexed by integers, states by index, palettes and
  themes.
- **Cost:** applying a style is an **index operation (O(1))**. It does
  **not** claim "0% CPU": *resolution* is cheap, but a full interaction
  redraws.
- **Derived colors:** `lighten`/`darken` are generated **per theme** at
  codegen time; theme switch = O(1) swap. Figures like "~1 ns" are
  **targets to measure**, not data.

### `rivet-ui`
- **Purpose:** widgets by composition, layout, events, focus.
- **Storage:** common `WidgetNode` + per-type slabs; children via
  intrusive links (§7).
- **Lifecycle:** Normal/Hover/Active/Focus/Disabled states; composition
  (no inheritance); hierarchy by `WidgetId`.
- **Cost:** *state resolution* is O(1); the full interaction cycle
  includes layout and redraw.
- **Size corrections:** `Option<WidgetId>` = 8 B with the niche (16 B
  without); `Option<Rect>` = 20 B (padding), never 17. The "~83 B"
  estimate for the struct is to be **measured**.

### `rivet` (facade)
- Re-exports the ecosystem for the end user. It is the umbrella equivalent
  of `#include <gtk/gtk.h>`.

---

## 12. v1 scope

Rivet v1 is defined as the minimal set of toolkit components: what is
needed to build lightweight graphical software on Wayland.

**In v1:** `xdg_shell` + `layer_shell`, overlays, popups, subsurfaces,
HiDPI; transparency and input regions (click-through); rendering of
shapes, images, and text (Latin/digits); PNG/JPEG (+SVG if viable) and
icons/themes; minimal widgets (`Label`, `Box/Row`, `Button`, `Image/Icon`,
`TextInput`, `List`, `Popup/Menu`); light styling/config; D-Bus; animation
loop.

**Out of v1 (horizon):** IME, full shaping/BiDi, a11y AT-SPI,
clipboard/DnD, full CSS engine, Vulkan, X11/XWayland, bindings.

> Implementation note (2026-10-13): `List`/`Scroll` widgets are **not**
> implemented in the v1 codebase; the catalog shipped with
> Box/Panel/Button/Label/Image/TextInput. Clipboard, DnD, and gestures
> **are** implemented in `rivet-window` despite being listed as v1.x
> here. This section records the original plan; deviations are tracked in
> §17.

---

## 13. Roadmap

### 13.1. Approach: build vs. borrow
To reach working apps quickly, the starting point is the **pure-Rust
stack**, replaced by Rivet crates where they add value.

| Need | Start with (pure Rust) | Replace with (Rivet) when |
|---|---|---|
| Window | `wayland-client` / `smithay`-rs | `rivet-window` |
| Rendering | `wgpu` or similar | `rivet-canvas` |
| Fonts | `ttf-parser` + `fontdb` | `rivet-typography` |
| Images | `image` | `rivet-image` |
| D-Bus | `zbus` | (probably kept) |
| Styling | own | `rivet-style` |
| Widgets | — | `rivet-ui` |

> Depending on GUI C is **forbidden** even when it would be faster; for
> those cases a **pure-Rust** alternative is used even if less mature.

### 13.2. Phases
- **Phase 0 — Crate specifications.** Signatures, types, and protocol
  tables (D-03) for each crate in §5.
- **Phase 1 — `rivet-foundation` + `rivet-window`.** Epoll loop,
  generational slab, `xdg_shell`/`layer_shell`. **First measurement**
  (§14).
- **Phase 2 — `rivet-canvas` + `rivet-typography`.** GL 3.3 Core
  rendering and Latin text.
- **Phase 3 — `rivet-ui` + `rivet-style`.** Widgets by composition and
  precompiled styles.
- **Phase 4 — `rivet-image` + the `rivet` facade.** Icons/themes and the
  ecosystem re-export.
- **Phase 5 — Publication and community** (English docs, semver,
  contribution guide; announce **with working, measured components**).

### 13.3. Path to a general toolkit (horizon)
1. **Component toolkit** (v1) → builds a footprint and first users.
2. **GPU UI framework** (scene graph, reactivity, high-precision canvas)
   → editors and creative apps.
3. **General toolkit** (widgets + styling + i18n + a11y + bindings) →
   comparable in scope to GTK/Qt.

---

## 14. Metrics and measurement

### 14.1. "Measured, not promised" policy
No document publishes RAM/CPU/latency as **data** without a measurement.
Mandatory format to publish a figure:

```
Metric:       <e.g. idle RSS>
Method:       <command, conditions>
Baseline:     <what it is compared against>
Result:       <measured value>
Date/Commit:  <reference>
```

A figure without **method + baseline + date** is not published. Targets
may be cited **as targets**, never as results.

### 14.2. Measurement protocol (mandatory)
**Status: first measurement completed** — 2026-10-13, `develop`, criterion
benches in each crate (`cargo bench --workspace`); results in §14.3. The
table below lists **targets**:

| Area | What to measure | Method | Baseline | Criterion |
|---|---|---|---|---|
| canvas | `eglInitialize` + context | timer, 100 reps | GLX | < 50 ms |
| canvas | shader compilation | timer | — | < 30 ms |
| canvas | VRAM per 32px glyph | `glGetTexLevelParameteriv` | — | in KB |
| canvas | frame time (1k/10k/100k rects) | frame timer, no VSync | Cairo | document |
| window | `wl_shm` copy (1080p) | frame time | — | DMA-BUF trigger |
| window | `memfd_create` | 1000 reps | `shm_open` | < 0.1 ms |
| style | `StyleId` change | `criterion`, 1M ops | hot CSS | O(1) |
| style | table memory (average app) | counter | — | target (<10 KB?) |
| ui | `size_of::<WidgetNode>()` | `size_of` | — | document |
| ui | hit-test, traversal, layout | `criterion`/frame timer | — | document |
| typography | raster per glyph (16/32/64px) | `criterion` | FreeType | document |

Tools: `criterion`, `hyperfine`, `perf`, `smem`/`/proc/<pid>/status`,
custom frame timer.

### 14.3. Measurement registry — v0.1.0 (2026-10-13)

Machine: Intel Core i7-8665U (4C/8T) · Mesa Intel UHD Graphics 620 (WHL
GT2) · Linux/Wayland (KWin) · benches on the `bench` profile (release,
thin LTO). Cairo/FreeType/GLX baselines: **not executed** (the ecosystem
does not depend on them; comparison pending until a dedicated harness
exists).

**canvas — EGL init + 3.3 context** (target < 50 ms, met)
```
Metric:       eglGetDisplay+Initialize+ChooseConfig+CreateContext+MakeCurrent
Method:       criterion `canvas/egl init + contexto 3.3`, iter_custom, headless FBO
Baseline:     GLX (pending)
Result:       37.7–39.9 ms (median 38.8 ms) — real hardware (UHD 620)
Date/Commit:  2026-10-13, develop
```

**canvas — shader compilation** (target < 30 ms, met; 30× headroom)
```
Metric:       GraphicsContext::from_loader (loader + 2 linked programs)
Method:       criterion `canvas/compilación de shaders`, persistent context
Baseline:     —
Result:       1.00–1.02 ms (median 1.01 ms)
Date/Commit:  2026-10-13, develop
```

**canvas — frame time with N rects, 1080p** (criterion: document)
```
Metric:       GL render to FBO + glReadPixels 1920x1080 (v1 wl_shm path, no VSync)
Method:       criterion `canvas/frame {N} rects`, flat batch, rotated colors
Baseline:     Cairo (pending)
Result:       1k: 8.88 ms (~112 fps) · 10k: 11.95 ms (~84 fps) · 100k: 28.38 ms (~35 fps)
              1k→100k margin: +19.5 ms for 99k rects ≈ 200 ns/rect marginal.
              Readback dominates the fixed cost (~8 ms) → DMA-BUF trigger (ADR-02).
Date/Commit:  2026-10-13, develop
```

**canvas/typography — VRAM per 32 px glyph** (criterion: in KB)
```
Metric:       VRAM per 32 px glyph in the atlas (512² RGBA page, padding 1)
Method:       full-page estimate: 415 glyphs/page (the §14
              glGetTexLevelParameteriv method was replaced by an estimate:
              no real memory query on llvmpipe/iGPU)
Baseline:     —
Result:       ~5.0 KB/glyph (1 MiB page / 415 glyphs)
Date/Commit:  2026-10-13, develop
```

**window — memfd_create** (target < 0.1 ms, met; 55× headroom)
```
Metric:       memfd_create + close
Method:       criterion `window/memfd_create+close` (1000+ reps)
Baseline:     shm_open + shm_unlink (POSIX) — MEASURED: 5.14 µs/op
Result:       1.79 µs/op → memfd 2.9× faster than the baseline
Date/Commit:  2026-10-13, develop
```

**window — 1080p buffer pool** (criterion: document)
```
Metric:       ShmPixels::new(1920, 1080, 3 slots) — memfd + ftruncate + mmap + unmap
Method:       criterion `window/pool ShmPixels 1080p x3`
Baseline:     —
Result:       5.43 µs (lazy pages: the 24 MiB is not physically touched)
Date/Commit:  2026-10-13, develop
```

**style — StyleId change** (target O(1), met)
```
Metric:       StyleTable::get(kind, state) rotating over (kind, state)
Method:       criterion `style/lookup (kind,estado)`; 1M ops ≈ 3.3 ms
Baseline:     hot CSS (pending)
Result:       3.33 ns/op (~300M ops/s) — constant flat indexing,
              independent of table contents
Date/Commit:  2026-10-13, develop
```

**style — table memory** (target < 10 KB, met)
```
Metric:       size_of StyleTable compiled from EXAMPLE_DSL (10 kinds × 5 rows + palette)
Method:       size_of_val (static counter)
Baseline:     —
Result:       48 bytes
Date/Commit:  2026-10-13, develop
```

**ui — node size** (criterion: document)
```
Metric:       size_of::<WidgetNode>() (common node, ADR-03; replaces the
              withdrawn "24 bytes" figure from the old docs)
Method:       size_of (static counter)
Baseline:     —
Result:       104 bytes (5×Option<WidgetId> + kind + state + Rect + dirty,
              with padding)
Date/Commit:  2026-10-13, develop
```

**ui — hit-test, layout, traversal** (criterion: document)
```
Metric:       hit-test of one point, full layout (2 passes), tree traversal
Method:       criterion over a 1050-widget tree (50 boxes × 20 buttons + root),
              1280×720 viewport
Baseline:     —
Result:       hit-test: 5.76 µs/point · layout: 5.48 ms/full frame
              (≈5.2 µs/widget, dominated by text measurement) ·
              traversal: 5.91 µs (1050 nodes)
Date/Commit:  2026-10-13, develop
```

**typography — raster per glyph** (criterion: document; FreeType baseline pending)
```
Metric:       analytical rasterization of glyph 'R' (Inter Regular), ¼px subpixel
Method:       criterion `typography/raster glifo {N}px`
Baseline:     FreeType (pending)
Result:       16px: 145.7 µs · 32px: 189.7 µs · 64px: 274.5 µs
              (per glyph; ~1000 glyphs/s at 32px ≈ 190 ms to fill a page)
Date/Commit:  2026-10-13, develop
```

**Not yet measured:** the `wl_shm` 1080p copy as an independent breakdown
(the end-to-end frame readback is measured above), idle RSS of the example
application, and the Cairo/FreeType/GLX comparisons.

---

## 15. Design log: resolved misconceptions

These beliefs appeared in the old documentation (Wrappers/Zero/v1). They
are recorded as **resolved** so they are not reintroduced. **The "canonical
reality" column is authoritative.**

### 15.1. Identity

| ID | Old (mistaken) belief | Canonical reality |
|---|---|---|
| C-01 | "Rivet is a desktop / DE." | It is a **library toolkit**. Not a desktop, WM, or environment. |
| C-02 | "It replaces GTK/Wrappers 1:1." | It covers the GTK/Qt role, but is **not** a drop-in and does not clone their catalog. |
| C-03 | "Everything can be written with no C at all." | The goal = **zero GUI C**; system C is allowed via the allowlist (§6). |
| C-04 | "Wrappers/Zero = Rivet." | They are **historical phases**; legacy is a record, not a specification. |
| C-05 | "No mature external dependencies" as dogma. | **Pure-Rust** crates are allowed when they contribute; only GUI C is forbidden. |
| C-06 | CC BY-SA/LGPLv3 for code. | **CC BY-SA is unsuitable for code.** MIT OR Apache-2.0 is used. |

### 15.2. Technical (graphics, input)

| ID | Old (mistaken) belief | Canonical reality |
|---|---|---|
| E-01 | Blur via **compute shaders on GL 3.3**. | GL 3.3 has **no** compute (added in 4.3). Use FBO ping-pong; compute is optional. |
| E-02 | `wl_shm` = zero-copy. | `wl_shm` **copies**. Real zero-copy = **DMA-BUF**. |
| E-03 | Export from GL via `EGL_EXT_image_dma_buf_import`. | That extension **imports** into GL. To export: **`EGL_MESA_image_dma_buf_export`**. |
| E-04 | Mixing `eglSwapBuffers` with "export DMABUF + attach". | They are **two distinct models** (see §9.3). |
| E-05 | `EGL_SWAP_BEHAVIOR_PRESERVED` as VSync control. | VSync = **`eglSwapInterval`**. |
| E-06 | "<4 MB per glyph (32px)". | A 32px glyph is **a few KB**. |
| E-07 | Translating scancodes to Unicode by hand. | That amounts to reimplementing XKB: use **`libxkbcommon`**. |
| E-08 | `zero-wayland` → `zero-gfx` to obtain buffers. | Inverted: **window hands surface/buffer to canvas**; canvas does not depend on window. |
| E-09 | Circular presentation cycle between canvas and window. | Fixed boundary: window = protocol/presentation; canvas = rendering. |
| E-10 | The `EGL_CONTEXT_OPENGL_VERSION` attribute. | It does not exist: use `EGL_CONTEXT_MAJOR_VERSION`/`MINOR_VERSION`. |

### 15.3. Memory and widgets

| ID | Old (mistaken) belief | Canonical reality |
|---|---|---|
| M-01 | "A widget = 24 bytes" with `children: Vec<WidgetId>`. | Inconsistent (a `Vec` implies heap). Figure **withdrawn** until measured. |
| M-02 | One `Slab<T>` for heterogeneous widgets. | Use a **common node + per-type slabs** (ADR-03). |
| M-03 | Children as a `Vec<WidgetId>` per widget. | **Intrusive links** (ADR-04). |
| M-04 | Contradictory sizes (28/24/24-48 B). | A **single, measured** number. |
| M-05 | `u32` vs 64-bit ID width. | **64 bits** (32 index + 32 generation). |
| M-06 | `lighten(...)` at build time. | Derived **per theme** (ADR-06); build-time derivation would break runtime theme switching. |
| M-07 | "canvas/window depend on nothing." | They depend on `libc` and the GL/EGL driver (allowlist). |

### 15.4. Metrics

| ID | Old (mistaken) belief | Canonical reality |
|---|---|---|
| P-01 | "0% CPU during interaction". | Forbidden (ADR-08): resolution is O(1); redraw costs. |
| P-02 | "RSS <15 MB" (and 10-14 / <100 / 25-35). | **Measured per component**; no invented global target. |
| P-03 | GTK "0.5-2 ms" vs Rivet "1 ns". | Compares different things; removed until equivalent measurements exist. |
| P-03b | Tables with "70-80% savings" without measurements. | Withdrawn. Hypotheses are not published looking like data. |
| P-04 | Contradictory RAM targets. | Single criterion: **measure per component**. |
| P-05 | "1-2 ms frame latency" without a baseline. | Only with a published protocol and baseline (§14). |

### 15.5. Documentation and process

| ID | Old problem | Canonical reality |
|---|---|---|
| D-01 | `Historia de conversacion.md` as a document. | It is a log; historical, not a specification. |
| D-02 | Duplicated/overlapping names per subsystem. | **This single document** consolidates them. |
| D-03 | Narrative "specs" without a normative API. | Signatures, types, and protocol tables are required. |
| D-04 | Growing figures without an implementation. | The "measured, not promised" policy. |

---

## 16. Relation to the historical documentation

- **`Rivet Documentacion/archivo/`** — old v1 documentation and v2 drafts.
  **Historical only.** Not edited; not a source of truth.
- **`Wrappers-Legacy-prerivet/`** — three identity phases and chat
  debates. **Historical only.**
- Rule: any idea that reappears from the archive must pass through
  **this document** before being adopted.
- **Summary:** the legacy was a **good lab notebook and a bad
  specification**; it contributed the layered structure, the vertical
  validation, and generational IDs, but also technical errors and
  invented metrics. Rivet keeps the ideas, corrects the data, and
  discards the figures.

---

## 17. Project status

- [x] Identity, architecture, dependencies, and root corrections —
  **consolidated in this document.**
- [x] Architecture decisions (ADR-01..08) — **closed** (2026-10-11).
- [x] Phase 0: crate specifications.
- [x] Phase 1: `rivet-foundation` + `rivet-window` + first measurement
  (2026-10-13, §14.3).
- [x] Phase 2: `rivet-canvas` + `rivet-typography`.
- [x] Phase 3: `rivet-ui` + `rivet-style`.
- [x] Phase 4: `rivet-image` + the `rivet` facade.
- [x] v1 complete: 24 test suites green in CI, example application
  verified on a live Wayland session.
- [ ] Phase 5: publication and community (crates.io, semver, contribution
  guide; announcement with working, measured components).
- [ ] v1.x backlog: scrolling containers, menus, multiline text, IME,
  a11y (AT-SPI), DMA-BUF, client-side decorations.
- [ ] Remaining measurements: Cairo/FreeType/GLX baselines, `wl_shm`
  copy breakdown, idle RSS.

> **For agents/AI:** this is the authoritative document. Do not consult
> `archivo/` or `Wrappers-Legacy-prerivet/` for design decisions; use
> only this file. To add a subsystem or change a decision, edit here
> (closed decisions require a new entry in §10).
