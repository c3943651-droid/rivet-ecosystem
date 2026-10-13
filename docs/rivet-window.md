# rivet-window — Documentación definitiva

Subsistema de **ventanas y superficies Wayland** de Rivet.

- **Versión:** 2.0 (definitiva)
- **Fecha:** 2026-10-11
- **Capa:** 2 (comunicación con el sistema de ventanas)
- **Depende de:** `rivet-foundation`
- **Consumido por:** `rivet-canvas`, `rivet-ui`
- **Reemplaza a:** parte de GTK y librerías de ventanas.
- **Documento paraguas:** `../RIVET.md`

---

## 1. Propósito

`rivet-window` gestiona el ciclo de vida de ventanas y superficies sobre **Wayland** (con **`libxkbcommon`** para teclado). Entrega a `rivet-canvas` una **superficie lista para renderizar** y **buffers**; **no** dibuja, **no** conoce píxeles ni widgets. Es la frontera del ecosistema con el compositor.

Regla de separación (ADR de `../RIVET.md`): **`rivet-window` entrega superficie/buffer a `rivet-canvas`; `rivet-canvas` NO depende de `rivet-window`.** El render recibe un `wgpu`/EGL surface abstracto, no un objeto de ventana.

## 2. Componentes

| Módulo | Responsabilidad |
|---|---|
| `connection` | Conexión al compositor, `wl_registry`, handshake. |
| `surface` | `wl_surface` + rol (toplevel / layer-shell / popup / subsurface). |
| `buffer` | Buffer de píxeles: `wl_shm` (v1) y DMA-BUF (v1.x). |
| `input` | Teclado (`xkbcommon`), puntero, touch, gestos. |
| `window` | Objeto `Window`: estado, tamaño, decoraciones, visibilidad. |
| `display` | Salidas, HiDPI/escala, vsync, modos. |
| `frame` | Sincronización de frames (`wl_callback`). |
| `clipboard` | Selección y *drag & drop*. |
| `error` | `WindowError`. |

## 3. Conexión y handshake

1. `wl_display_connect` → `wl_display_get_registry` → escuchar `wl_registry.global`/`global_remove`.
2. `wl_display_roundtrip` para enumerar globals.
3. Adquirir **enlaces** (`wl_compositor`, `wl_shm`, `wl_seat`, `xdg_wm_base`, `zxdg_output_manager_v1`, `zwp_layer_shell_v1`, `wp_viewporter`, `zwp_linux_dmabuf_v1` si está).
4. Verificar versión mínima de cada interfaz; degradar con elegancia si falta una opcional (p. ej. sin `xdg_wm_base` no hay toplevels).
5. Integrar el fd de Wayland en el bucle `epoll` de `rivet-foundation`.

## 4. Rol de la superficie

- **Toplevel** (`xdg_toplevel`): ventana normal; título, tamaño, estados (maximizado, fullscreen, activo).
- **Layer-shell** (`zwp_layer_shell_v1`): superficies ancladas — **base para el panel y el lanzador** de las apps semilla.
- **Popup** (`xdg_popup`): menús y tooltips, posicionados por `xdg_positioner`.
- **Subsurface** (`wl_subsurface`): composición dentro de otra superficie.

## 5. Buffers — **corrección importante**

Hay dos caminos, y no son equivalentes:

- **`wl_shm` (v1):** buffer en memoria compartida. El cliente escribe píxeles en la CPU y el compositor los toma. **NO es zero-copy**: hay una copia por frame. Simple y universal; es la base de v1.
- **DMA-BUF (`zwp_linux_dmabuf_v1`, v1.x):** buffer de GPU compartido entre cliente y compositor. **Este sí es el camino zero-copy.** Se exporta desde GL con **`EGL_MESA_image_dma_buf_export`** (NO `EGL_EXT_image_dma_buf_import`, que importa *hacia* GL). Requiere **sincronización explícita** (`linux-drm-syncobj-v1`) para evitar *tearing*/condiciones de carrera.

Decisión (ADR-02): **v1 usa `wl_shm`**; la migración a DMA-BUF entra en **v1.x**, **activada por medición** (solo si el throughput de `wl_shm` limita el objetivo de framerate), no por adelantado.

