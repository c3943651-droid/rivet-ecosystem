# Rivet Architecture Specification

* **Version:** 2.3 (Strictly neutral architecture specification)
* **Status:** Authoritative technical specification.
* **Project License:** MIT OR Apache-2.0.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Design Principles](#2-design-principles)
3. [Crate Map & Ecosystem Architecture](#3-crate-map--ecosystem-architecture)
4. [Dependency Policy](#4-dependency-policy)
5. [Memory and Object Model](#5-memory-and-object-model)
6. [Input and Keyboard Subsystem](#6-input-and-keyboard-subsystem)
7. [Graphics Subsystem](#7-graphics-subsystem)
8. [Architecture Decision Records (ADR)](#8-architecture-decision-records-adr)
9. [Subsystem Specifications](#9-subsystem-specifications)
10. [v1 Scope and Roadmap](#10-v1-scope-and-roadmap)
11. [Performance Metrics Policy](#11-performance-metrics-policy)
12. [Metrics Registry (v0.1.0)](#12-metrics-registry-v010)
13. [Project Status](#13-project-status)

---

## 1. Overview

Rivet is a modular, native Rust GUI toolkit designed for building high-performance graphical software, providing the foundation for user interface applications and desktop components.

Rivet is a suite of low-level libraries delivering window management, GPU rendering, typography, styling, and widget composition.

---

## 2. Design Principles

1. **Native Rust Core:** The toolkit avoids heavy C/C++ graphics and text stacks. System-level primitives are utilized strictly through a bounded allowlist.
2. **Strict Modularity:** Organized into decoupled, single-responsibility crates. Every crate can be utilized independently or composed into the full UI framework.
3. **Measured, Not Promised:** No performance, memory footprint, or latency figure is published as data unless empirically verified via rigorous benchmarking on target hardware.
4. **Component First:** Architecture prioritizes clean data layouts, low cache-miss rates, and predictable resource usage.
5. **Composition over Inheritance:** Widget hierarchies are managed via ID-based generational allocators and intrusive links rather than deep object-oriented inheritance trees.

---

## 3. Crate Map & Ecosystem Architecture

| Crate | Responsibility |
| --- | --- |
| `rivet-base` | Generational slab, epoll event loop, TOML configuration, error primitives |
| `rivet-window` | Native Wayland client (`xdg_shell`, `layer_shell`, popups, input, `wl_shm`) |
| `rivet-canvas` | GPU 2D rendering engine (OpenGL 3.3 Core + EGL), shaders, FBOs, batching |
| `rivet-typography` | TTF/OTF parsing, analytical scanline rasterizer, glyph cache, metrics |
| `rivet-image` | Multi-format image decoding (PNG, JPEG, WebP, SVG), icon themes |
| `rivet-style` | Compile-time stylesheet DSL compiler translating rules into $O(1)$ flat tables |
| `rivet-ui` | Retained-mode widget tree, event dispatch, focus management, layout engine |
| `rivet-gui` | Public facade re-exporting the ecosystem with an ergonomic prelude |

---

## 4. Dependency Policy

Rivet enforces a strict boundary regarding external dependencies to maintain architectural control and minimal binary footprint.

* **Forbidden:** External UI bindings, legacy GUI C libraries, or heavy abstraction wrappers.
* **Allowed — System C Allowlist:**
* `libc` (System ABI bindings).
* OpenGL/EGL runtime libraries (`libEGL`, `libGL`).
* `libxkbcommon` (Compositor keyboard map compilation and keysym mapping).


* **Allowed — Pure-Rust Crates:** `serde`/`toml`, `zbus`, `fontdb`, `ttf-parser`, and vetted utility crates that avoid native C graphics or layout bindings.

---

## 5. Memory and Object Model

### 5.1. Generational Slabs

All dynamic entity collections utilize generational memory management (`Slab<T>`). Identifiers are **64-bit handles** composed of a 32-bit index and a 32-bit generation counter, eliminating silent use-after-free vulnerabilities caused by recycled indices.

### 5.2. Widget Storage Architecture

Widgets are structured via a two-level data layout:

1. **Common Node Arena:** `Slab<WidgetNode>` storing a fixed-size header (`type_tag`, `style_id`, `flags`, `bounds`, and structural pointers).
2. **Type-Specific Slabs:** Dedicated contiguous memory buffers for component-specific payloads (`Button`, `Label`, `TextInput`, etc.).
3. **Intrusive Links:** Child nodes are linked via `first_child`, `last_child`, `prev_sibling`, and `next_sibling` pointers directly inside the common node header, avoiding per-widget heap vectors (`Vec<WidgetId>`) and guaranteeing $O(1)$ insertions and removals.

---

## 6. Input and Keyboard Subsystem

* **Keymaps:** `rivet-window` interfaces directly with `libxkbcommon` to query and process compositor keymaps into standard keysyms and UTF-8 characters. Reimplementing XKB layout handling in user-space code is avoided.
* **IME:** Input Method Editor integration (`text-input-v3`) is deferred to post-v1 iterations.

---

## 7. Graphics Subsystem

### 7.1. Rendering Baseline

* **Mandatory Target:** OpenGL 3.3 Core Context + EGL. All geometry batching, text rendering, framebuffers, and ping-pong blur effects operate natively on 3.3.
* **Compute Path:** Optional runtime enhancement via `GL_ARB_compute_shader` (or OpenGL 4.3+ / GLES 3.1) when supported by hardware.

### 7.2. Buffer Presentation Pipelines

```
rivet-canvas pipeline:
  1. Render commands submitted to back buffer (FBO / offscreen surface).
  2. Presentation transfer:
     a) wl_shm path: Pixels written to shared memory (memfd); window attaches and commits (Copy, v1).
     b) dmabuf path: GPU memory exported via EGL_MESA_image_dma_buf_export; zero-copy IPC (v1.x).
  3. VSync synchronization controlled via eglSwapInterval.

```

---

## 8. Architecture Decision Records (ADR)

| ID | Topic | Decision Summary |
| --- | --- | --- |
| **ADR-01** | Graphics Baseline | OpenGL 3.3 Core mandatory; compute optional at runtime. |
| **ADR-02** | Presentation | `wl_shm` via `memfd` for v1; DMA-BUF with explicit sync for v1.x. |
| **ADR-03** | Widget Storage | Common `WidgetNode` header + per-type backing slabs. |
| **ADR-04** | Hierarchy | Intrusive sibling/child links; zero per-node heap vectors. |
| **ADR-05** | Font Discovery | Pure-Rust `fontdb` + bundled fallback font (no external fontconfig dependencies). |
| **ADR-06** | Theming & Styles | Colors derived at compile-time via DSL; theme switching is an $O(1)$ index swap. |
| **ADR-07** | Visual Effects | Blur implemented via FBO ping-pong fragment shaders. |
| **ADR-08** | Performance Rules | State lookup is $O(1)$ memory access, while interaction requires explicit redraw loops. |

---

## 9. Subsystem Specifications

* **`rivet-base`:** Foundation services including generational allocators, epoll event loops, and robust error management.
* **`rivet-window`:** Native Wayland client handling surface lifecycles (`xdg_shell`, `layer_shell`), input seats, and shared buffer pools.
* **`rivet-canvas`:** Immediate/batched 2D GPU renderer targeting OpenGL 3.3 pipelines with damage tracking and offscreen FBO targets.
* **`rivet-typography`:** TrueType/OpenType font parser coupled with an analytical scanline rasterizer and glyph texture cache.
* **`rivet-image`:** Decoder engine for raster formats (PNG, JPEG, WebP) and vector graphics (SVG), along with icon theme parsers.
* **`rivet-style`:** Proprietary `.rivet-style` DSL compiler transforming rules into flat lookup tables.
* **`rivet-ui`:** Retained-mode tree orchestrating multi-pass layout, pointer hit-testing, focus rings, and application event loops.
* **`rivet-gui`:** Top-level ecosystem facade and user prelude.

---

## 10. v1 Scope and Roadmap

### 10.1. v1 Feature Set

* Wayland integration (`xdg_shell` + `layer_shell`, popups, subsurfaces, HiDPI scaling).
* Core UI components: `Box`, `Panel`, `Button`, `Label`, `Image`, `TextInput`.
* GPU-accelerated primitive, image, and Latin typography rendering.
* Precompiled styling rules and event management loops.

### 10.2. v1.x Evolution Backlog

* Scrolling containers (`Scroll` / `List` enhancements).
* Context menus, dialog overlays, and multi-line text wrapping.
* Advanced input methods (IME) and accessibility bindings (AT-SPI).
* Zero-copy display via DMA-BUF and client-side decorations (CSD).

---

## 11. Performance Metrics Policy

### 11.1. "Measured, Not Promised" Standard

All benchmark figures must adhere strictly to the verification format:

```
Metric:      <descriptor>
Method:      <exact tool command / test harness conditions>
Baseline:    <reference baseline comparison>
Result:      <empirical measurement>
Date/Commit: <version reference>

```

---

## 12. Metrics Registry (v0.1.0)

*Hardware Environment: Intel Core i7-8665U · Mesa Intel UHD Graphics 620 · Linux/Wayland (KWin) · `bench` profile (release, thin LTO).*

Per-metric method and baseline details follow the §11 format and are
recorded verbatim in the bench sources (`crates/*/benches/`) and in the
original registry commit (`31954e0`). Cairo/FreeType/GLX baselines were
not executed; comparisons are pending until a dedicated harness exists.

| Metric | Target / Limit | Empirical Result | Status |
| --- | --- | --- | --- |
| EGL Initialization + Context | $< 50\text{ ms}$ | $38.8\text{ ms}$ (median) | ✅ Met |
| Shader Compilation | $< 30\text{ ms}$ | $1.01\text{ ms}$ | ✅ Met ($30\times$ headroom) |
| `memfd_create` Allocation | $< 0.1\text{ ms}$ | $1.79\ \mu\text{s}$ ($2.9\times$ baseline) | ✅ Met |
| Style Lookup (`StyleId`) | $O(1)$ flat index | $3.33\text{ ns/op}$ (~300M ops/s) | ✅ Met |
| Compiled StyleTable Footprint | $< 10\text{ KB}$ | $48\text{ bytes}$ | ✅ Met |
| Frame Render (1080p, 1k rects) | Documented | $8.88\text{ ms}$ (~112 FPS) | Measured |
| Frame Render (1080p, 100k rects) | Documented | $28.38\text{ ms}$ (~35 FPS) | Measured |
| Common Widget Node Size (`WidgetNode`) | Documented | $104\text{ bytes}$ | Measured |
| UI Layout Pass (1050 widgets) | Documented | $5.48\text{ ms}$ (~5.2 µs/widget) | Measured |

---

## 13. Project Status

* [x] Architecture, design records, and core crate boundaries established.
* [x] Phases 0 through 4 complete: all 8 crates implemented, tested, and integrated.
* [x] v1 verified: 24 test suites passing in CI; live integration test executed successfully on a native Wayland compositor.
* [x] Initial empirical benchmarking suite established (§12).
* [ ] Phase 5 (Pending): Public crate publication (`crates.io`), version tagging (`v0.1.0`), and `develop` -> `main` integration.
* [x] Source-level module comments (`//!`) migrated to English across all crates, tests, and benches ([#1](https://github.com/c3943651-droid/rivet-ecosystem/issues/1)).