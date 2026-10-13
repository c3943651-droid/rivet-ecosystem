# rivet-foundation — Documentación definitiva

> Parte del ecosistema **Rivet**. La visión global, los principios, las decisiones de arquitectura (ADR) y la política de medición están en **`../RIVET.md`** (documento paraguas, autoridad raíz). Este documento es la **especificación definitiva** del subsistema y sustituye a toda la documentación anterior de `rivet-foundation`.

- **Versión:** 2.0 (definitiva)
- **Fecha:** 2026-10-11
- **Rol:** núcleo del ecosistema. **Nivel más bajo**, sin dependencias internas.
- **Reemplaza a:** GLib + parte de GObject.

---

## 1. Propósito

`rivet-foundation` es la base sobre la que se construyen todas las demás crates. Aporta tres cosas:

1. **Memoria:** un *slab allocator* con generaciones; el almacén de objetos del ecosistema.
2. **Eventos:** un bucle de eventos sobre `epoll` (temporizadores, señales, E/S) que mantiene el proceso **bloqueado** en reposo.
3. **Servicios comunes:** configuración, errores/logging, utilidades (tiempo, IDs, endianness).

**No** es un framework de UI ni conoce el dominio gráfico; solo provee los cimientos.

## 2. Componentes

| Módulo | Responsabilidad |
|---|---|
| `slab` | Slab allocator con generaciones e IDs de 64 bits. |
| `event_loop` | Bucle de eventos `epoll`: descriptores, `timerfd`, `signalfd`, despacho. |
| `config` | Configuración TOML tipada + hot-reload. |
| `error` / `log` | `FoundationError`, niveles de log, canales, gancho de pánico. |
| `utils` | `Clock` monotónico, macros, helpers de IDs/UTF-8/endianness, utilidades de test. |

Actores internos: `EpollWrapper`, `TimerManager`, `SignalManager`, `EventDispatcher`, `ConfigLoader`, `ConfigWatcher`, `ConfigNotifier`, `Logger`.

## 3. Slab allocator (decisión cerrada)

```rust
struct Slab<T> {
    entries: Vec<Entry<T>>,   // vector plano y contiguo
    free_list: Vec<usize>,    // índices de slots libres
}
struct Entry<T> {
    generation: u32,
    value: Option<T>,
}
```

**ID de 64 bits** (ADR de `../RIVET.md` §7): bits `0..31` = índice, `32..63` = generación. `id = (gen << 32) | index`.

**Operaciones:** `insert(value) -> Id`, `get(id)`, `get_mut(id)`, `remove(id) -> Option<T>`, iteración sobre ocupados, `clear()`. Todas O(1) salvo la iteración (O(n)).

**Reglas:**
- `remove` no incrementa la generación; `insert` que reutiliza un slot **sí** la incrementa → los IDs viejos quedan invalidados (evita *use-after-free* silencioso).
- Validación: límites + coincidencia de generación + slot ocupado.
- Reutilización de slots vía `free_list` (LIFO) para conservar localidad.

> El tamaño de `Entry<T>` es `~4 + size_of::<Option<T>>()` (p. ej. ~24 B con un `T` de 16 B). Es el **overhead de la entrada del slab**, distinto del tamaño del objeto de dominio (ver `rivet-ui`).

## 4. Bucle de eventos

- **`epoll`** en modo *level-triggered* (simple y predecible). `epoll_create1` + `epoll_ctl` (ADD/MOD/DEL) + `epoll_wait` (con `timeout` en ms; `-1` = bloquear).
- **Temporizadores** vía `timerfd` (único disparo o periódico), integrados en `epoll`.
- **Señales** vía `signalfd`: se bloquean con `sigprocmask` y se entregan como descriptor legible (sin manejadores asíncronos inseguros).
- **Despacho** por lotes (`max_events_per_iteration`, por defecto 64), en el mismo hilo.
- **API:** `register_fd`, `unregister_fd`, `add_timer`, `run`, `quit`, `post_event`.

**Reposo:** con `epoll_wait(..., -1)` el proceso queda **dormido** hasta que llega un evento; el consumo de CPU en reposo tiende a ~0. Esto es un **objetivo a medir**, no una promesa publicada (política "medido, no prometido", `../RIVET.md` §14).

## 5. Configuración

