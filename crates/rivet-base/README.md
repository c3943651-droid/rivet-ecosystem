# rivet-base

Foundation crate of the [Rivet](https://github.com/c3943651-droid/rivet-ecosystem)
GUI toolkit: the storage, event loop, configuration, and error primitives
that every other layer builds on. It has no dependencies on the rest of
the ecosystem.

## What it provides

- **Generational slab** (`slab`): stable 64-bit handles (`index` +
  `generation`) over a contiguous `Vec`, with O(1) insert/remove and
  automatic invalidation of stale handles.
- **Event loop** (`event_loop`): a level-triggered `epoll` wrapper with
  file-descriptor registration, timers, POSIX signals, cross-thread
  wakeups, and a post-event callback queue.
- **Channel** (`channel`): an MPMC channel that integrates with the event
  loop without extra threads.
- **Configuration** (`config`): layered TOML configuration
  (`~/.config/rivet/config.toml` plus an explicit path) with a watcher.
- **Context** (`context`): the shared state container handed to every
  crate at init time.
- **Errors and logging**: a common `Result`/`FoundationError` pair and
  the `[LEVEL] [time] [module] [message]` log macros.

## Usage

```rust
use rivet_base::slab::Slab;

let mut slab = Slab::new();
let a = slab.insert(10);
let b = slab.insert(20);

assert_eq!(slab.get(a), Some(&10));
slab.remove(a);
// Stale handles fail instead of returning the wrong element.
assert_eq!(slab.get(a), None);
assert_eq!(slab.get(b), Some(&20));
```

The event loop drives the whole toolkit (windows, timers, redraws):

```rust
use rivet_base::event_loop::EventLoop;

let mut loop_ = EventLoop::new(Some(16 /* ms poll timeout */), 64)?;
// loop_.register_fd(...); loop_.add_timer(...); loop_.run(...)
```

## Position in the ecosystem

Every Rivet crate depends on `rivet-base`; it depends on nothing else in
the ecosystem. It is pure Rust plus `libc` (for `epoll`, `memfd`, and
signals).

## License

MIT OR Apache-2.0.
