# rivet-foundation — Definitive documentation

> Part of the **Rivet** ecosystem. The overall vision, principles, architecture decisions (ADRs), and the measurement policy are in **`../RIVET.md`** (umbrella document, root authority). This document is the **definitive specification** of the subsystem and supersedes all previous documentation of `rivet-foundation`.

- **Version:** 2.0 (definitive)
- **Date:** 2026-10-11
- **Role:** core of the ecosystem. **Lowest layer**, no internal dependencies.

---

## 1. Purpose

`rivet-foundation` is the base on which all other crates are built. It provides three things:

1. **Memory:** a generational *slab allocator*; the ecosystem's object store.
2. **Events:** an event loop over `epoll` (timers, signals, I/O) that keeps the process **blocked** while idle.
3. **Common services:** configuration, errors/logging, utilities (time, IDs, endianness).

It is **not** a UI framework and knows nothing of the graphics domain; it only provides the foundations.

## 2. Components

| Module | Responsibility |
|---|---|
| `slab` | Slab allocator with generations and 64-bit IDs. |
| `event_loop` | `epoll` event loop: descriptors, `timerfd`, `signalfd`, dispatch. |
| `config` | Typed TOML configuration + hot-reload. |
| `error` / `log` | `FoundationError`, log levels, sinks, panic hook. |
| `utils` | Monotonic `Clock`, macros, ID/UTF-8/endianness helpers, test utilities. |

Internal actors: `EpollWrapper`, `TimerManager`, `SignalManager`, `EventDispatcher`, `ConfigLoader`, `ConfigWatcher`, `ConfigNotifier`, `Logger`.

## 3. Slab allocator (closed decision)

```rust
struct Slab<T> {
    entries: Vec<Entry<T>>,   // flat, contiguous vector
    free_list: Vec<usize>,    // indices of free slots
}
struct Entry<T> {
    generation: u32,
    value: Option<T>,
}
```

**64-bit ID** (ADR in `../RIVET.md` §5): bits `0..31` = index, `32..63` = generation. `id = (gen << 32) | index`.

**Operations:** `insert(value) -> Id`, `get(id)`, `get_mut(id)`, `remove(id) -> Option<T>`, iteration over occupied slots, `clear()`. All O(1) except iteration (O(n)).

**Rules:**
- `remove` does not increment the generation; an `insert` that reuses a slot **does** increment it → old IDs are invalidated (avoids silent *use-after-free*).
- Validation: bounds + generation match + occupied slot.
- Slot reuse via `free_list` (LIFO) to preserve locality.

> The size of `Entry<T>` is `~4 + size_of::<Option<T>>()` (e.g. ~24 B with a 16 B `T`). This is the **slab entry overhead**, distinct from the size of the domain object (see `rivet-ui`).

## 4. Event loop

- **`epoll`** in *level-triggered* mode (simple and predictable). `epoll_create1` + `epoll_ctl` (ADD/MOD/DEL) + `epoll_wait` (with `timeout` in ms; `-1` = block).
- **Timers** via `timerfd` (one-shot or periodic), integrated into `epoll`.
- **Signals** via `signalfd`: blocked with `sigprocmask` and delivered as a readable descriptor (no unsafe async handlers).
- **Dispatch** in batches (`max_events_per_iteration`, default 64), on the same thread.
- **API:** `register_fd`, `unregister_fd`, `add_timer`, `run`, `quit`, `post_event`.

**Idle behavior:** with `epoll_wait(..., -1)` the process stays **asleep** until an event arrives; idle CPU usage tends toward ~0. This is a **target to be measured**, not a published promise ("measured, not promised" policy, `../RIVET.md` §12).

## 5. Configuration

