# Contributing to Rivet

## Prerequisites

- Linux with a Wayland session (required for the window and application
  integration tests).
- Mesa with EGL (a hardware GPU or llvmpipe).
- Rust 1.75 or newer.

The full test suite runs headless except `app_wayland`, which opens a
real window and is skipped automatically when `WAYLAND_DISPLAY` is not
set.

## Getting started

```sh
git clone https://github.com/c3943651-droid/rivet-ecosystem
cd rivet-ecosystem
cargo build --workspace
cargo test --workspace
```

To run the example application:

```sh
cargo run -p rivet-gui --example hello
```

## Verification gate

Every change must pass, in this order:

```sh
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same commands on every push. A pull request is not merged
with failing checks.

## Workspace layout

| Crate | Responsibility |
|---|---|
| `rivet-base` | Generational slab, epoll event loop, TOML config, logging |
| `rivet-window` | Wayland client: surfaces, input, clipboard, HiDPI |
| `rivet-canvas` | GPU 2D renderer (OpenGL 3.3 Core + EGL) |
| `rivet-typography` | Font parsing, rasterization, glyph cache |
| `rivet-image` | Image decoding and icon themes |
| `rivet-style` | Style DSL compiler |
| `rivet-ui` | Widgets, layout, events, focus, `Application` |
| `rivet-gui` | Facade and prelude |

Dependency direction is fixed: `window` produces surfaces and presents
buffers; `canvas` renders into them and does not depend on `window`.
Architecture decisions are recorded in `docs/RIVET.md` §8; closed
decisions are not revisited without a new ADR entry.

## Branching and commits

- `develop` is the integration branch; commits land directly on it for
  routine work.
- `main` holds release history; it advances via pull request from
  `develop` at each release.
- Commit messages follow Conventional Commits
  (`feat:`, `fix:`, `docs:`, `perf:`, `refactor:`, `test:`, `chore:`),
  with a scope where useful (`feat(ui): ...`). Keep the subject line
  under 72 characters; explain the *why* in the body when it is not
  obvious from the diff.

## Pull requests

- One topic per pull request. If a change spans several crates, justify
  the coupling in the description.
- PRs must pass the verification gate above before merging.
- Performance-related changes should include before/after numbers from
  `cargo bench` where applicable.

## Documentation language

Documentation and code comments are written in technical English:
README, `docs/*.md`, doc comments (`//!`, `///`), commit messages, and
GitHub issues.

## Semver policy

The workspace is pre-1.0. Until `1.0.0`:

- Breaking API changes may land in minor (`0.x`) releases.
- Once a crate is published to crates.io, a change that breaks the
  public API requires a minor bump and an explicit note in the release
  notes.
- `1.0.0` marks the start of strict semver guarantees.

## Performance claims

No performance, memory, or latency figure may be added to the
documentation without a measurement following the protocol in
`docs/RIVET.md` §11 (method, baseline, result, date/commit). Targets
may be stated as targets, never as results.

## Release process (maintainers)

1. Verify `develop` is green in CI.
2. Open a `develop` → `main` pull request, wait for checks, merge with a
   merge commit (history is preserved; do not squash release
   integrations).
3. Tag the merge commit: `git tag vX.Y.Z main && git push origin vX.Y.Z`.
4. Publish to crates.io in dependency order — each crate consumes the
   previously published versions:
   `rivet-base` → `rivet-window` → `rivet-canvas` →
   `rivet-typography` → `rivet-image` → `rivet-style` → `rivet-ui` →
   `rivet-gui`.
   Publishing a crate makes its source public on crates.io regardless
   of repository visibility.
5. Create the GitHub release from the tag with `gh release create
   vX.Y.Z --generate-notes`.

## License

Contributions are licensed MIT OR Apache-2.0, at the option of the
receiver. By submitting a pull request you agree to license your
contribution under those terms.