- Formato **TOML**, estructura tipada con valores por defecto.
- **Decisión:** se usa **`serde` + `toml`** (ambos Rust puro; la política de dependencias de `../RIVET.md` §6 los permite). La versión anterior decía hacerlo "a mano"; se simplifica.
- Prioridad: `~/.config/rivet/config.toml` + defaults (ruta configurable en el builder).
- **Hot-reload** vía `inotify` registrado en `epoll` (cooldown sugerido 100 ms); si el parseo falla, se conserva la configuración anterior y se avisa.

Secciones típicas: `[core]` (capacidad del slab), `[event_loop]` (timeout, lote), `[logging]`, `[paths]`.

## 6. Errores y logging

```rust
pub enum FoundationError {
    Slab { kind: SlabErrorKind, id: u64 },
    EventLoop { kind: EventLoopErrorKind, source: Option<io::Error> },
    Config { kind: ConfigErrorKind, path: PathBuf },
    System { source: io::Error },
    Logging { kind: LoggingErrorKind },
}
```

- Toda operación pública falible devuelve `Result<T, FoundationError>`.
- Errores recuperables → `Result`; condiciones que indican bug → `panic!`.
- Un error dentro de un handler **se registra** y no detiene el bucle.
- Logging con niveles `Error`/`Warn`/`Info`/`Debug`/`Trace`, canales `stderr` y archivo, formato `[NIVEL] [TIMESTAMP] [MÓDULO] [MENSAJE]`. Gancho de pánico que registra diagnóstico.

## 7. Utilidades

- **`Clock`** sobre `clock_gettime(CLOCK_MONOTONIC)`: `now_nanos/micros/millis/secs`, `elapsed`.
- **Macros**: `error!`/`warn!`/`info!`/`debug!`/`trace!` (se compilan a vacío si el nivel está desactivado), aserciones de test, helpers de bits de IDs (`make_id!`, `id_index!`, `id_generation!`).
- **Helpers**: validación/creación de IDs, `truncate_utf8`, conversión de endianness (necesaria para Wayland/TrueType).
- **Test/simulación**: generadores de datos de prueba, `MockClock` con `advance`.

## 8. Contexto e integración

```rust
pub struct Context {
    slab: Slab<dyn Any + Send + Sync>,
    event_loop: EventLoop,
    config: RwLock<Config>,
    logger: Logger,
    clock: Clock,
}
```

- Se crea con `ContextBuilder::build()`.
- **Inyección de dependencias explícita** (sin singletons): cada crate recibe `&Context`/`&mut Context`.
- Flujo típico: construir contexto → cargar config → registrar módulos de las otras crates → `event_loop.run()`.

**Orden de dependencias** (quién usa a quién): todo el ecosistema depende de `rivet-foundation`; `rivet-foundation` no depende de nadie del ecosistema.

## 9. Dependencias

- **Rust puro:** `std`, `serde` + `toml` (config).
- **C de sistema (allowlist):** `libc` (syscalls: `epoll`, `timerfd`, `signalfd`, `inotify`, `memfd_create`, `clock_gettime`, `sigprocmask`).
- **No C de GUI.**

## 10. Alcance

**v1:** slab, bucle de eventos (`epoll`/`timerfd`/`signalfd`), config TOML + hot-reload, errores/logging, utilidades, `Context`.

**Fuera de v1 (horizonte):** variables de entorno en la config, logging JSON estructurado, trace IDs, integración de hilos secundarios vía canales (v1.1), múltiples contextos independientes (v2), alineación de memoria personalizada, soporte multi-SO/ARM64.

## 11. Decisiones y correcciones aplicadas

- IDs de **64 bits** (32 índice + 32 generación) — coherente con `../RIVET.md` §7.
- Se admite `serde`/`toml` (Rust puro); no se exige parsear TOML a mano.
- **No** se publica "0% de CPU" como dato: el reposo bloqueado en `epoll` es un mecanismo; la cifra se mide (ADR-08).

## 12. Métricas (objetivos, a medir — `../RIVET.md` §14)

Todas son **objetivos sin verificar**: inserción/`get`/`remove` en decenas de ns, iteración de 10k objetos, latencia de despacho de evento, RAM del contexto vacío, tiempo de inicialización. Método previsto: `criterion`, `perf`, `/proc/<pid>/status`.

## 13. Ideas preservadas (del diseño previo)

Memoria plana sin fragmentación · acceso por índice con generaciones · complejidades definidas y predecibles · inyección de dependencias sin globales · errores con contexto y recuperación · minimizar dependencias externas · "fallo seguro".