- **TOML** format, typed structure with default values.
- **Decision:** **`serde` + `toml`** are used (both pure Rust; allowed by the dependency policy of `../RIVET.md` §4). The previous version required a hand-written parser; this is simplified.
- Priority: `~/.config/rivet/config.toml` + defaults (path configurable in the builder).
- **Hot-reload** via `inotify` registered in `epoll` (suggested cooldown 100 ms); if parsing fails, the previous configuration is kept and a warning is logged.

Typical sections: `[core]` (slab capacity), `[event_loop]` (timeout, batch), `[logging]`, `[paths]`.

## 6. Errors and logging

```rust
pub enum FoundationError {
    Slab { kind: SlabErrorKind, id: u64 },
    EventLoop { kind: EventLoopErrorKind, source: Option<io::Error> },
    Config { kind: ConfigErrorKind, path: PathBuf },
    System { source: io::Error },
    Logging { kind: LoggingErrorKind },
}
```

- Every fallible public operation returns `Result<T, FoundationError>`.
- Recoverable errors → `Result`; conditions that indicate a bug → `panic!`.
- An error inside a handler is **logged** and does not stop the loop.
- Logging with levels `Error`/`Warn`/`Info`/`Debug`/`Trace`, `stderr` and file sinks, format `[LEVEL] [TIMESTAMP] [MODULE] [MESSAGE]`. Panic hook that logs diagnostics.

## 7. Utilities

- **`Clock`** over `clock_gettime(CLOCK_MONOTONIC)`: `now_nanos/micros/millis/secs`, `elapsed`.
- **Macros**: `error!`/`warn!`/`info!`/`debug!`/`trace!` (compiled to nothing if the level is disabled), test assertions, ID bit helpers (`make_id!`, `id_index!`, `id_generation!`).
- **Helpers**: ID validation/creation, `truncate_utf8`, endianness conversion (needed for Wayland/TrueType).
- **Test/simulation**: test data generators, `MockClock` with `advance`.

## 8. Context and integration

```rust
pub struct Context {
    slab: Slab<dyn Any + Send + Sync>,
    event_loop: EventLoop,
    config: RwLock<Config>,
    logger: Logger,
    clock: Clock,
}
```

- Created with `ContextBuilder::build()`.
- **Explicit dependency injection** (no singletons): each crate receives `&Context`/`&mut Context`.
- Typical flow: build context → load config → register modules from the other crates → `event_loop.run()`.

**Dependency order** (who uses whom): the whole ecosystem depends on `rivet-foundation`; `rivet-foundation` depends on nothing in the ecosystem.

## 9. Dependencies

- **Pure Rust:** `std`, `serde` + `toml` (config).
- **System C (allowlist):** `libc` (syscalls: `epoll`, `timerfd`, `signalfd`, `inotify`, `memfd_create`, `clock_gettime`, `sigprocmask`).
- **No GUI C.**

## 10. Scope

**v1:** slab, event loop (`epoll`/`timerfd`/`signalfd`), TOML config + hot-reload, errors/logging, utilities, `Context`.

**Out of v1 (horizon):** environment variables in config, structured JSON logging, trace IDs, secondary-thread integration via channels (v1.1), multiple independent contexts (v2), custom memory alignment, multi-OS/ARM64 support.

## 11. Decisions and applied corrections

- **64-bit** IDs (32 index + 32 generation) — consistent with `../RIVET.md` §5.
- `serde`/`toml` are accepted (pure Rust); hand-written TOML parsing is not required.
- **No** "0% CPU" figure is published: blocking on `epoll` while idle is a mechanism; the number is measured (ADR-08).

## 12. Metrics (targets, to be measured — `../RIVET.md` §12)

All are **unverified targets**: insert/`get`/`remove` in tens of ns, iteration over 10k objects, event dispatch latency, RAM of an empty context, initialization time. Planned method: `criterion`, `perf`, `/proc/<pid>/status`.

## 13. Preserved ideas (from the previous design)

Flat memory without fragmentation · index access with generations · defined and predictable complexities · dependency injection without globals · errors with context and recovery · minimize external dependencies · "fail safe".