Ciclo de presentación: tomar buffer libre → dibujar (lo hace canvas) → `wl_surface.attach` + `damage` + `commit` → el compositor lo presenta → se devuelve vía `wl_buffer.release`.

## 6. Entrada

- **Teclado:** `wl_keyboard` + **`libxkbcommon`** para *keymap*, modificadores, layout, repetición. Se usa el `keymap` que envía el compositor.
- **Puntero:** `wl_pointer` (entrada/salida, movimiento, botones, eje de scroll/discreto, `axis_value120`), cursores (`wl_cursor_theme`).
- **Touch:** `wl_touch` (down/up/motion/cancel), multi-touch.
- **Gestos:** `zwp_pointer_gestures_v1` (swipe/pinch/hold), `zwp_pointer_constraints_v1`, `zwp_relative_pointer_v1`.
- Los eventos se traducen a un **tipo propio de Rivet** y se enrutan a `rivet-ui`; `rivet-window` no decide foco ni *hit-testing*.

## 7. Salidas, HiDPI y sincronización

- `zxdg_output_manager_v1`: geometría lógica, escala por salida, nombre, `done`.
- **HiDPI:** respetar `scale` entero y `fractional_scale_v1`; superficies múltiplo de la escala para nitidez. El tamaño lógico vs físico se maneja aquí.
- **VSync:** `eglSwapInterval(display, 1)` para el throttle de presentación (GL). **No** usar `EGL_SWAP_BEHAVIOR_PRESERVED` para esto.
- **Frame callback:** `wl_surface.frame` → `wl_callback.done` para pacear el redibujado y evitar dibujar de más.

## 8. Clipboard y Drag & Drop

- `wl_data_device_manager`: selección (copiar/pegar) y DnD.
- MIME types propios (`text/plain;charset=utf-8`, `text/uri-list`, imagen).
- Transferencia por `wl_data_source`/`wl_data_offer` con `pipe` (o `memfd`) gestionados en el bucle de eventos.

## 9. API pública (orientativa)

```rust
pub struct Connection;           // handshake y globals
pub struct Window {               // estado, rol, tamaño, visibilidad
    pub fn new(ctx: &Context, opts: WindowOptions) -> Result<Self, WindowError>;
    pub fn surface(&self) -> &Surface;
    pub fn set_size(&mut self, logical: Size) -> Result<(), WindowError>;
    pub fn commit(&mut self) -> Result<(), WindowError>;
}
pub struct Surface;               // wl_surface + rol
pub struct Buffer;                // shm o dmabuf
pub struct InputEvent;            // enum unificado
pub enum WindowError { ... }
```

## 10. Dependencias

- **Rust:** binding Wayland (`wayland-client`/`wayland-protocols` — Rust) o FFI mínima a `libwayland`; **`libxkbcommon`** (C de sistema, permitido).
- **GL:** solo el *display*/EGL necesario para el contexto (compartido con canvas); no píxeles aquí.
- **Sin GTK, sin Qt, sin toolkit ajeno.**

## 11. Alcance

**v1:** conexión + registry, toplevel, `wl_shm`, teclado/puntero/touch, salidas y HiDPI básico, frame callback, clipboard/DnD básico, layer-shell para panel/lanzador.

**v1.x (por medición):** DMA-BUF + `linux-drm-syncobj-v1`, `fractional_scale` avanzado, gestos, decoraciones del servidor vs cliente.

**Fuera:** X11 (solo Wayland), sesiones remotas, multi-GPU explícito.

## 12. Decisiones y correcciones aplicadas

- **`wl_shm` ≠ zero-copy** (copia); zero-copy = **DMA-BUF**. Retirada la afirmación contraria.
- Export a DMA-BUF con **`EGL_MESA_image_dma_buf_export`** (importación sería `EGL_EXT_image_dma_buf_import`).
- **VSync vía `eglSwapInterval`**, no `EGL_SWAP_BEHAVIOR_PRESERVED`.
- **`rivet-canvas` no depende de `rivet-window`** (ADR de `../RIVET.md`).
- No se promete "0% de CPU" (ADR-08).

## 13. Métricas (objetivos, a medir — `../RIVET.md` §14)

Latencia evento→frame, coste por frame con `wl_shm` vs DMA-BUF, uso de RAM de un buffer 1080p, tiempo de handshake, overhead de presentación. Método: `perf`, ftrace, contadores de frame propios.
