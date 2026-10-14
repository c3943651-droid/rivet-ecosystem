# RIVET — Documentación Oficial

> **Este es el único documento oficial de Rivet.** Todo lo demás (carpeta `Rivet Documentacion/archivo/`, `Wrappers-Legacy-prerivet/`) es **histórico** y no debe tomarse como especificación. Si algo en el archivo contradice este documento, **manda este documento**.

- **Versión:** 2.0 (integrada)
- **Fecha:** 2026-10-11
- **Estado:** Documento único y autoritativo. Integra la visión, la arquitectura, las decisiones cerradas, las especificaciones por subsistema, la hoja de ruta y el registro de malentendidos resueltos.
- **Licencia del proyecto:** MIT OR Apache-2.0.

---

## Índice

1. [Qué es Rivet](#1-qué-es-rivet)
2. [Historia (por qué existe)](#2-historia-por-qué-existe)
3. [Identidad y posicionamiento](#3-identidad-y-posicionamiento)
4. [Principios](#4-principios)
5. [Mapa de crates](#5-mapa-de-crates)
6. [Política de dependencias](#6-política-de-dependencias)
7. [Modelo de memoria y objetos](#7-modelo-de-memoria-y-objetos)
8. [Entrada y teclado](#8-entrada-y-teclado)
9. [Subsistema gráfico](#9-subsistema-gráfico-especificación-corregida)
10. [Decisiones de Arquitectura (ADR)](#10-decisiones-de-arquitectura-adr)
11. [Especificaciones por subsistema](#11-especificaciones-por-subsistema)
12. [Alcance v1](#12-alcance-v1)
13. [Hoja de ruta](#13-hoja-de-ruta)
14. [Métricas y medición](#14-métricas-y-medición)
15. [Registro de malentendidos resueltos](#15-registro-de-malentendidos-resueltos)
16. [Relación con la documentación histórica](#16-relación-con-la-documentación-histórica)
17. [Estado del proyecto](#17-estado-del-proyecto)

---

## 1. Qué es Rivet

Rivet es un **conjunto de librerías escritas en Rust para crear software gráfico (GUI)**, en el mismo rol que GTK y Qt: desarrolladores lo usan para construir programas, aplicaciones, herramientas y componentes.

**No** es un escritorio, ni un gestor de ventanas, ni un entorno de escritorio. Rivet es **el toolkit**: las librerías sobre las que se construye software.

## 2. Historia (por qué existe)

El proyecto pasó por tres fases de identidad; conviene conocerlas para no repetir errores:

1. **"Wrappers"** — idea inicial de envoltorios. Nombre confuso: sugería FFI sobre C cuando el objetivo era reescribir en Rust.
2. **"Zero" (legacy)** — giro al "Constructor Total". Nombre más claro, pero objetivos de memoria cada vez más extremos (<100 MB → 25-35 MB) **sin mediciones**.
3. **Rivet (actual)** — unificación. Un solo nombre, un solo ecosistema, criterio de **métricas medidas, no prometidas**.

## 3. Identidad y posicionamiento

- **Qué es:** librerías GUI en Rust (rol GTK/Qt).
- **Qué NO es:** un escritorio, un DE, un drop-in 1:1 de GTK/Qt, ni "el GTK nuevo".
- **Diferencial:** Rust 100% nativo (sin puentes C de GUI), cohesión entre crates y huella mínima por diseño.
- **Rampa de entrada (no el techo):** componentes de escritorio ligeros (lanzadores, paneles, overlays, mascotas virtuales, utilidades).
- **Horizonte:** un toolkit general con el que se construyan programas como editores de texto, herramientas creativas y aplicaciones completas.

## 4. Principios

1. **Rust nativo, sin C de GUI.** No hay GTK, Qt, Cairo, Pango ni FreeType en el stack. Se admite "C de sistema" mínimo y acotado (§6).
2. **Una responsabilidad por crate.** Cada crate hace una cosa; el resto es composición.
3. **Medido, no prometido.** Ninguna cifra de memoria/CPU/latencia se publica como dato hasta que exista una medición real sobre un componente real (§14).
4. **Componente primero.** Las crates se priorizan por sus dependencias entre sí y el valor de cada componente (§12).
5. **Cohesión sobre cantidad.** Mejor 6 crates coherentes que 12 superpuestas.
6. **Componer, no heredar.** Widgets por composición; jerarquía por IDs con generación, no por herencia ni punteros.
7. **Open source.** MIT OR Apache-2.0, público para que otros construyan con él y lo mejoren.

---

## 5. Mapa de crates

| Crate | Responsabilidad | Reemplaza a (stack clásico) |
|---|---|---|
| `rivet-foundation` | Slab con generaciones, bucle de eventos (epoll), config TOML, errores, utilidades | GLib + parte de GObject |
| `rivet-window` | Cliente Wayland: socket, `wl_registry`, `xdg_shell`, `layer_shell`, popups, subsurfaces, entrada, buffers, HiDPI | GDK (backend Wayland) + `libwayland-client` |
| `rivet-canvas` | Render 2D por GPU (OpenGL 3.3 Core + EGL), shaders, geometría, texturas, efectos | Cairo + GSK + cargador GL/EGL |
| `rivet-typography` | Parseo TTF/OTF, rasterizado (De Casteljau + scanline), caché de glifos, métricas | FreeType + parte de Pango |
| `rivet-image` | Carga/decodificación de imágenes (PNG/JPEG/SVG/WebP), iconos y temas de iconos | libpng/libjpeg/librsvg + `image` |
| `rivet-style` | Estilos precompilados en tablas planas, estados, paletas/temas | Motor CSS de GTK / QSS |
| `rivet-ui` | Widgets por composición, layout, eventos, foco | GTK (widgets + layout) |
| `rivet` | Fachada que re-exporta el ecosistema | El "paraguas" tipo GTK |

> `rivet-image` es un crate **propio y necesario** del toolkit (iconos e imágenes), no opcional.

```
Rivet (toolkit GUI en Rust)
└── Crates (librerías)  →  foundation · window · canvas · typography · image · style · ui · rivet (fachada)
```

---

## 6. Política de dependencias

La meta **no** es "cero C a cualquier coste", sino **cero C en la capa GUI** y C mínimo, acotado y justificado en la capa de sistema.

**Prohibido:** GTK, Qt, GLib, Cairo, Pango, FreeType, HarfBuzz (C) y sus puentes; `wgpu`, `smithay`, `fontdue` (no por malos, sino por la meta de control propio de la pila).

**Permitido — C de sistema (allowlist):**

| Dependencia | Motivo | Nota |
|---|---|---|
| `libc` | ABI del sistema | Mínimo |
| Driver OpenGL/EGL (`libEGL`/`libGL`) | Acceso a GPU | Sistema; siempre presente |
| `libxkbcommon` | Mapa de teclado / xkb | Único C no evitable hoy |

**Permitido — crates de Rust puro (según aporte):** `serde`/`toml` (config), `zbus` (D-Bus, puro Rust), `fontdb` (fuentes del sistema, sin `fontconfig`), `ttf-parser`, y similares **Rust puro**.

**Regla práctica:**
> Si existe una pieza en **Rust puro** que resuelve el problema, se usa hasta que una crate propia de Rivet la justifique. El **C de GUI** se prohíbe; el **C de sistema** se limita a la allowlist.

---

## 7. Modelo de memoria y objetos

### 7.1. Slab con generaciones
- IDs de **64 bits** = 32 de índice + 32 de generación. Referencias seguras ante liberación (evita *use-after-free* silencioso por índices reutilizados). **Decisión cerrada.**

### 7.2. Almacenamiento de widgets (decisión cerrada)
Modelo de **dos niveles**:
1. **Nodo común** en un arena plano: `Slab<WidgetNode>`, cabecera pequeña de tamaño fijo:
   `{ type_tag: u16, style_id: u32, flags: u32, parent: Option<WidgetId>, first_child, last_child, prev_sibling, next_sibling, bounds: Rect }`.
2. **Datos específicos del tipo** en **slabs por tipo** (`Slab<Button>`, `Slab<Label>`, …); el nodo guarda `type_tag` + el índice a su payload.

**`WidgetId`** = 64 bits (generación:32 | índice:32) al arena de nodos.

**Hijos:** **enlaces intrusivos** (`first_child`/`last_child`/`prev_sibling`/`next_sibling`), **sin `Vec<WidgetId>` por widget**. Inserta/elimina en O(1) y sin asignaciones de heap.

**Motivos:** el nodo es contiguo (buena localidad para hit-test, recorrido y layout global); los datos por tipo son contiguos (buena localidad y *batching*). Evita el desperdicio de un `enum Widget` (tamaño = variante mayor) y el heap por nodo.

> La cifra "24 bytes por widget" de la documentación antigua está **retirada**. El tamaño del nodo se **mide** (§14), no se promete.

---

## 8. Entrada y teclado

- `rivet-window` usa **`libxkbcommon`** para keymap/keysym/compose. Reimplementar XKB "a mano" se **descarta** por coste/riesgo.
- **IME (`text-input-v3`)** se **difiere**: no es necesario para la v1 (texto latino).

---

## 9. Subsistema gráfico (especificación corregida)

### 9.1. Contexto OpenGL
- **Baseline obligatorio: OpenGL 3.3 Core.** Todo el render (formas, texto, blur por *framebuffer ping-pong*, blends, FBO) funciona en 3.3.
- **Ruta "compute" opcional**, detectada en runtime (`GL_ARB_compute_shader` / GL 4.3 / GLES 3.1). El renderer **no depende** de ella.
- **Vulkan:** horizonte v2+.
- Atributos de contexto correctos: `EGL_CONTEXT_MAJOR_VERSION` y `EGL_CONTEXT_MINOR_VERSION` (no existe `EGL_CONTEXT_OPENGL_VERSION`).

### 9.2. Efectos (blur)
El blur base (v1) es **fragment shader con FBO ping-pong** (dos pasadas separables). La versión por **compute shader** es una optimización condicionada a §9.1. No se describe el blur v1 como "compute".

### 9.3. Presentación de buffers
```
Render en rivet-canvas:
  1. Dibujar en el back buffer (FBO o surface).
  2. Presentar según ruta:
     a) wl_shm:  canvas escribe píxeles en el buffer compartido (memfd),
                 window hace wl_surface.attach + commit.        [copia, v1]
     b) dmabuf:  se exporta la imagen (EGL_MESA_image_dma_buf_export),
                 se crea wl_buffer (zwp_linux_dmabuf_v1.create_params),
                 window adjunta y hace commit.                   [zero-copy, v1.x]
  3. Sincronización:
     - v1: implícita (suficiente con wl_shm).
     - DMABUF: preferir explícita (linux-drm-syncobj-v1), no solo implícita.
```

| Ruta | Memoria | ¿Zero-copy? | Uso |
|---|---|---|---|
| `wl_shm` | RAM del sistema (memfd) | **No** (copia) | v1 (simple, portable) |
| DMA-BUF (`zwp_linux_dmabuf_v1`) | VRAM de la GPU | **Sí** | v1.x, con gatillo por medición |

- **Exportar** desde GL = `EGL_MESA_image_dma_buf_export`.
- **Importar** hacia GL = `EGL_EXT_image_dma_buf_import` (p. ej. vídeo). No confundir: la documentación antigua usaba "import" para exportar.
- **VSync** se controla con `eglSwapInterval` (no con `EGL_SWAP_BEHAVIOR_PRESERVED`).

### 9.4. Dirección de dependencias
`rivet-window` **crea/gestiona** el buffer y lo presenta (`attach`/`commit`); `rivet-canvas` **escribe/renderiza** en él. `rivet-canvas` **no depende** de `rivet-window`.

---

## 10. Decisiones de Arquitectura (ADR)

Todas **cerradas** (2026-10-11). Cambios futuros requieren una nueva entrada.

| ID | Tema | Decisión |
|---|---|---|
| **ADR-01** | Baseline gráfico | GL 3.3 Core obligatorio; compute opcional en runtime; Vulkan v2+. |
| **ADR-02** | Presentación | `wl_shm` en v1; DMA-BUF en v1.x, con sincronización explícita y gatillo por medición. |
| **ADR-03** | Almacén de widgets | Nodo común `WidgetNode` + slabs por tipo; `WidgetId` de 64 bits. |
| **ADR-04** | Hijos | Enlaces intrusivos (`first_child`/`last_child`/`prev_sibling`/`next_sibling`); sin `Vec` por widget. |
| **ADR-05** | Fuentes | Descubrimiento con crate `fontdb` (Rust puro) + fuente de respaldo empaquetada; **sin `fontconfig`**. |
| **ADR-06** | Colores | `lighten`/`darken` se derivan **por tema** en codegen; cambiar de tema = swap O(1). |
| **ADR-07** | Efectos | Blur = FBO ping-pong; compute opcional. |
| **ADR-08** | Vocabulario | **Prohibido** "0% de CPU". Frases estándar: resolución de estilo/estado = "O(1): operación de memoria"; toda interacción incluye un redibujado con coste. |

---

## 11. Especificaciones por subsistema

### `rivet-foundation`
- **Propósito:** núcleo del ecosistema.
- **Contenido:** slab con generaciones (IDs de 64 bits), bucle de eventos con `epoll`, config (TOML), tipos de error, utilidades.
- **Decisión:** IDs de 64 bits = 32 índice + 32 generación (cerrada).

### `rivet-window`
- **Propósito:** cliente Wayland nativo.
- **Contenido:** socket, `wl_registry`, `xdg_shell`, `layer_shell`, popups, subsurfaces, entrada (`wl_seat`/pointer/keyboard/touch), buffers (`memfd_create`/`wl_shm`/DMA-BUF), HiDPI.
- **Buffers:**
  - **v1 = `wl_shm`** (memoria compartida con `memfd_create`): simple, portable. **No** es zero-copy.
  - **DMA-BUF = v1.x**, con gatillo basado en medición y **sincronización explícita** (`linux-drm-syncobj-v1`).
  - `wl_shm` **nunca fue zero-copy** (no se "pierde" zero-copy al elegirlo).
- **Teclado:** `libxkbcommon` (compila el keymap del compositor; keysym/UTF-8). No se reimplementa XKB. IME diferido.
- **Dirección:** entrega superficie/buffer a `rivet-canvas`; no al revés.

### `rivet-canvas`
- **Propósito:** render 2D por GPU (OpenGL 3.3 Core + EGL).
- **Contenido:** shaders, geometría, texturas, efectos (FBO ping-pong), presentación.
- **Correcciones clave:** ver §9. Exportación DMA-BUF = `EGL_MESA_image_dma_buf_export`; VSync = `eglSwapInterval`; sin compute en 3.3.
- **VRAM por glifo:** unos **pocos KB** por glifo de 32 px (no MB).
- **No** consume CPU "0%" durante render continuo: hay envío de draw calls y presentación.
- **No depende de `rivet-window`.**

### `rivet-typography`
- **Propósito:** convertir fuentes TrueType/OpenType en glifos rasterizados (bitmap) + métricas base.
- **Contenido:** parser de tablas (con `ttf-parser` para la lectura de bajo nivel; lógica propia de extracción), rasterizado (De Casteljau + scanline, antialiasing), caché de glifos (LRU), métricas (`hmtx`).
- **NO hace:** kerning, shaping, posicionamiento de línea, dirección de escritura. Eso es de `rivet-ui` / `rivet-layout` (futuro).
- **Descubrimiento de fuentes:** crate `fontdb` (Rust puro) + fuente de respaldo empaquetada; **sin `fontconfig`**.
- **Limitaciones v1 (documentadas):** sin kerning (`kern`/`GPOS`); sin shaping (árabe/hindi **no** correctos); **sin hinting completo** (solo alineación a la cuadrícula → nitidez subóptima a cuerpos pequeños); sin emoji a color (COLRv1/SVG).
- **Shaping futuro:** se delegará a `rustybuzz` (Rust puro) en una versión posterior.

### `rivet-image`
- **Propósito:** carga/decodificación de imágenes e iconos.
- **Contenido:** PNG/JPEG (+ SVG/WebP si es viable), iconos y temas de iconos.
- **Necesario** para iconos e imágenes del toolkit; no opcional.

### `rivet-style`
- **Propósito:** estilos precompilados en tablas planas.
- **Contenido:** pipeline de compilación (parser → optimizador → codegen), tablas numéricas planas indexadas por enteros, estados por índice, paletas y temas.
- **Coste:** aplicar un estilo es una **operación de índice (O(1))**. **No** afirma "0% de CPU": la *resolución* es barata, pero la interacción completa redibuja.
- **Colores derivados:** `lighten`/`darken` se generan **por tema** en codegen; cambio de tema = swap O(1). Las cifras tipo "~1 ns" son **objetivos a medir**, no datos.

### `rivet-ui`
- **Propósito:** widgets por composición, layout, eventos, foco.
- **Almacenamiento:** nodo común `WidgetNode` + slabs por tipo; hijos por enlaces intrusivos (§7).
- **Ciclo de vida:** estados Normal/Hover/Active/Focus/Disabled; composición (no herencia); jerarquía por `WidgetId`.
- **Coste:** la *resolución de estado* es O(1); el ciclo completo de interacción incluye layout y redibujado.
- **Correcciones de tamaño:** `Option<WidgetId>` = 8 B con nicho (16 B sin él), nunca 9; `Option<Rect>` = 20 B (padding), nunca 17. El "~83 B" del struct es estimación → **medir**.

### `rivet` (fachada)
- Re-exporta el ecosistema para el usuario final. Es el "paraguas" equivalente a `#include <gtk/gtk.h>`.

---

## 12. Alcance v1

El alcance de Rivet v1 se define por el conjunto mínimo de componentes del toolkit: lo necesario para construir software gráfico ligero sobre Wayland.

**Entra en v1:** `xdg_shell` + `layer_shell`, overlays, popups, subsurfaces, HiDPI; transparencia e input regions (click-through); render de formas, imágenes y texto (latín/números); PNG/JPEG (+SVG si viable) e iconos/temas; widgets mínimos (`Label`, `Box/Row`, `Button`, `Image/Icon`, `TextInput`, `List`, `Popup/Menu`); estilo/config ligero; D-Bus; bucle de animación.

**Fuera de v1 (horizonte):** IME, shaping/BiDi completo, a11y AT-SPI, portapapeles/DnD, motor CSS completo, Vulkan, X11/XWayland, bindings.

---

## 13. Hoja de ruta

### 13.1. Enfoque: build vs. borrow
Para llegar rápido a las apps, se arranca sobre la **pila Rust pura** y se sustituye por crates Rivet donde aporte.

| Necesidad | Se arranca con (Rust puro) | Se sustituye por (Rivet) cuando |
|---|---|---|
| Ventana | `wayland-client` / `smithay`-rs | `rivet-window` |
| Render | `wgpu` u otro | `rivet-canvas` |
| Fuentes | `ttf-parser` + `fontdb` | `rivet-typography` |
| Imágenes | `image` | `rivet-image` |
| D-Bus | `zbus` | (probablemente se mantiene) |
| Estilo | propio | `rivet-style` |
| Widgets | — | `rivet-ui` |

> Se **prohíbe** depender de C de GUI aunque acelere; para esos casos se usa una alternativa **Rust puro** aunque sea menos madura.

### 13.2. Fases
- **Fase 0 — Especificación de las crates.** Firmas, tipos y tablas de protocolo (D-03) de cada crate del §5.
- **Fase 1 — `rivet-foundation` + `rivet-window`.** Bucle epoll, slab generacional, `xdg_shell`/`layer_shell`. **Primera medición** (§14).
- **Fase 2 — `rivet-canvas` + `rivet-typography`.** Render GL 3.3 Core y texto latino.
- **Fase 3 — `rivet-ui` + `rivet-style`.** Widgets por composición y estilos precompilados.
- **Fase 4 — `rivet-image` + fachada `rivet`.** Iconos/temas y re-export del ecosistema.
- **Fase 5 — Publicación y comunidad** (docs en inglés, semver, guía de contribución; anunciar **con componentes funcionando y medidos**).

### 13.3. Camino a toolkit general (horizonte)
1. **Toolkit de componentes** (v1) → gana huella y primeros usuarios.
2. **Framework de UI GPU** (escena, reactividad, canvas de alta precisión) → editores y apps creativas.
3. **Toolkit general** (widgets + estilo + i18n + a11y + bindings) → compite con GTK/Qt.

---

## 14. Métricas y medición

### 14.1. Política "medido, no prometido"
Ningún documento publica RAM/CPU/latencia como **dato** sin medición. Formato obligatorio para publicar una cifra:

```
Métrica:      <p. ej. RSS en reposo>
Método:       <comando, condiciones>
Baseline:     <con qué se compara>
Resultado:    <valor medido>
Fecha/Commit: <referencia>
```

Una cifra sin **método + baseline + fecha** no se publica. Los objetivos pueden citarse **como objetivos**, nunca como resultados.

### 14.2. Protocolo de medición (obligatorio)
**Estado: primera medición realizada** — 2026-10-13, `develop`, benches criterion en cada crate (`cargo bench --workspace`); resultados en §14.3. La tabla siguiente son **objetivos**:

| Área | Qué medir | Método | Baseline | Criterio |
|---|---|---|---|---|
| canvas | `eglInitialize`+contexto | cronómetro, 100 reps | GLX | < 50 ms |
| canvas | compilación de shaders | cronómetro | — | < 30 ms |
| canvas | VRAM por glifo 32px | `glGetTexLevelParameteriv` | — | en KB |
| canvas | frame time (1k/10k/100k rects) | frame timer sin VSync | Cairo | documentar |
| window | copia `wl_shm` (1080p) | frame time | — | gatillo DMA-BUF |
| window | `memfd_create` | 1000 reps | `shm_open` | < 0.1 ms |
| style | cambio de `StyleId` | `criterion`, 1M ops | CSS en caliente | O(1) |
| style | memoria de tablas (app media) | contador | — | objetivo (<10 KB?) |
| ui | `size_of::<WidgetNode>()` | `size_of` | — | documentar |
| ui | hit-test, recorrido, layout | `criterion`/frame timer | — | documentar |
| typography | raster por glifo (16/32/64px) | `criterion` | FreeType | documentar |

Herramientas: `criterion`, `hyperfine`, `perf`, `smem`/`/proc/<pid>/status`, frame timer propio.

### 14.3. Registro de mediciones — v0.1.0 (2026-10-13)

Máquina: Intel Core i7-8665U (4C/8T) · Mesa Intel UHD Graphics 620 (WHL
GT2) · Linux/Wayland (KWin) · benches en perfil `bench` (release, LTO
thin). Baselines Cairo/FreeType/GLX: **no ejecutados** (el ecosistema no
depende de ellos; comparación pendiente si se añade un harness dedicado).

**canvas — init EGL + contexto 3.3** (objetivo < 50 ms ✓)
```
Métrica:      eglGetDisplay+Initialize+ChooseConfig+CreateContext+MakeCurrent
Método:       criterion `canvas/egl init + contexto 3.3`, iter_custom, FBO headless
Baseline:     GLX (pendiente)
Resultado:    37.7–39.9 ms (mediana 38.8 ms) — REAL (UHD 620)
Fecha/Commit: 2026-10-13, develop
```

**canvas — compilación de shaders** (objetivo < 30 ms ✓, margen 30×)
```
Métrica:      GraphicsContext::from_loader (loader + 2 programas enlazados)
Método:       criterion `canvas/compilación de shaders`, contexto persistente
Baseline:     —
Resultado:    1.00–1.02 ms (mediana 1.01 ms)
Fecha/Commit: 2026-10-13, develop
```

**canvas — frame time con N rects, 1080p** (criterio: documentar)
```
Métrica:      render GL al FBO + glReadPixels 1920x1080 (ruta wl_shm v1, sin VSync)
Método:       criterion `canvas/frame {N} rects`, batch plano, colores rotados
Baseline:     Cairo (pendiente)
Resultado:    1k: 8.88 ms (~112 fps) · 10k: 11.95 ms (~84 fps) · 100k: 28.38 ms (~35 fps)
              Margen 1k→100k: +19.5 ms por 99k rects ≈ 200 ns/rect marginal.
              El readback domina el costo fijo (~8 ms) → gatillo DMA-BUF (ADR-02).
Fecha/Commit: 2026-10-13, develop
```

**canvas/typography — VRAM por glifo 32 px** (criterio: en KB)
```
Métrica:      VRAM por glifo de 32 px en el atlas (página 512² RGBA, padding 1)
Método:       estimación por página llena: 415 glifos/página (método §14
              glGetTexLevelParameteriv sustituido por estimación: sin
              query de memoria real en llvmpipe/iGPU)
Baseline:     —
Resultado:    ~5.0 KB/glifo (página de 1 MiB / 415 glifos)
Fecha/Commit: 2026-10-13, develop
```

**window — memfd_create** (objetivo < 0.1 ms ✓, margen 55×)
```
Métrica:      memfd_create + close
Método:       criterion `window/memfd_create+close` (1000+ reps)
Baseline:     shm_open + shm_unlink (POSIX) — MEDIDO: 5.14 µs/op
Resultado:    1.79 µs/op → memfd 2.9× más rápido que el baseline
Fecha/Commit: 2026-10-13, develop
```

**window — pool de buffers 1080p** (criterio: documentar)
```
Métrica:      ShmPixels::new(1920, 1080, 3 slots) — memfd + ftruncate + mmap + unmap
Método:       criterion `window/pool ShmPixels 1080p x3`
Baseline:     —
Resultado:    5.43 µs (páginas lazy: sin tocar físicamente los 24 MiB)
Fecha/Commit: 2026-10-13, develop
```

**style — cambio de StyleId** (objetivo O(1) ✓)
```
Métrica:      StyleTable::get(kind, estado) con rotación de (tipo, estado)
Método:       criterion `style/lookup (kind,estado)`; 1M ops ≈ 3.3 ms
Baseline:     CSS en caliente (pendiente)
Resultado:    3.33 ns/op (~300M ops/s) — indexación plana constante,
              independiente del contenido de la tabla
Fecha/Commit: 2026-10-13, develop
```

**style — memoria de tablas** (objetivo < 10 KB ✓)
```
Métrica:      size_of StyleTable compilada de EXAMPLE_DSL (10 tipos × 5 filas + paleta)
Método:       size_of_val (contador estático)
Baseline:     —
Resultado:    48 bytes
Fecha/Commit: 2026-10-13, develop
```

**ui — tamaño del nodo** (criterio: documentar)
```
Métrica:      size_of::<WidgetNode>() (nodo común ADR-03; la cifra "24 bytes"
              de la doc antigua queda sustituida por la medida)
Método:       size_of (contador estático)
Baseline:     —
Resultado:    104 bytes (5×Option<WidgetId> + kind + estado + Rect + dirty,
              con padding)
Fecha/Commit: 2026-10-13, develop
```

**ui — hit-test, layout y recorrido** (criterio: documentar)
```
Métrica:      hit-test de 1 punto, layout completo (2 pasadas), recorrido del árbol
Método:       criterion sobre árbol de 1050 widgets (50 cajas × 20 botones + raíz),
              viewport 1280×720
Baseline:     —
Resultado:    hit-test: 5.76 µs/punto · layout: 5.48 ms/frame completo
              (≈5.2 µs/widget, dominado por medición de texto) ·
              recorrido: 5.91 µs (1050 nodos)
Fecha/Commit: 2026-10-13, develop
```

**typography — raster por glifo** (criterio: documentar; baseline FreeType pendiente)
```
Métrica:      rasterizado analítico del glifo 'R' (Inter Regular), subpíxel ¼px
Método:       criterion `typography/raster glifo {N}px`
Baseline:     FreeType (pendiente)
Resultado:    16px: 145.7 µs · 32px: 189.7 µs · 64px: 274.5 µs
              (por glifo; ~1000 glifos/s a 32px ≈ 190 ms de atlas lleno)
Fecha/Commit: 2026-10-13, develop
```

**Pendientes de medir:** copia `wl_shm` 1080p como desglose independiente
(el readback del frame end-to-end está medido arriba), RSS en reposo de la
app de ejemplo, comparativas contra Cairo/FreeType/GLX.

---

## 15. Registro de malentendidos resueltos

Estas creencias aparecieron en la documentación antigua (Wrappers/Zero/v1). Se registran **resueltas** para que no vuelvan a introducirse. **La columna "realidad canónica" es la vigente.**

### 15.1. Identidad

| ID | Creencia antigua (errónea) | Realidad canónica |
|---|---|---|
| C-01 | "Rivet es un escritorio / DE." | Es un **toolkit de librerías**. No es escritorio, WM ni entorno. |
| C-02 | "Reemplaza a GTK/Wrappers 1:1." | Cubre el rol de GTK/Qt, pero **no** es drop-in ni clona su catálogo. |
| C-03 | "Se puede escribir todo sin C alguno." | Meta = **cero C de GUI**; se admite C de sistema en allowlist (§6). |
| C-04 | "Wrappers/Zero = Rivet." | Son **fases históricas**; el legacy es registro, no especificación. |
| C-05 | "Sin dependencias externas maduras" como dogma. | Se admiten crates **Rust puro** si aportan; solo se prohíbe C de GUI. |
| C-06 | Licencia CC BY-SA/LGPLv3 para código. | **CC BY-SA no es apta para código.** Se usa MIT OR Apache-2.0. |

### 15.2. Técnico (gráficos, entrada)

| ID | Creencia antigua (errónea) | Realidad canónica |
|---|---|---|
| E-01 | Blur con **compute shaders en GL 3.3**. | GL 3.3 **no** tiene compute (llegan en 4.3). Usar FBO ping-pong; compute opcional. |
| E-02 | `wl_shm` = zero-copy. | `wl_shm` **copia**. Zero-copy real = **DMA-BUF**. |
| E-03 | Exportar desde GL con `EGL_EXT_image_dma_buf_import`. | Esa extensión **importa** hacia GL. Para exportar: **`EGL_MESA_image_dma_buf_export`**. |
| E-04 | Mezclar `eglSwapBuffers` con "exportar DMABUF + attach". | Son **dos modelos** distintos (ver §9.3). |
| E-05 | `EGL_SWAP_BEHAVIOR_PRESERVED` como control de VSync. | VSync = **`eglSwapInterval`**. |
| E-06 | "<4 MB por glifo (32px)". | Un glifo 32px son **unos pocos KB**. |
| E-07 | Traducir scancodes a Unicode "a mano". | Equivale a reimplementar XKB: se usa **`libxkbcommon`**. |
| E-08 | `zero-wayland` → `zero-gfx` para obtener buffers. | Invertida: **window entrega superficie/buffer a canvas**; canvas no depende de window. |
| E-09 | Ciclo de presentación circular entre canvas y window. | Frontera fija: window = protocolo/presentación; canvas = render. |
| E-10 | Atributo `EGL_CONTEXT_OPENGL_VERSION`. | No existe: usar `EGL_CONTEXT_MAJOR_VERSION`/`MINOR_VERSION`. |

### 15.3. Memoria y widgets

| ID | Creencia antigua (errónea) | Realidad canónica |
|---|---|---|
| M-01 | "Un widget = 24 bytes" con `children: Vec<WidgetId>`. | Inconsistente (el `Vec` implica heap). Cifra **retirada** hasta medir. |
| M-02 | Un `Slab<T>` para widgets heterogéneos. | Se usa **nodo común + slabs por tipo** (ADR-03). |
| M-03 | Hijos como `Vec<WidgetId>` por widget. | **Enlaces intrusivos** (ADR-04). |
| M-04 | Tamaños contradictorios (28/24/24-48 B). | Número **único y medido**. |
| M-05 | Ancho de ID `u32` vs 64 bits. | **64 bits** (32 índice + 32 generación). |
| M-06 | `lighten(...)` en build-time. | Se deriva **por tema** (ADR-06); rompería el cambio de tema en runtime. |
| M-07 | "canvas/window no dependen de nada". | Dependen de `libc` y del driver GL/EGL (allowlist). |

### 15.4. Métricas

| ID | Creencia antigua (errónea) | Realidad canónica |
|---|---|---|
| P-01 | "0% de CPU en interacción". | Prohibido (ADR-08): la resolución es O(1); el redibujado cuesta. |
| P-02 | "RSS <15 MB" (y 10-14 / <100 / 25-35). | Se **mide por componente**; sin meta global inventada. |
| P-03 | GTK "0.5-2 ms" vs Rivet "1 ns". | Compara cosas distintas; se elimina hasta tener mediciones equivalentes. |
| P-03b | Tablas con "70-80% de ahorro" sin mediciones. | Retiradas. Las hipótesis no se publican con apariencia de dato. |
| P-04 | Objetivos de RAM contradictorios. | Criterio único: **medir por componente**. |
| P-05 | "Latencia de frame 1-2 ms" sin baseline. | Solo con protocolo y baseline publicado (§14). |

### 15.5. Documentación y proceso

| ID | Problema antiguo | Realidad canónica |
|---|---|---|
| D-01 | `Historia de conversacion.md` como documento. | Es un log; histórico, no especificación. |
| D-02 | Nombres duplicados/superpuestos por subsistema. | **Este documento único** los consolida. |
| D-03 | "Specs" narrativas sin API normativa. | Se exigen firmas, tipos y tablas de protocolo. |
| D-04 | Cifras crecientes sin implementación. | Política "medido, no prometido". |

---

## 16. Relación con la documentación histórica

- **`Rivet Documentacion/archivo/`** — documentación v1 y borradores v2 antiguos. **Solo histórico.** No se edita; no es fuente de verdad.
- **`Wrappers-Legacy-prerivet/`** — tres fases de identidad y debates del chat. **Solo histórico.**
- Regla: cualquier idea que reaparezca del histórico debe pasar por **este documento** antes de entrar.
- **Resumen:** el legacy fue un **buen cuaderno de bitácora y una mala especificación**; aportó la estructura por capas, la validación vertical y los IDs generacionales, pero también errores técnicos y métricas inventadas. Rivet conserva las ideas, corrige los datos y descarta las cifras.

---

## 17. Estado del proyecto

- [x] Identidad, arquitectura, dependencias y correcciones raíz — **consolidado en este documento**.
- [x] Decisiones de arquitectura (ADR 01-08) — **cerradas**.
- [ ] Fase 0: especificación de las crates.
- [ ] Fase 1: `rivet-foundation` + `rivet-window` + primera medición.
- [ ] Resto de fases (§13).

> **Para agentes/IA:** este es el documento autoritativo. No consultes `archivo/` ni `Wrappers-Legacy-prerivet/` para decisiones de diseño; usa únicamente este archivo. Si necesitas añadir un subsistema o cambiar una decisión, edita aquí (las decisiones cerradas requieren una nueva entrada en §10).
