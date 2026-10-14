# rivet-canvas

GPU 2D renderer of the [Rivet](https://github.com/c3943651-droid/rivet-ecosystem)
GUI toolkit: OpenGL 3.3 Core profile over EGL, with batching, texture
atlasing, and effect passes. It renders into whatever framebuffer it is
given and never touches the windowing system.

## What it provides

- **Render queue**: a flat command list filled per frame (solid and
  textured rectangles, transforms, blur) that the renderer sorts by
  layer, batch, and texture.
- **Batching**: draw calls are merged by state; the queue is reusable
  across frames (`RenderQueue::clear`).
- **Textures**: `TextureManager` with atlas regions and a `TextureId`
  handle backed by `rivet-base`'s generational slab.
- **Effects**: a separable two-pass Gaussian blur.
- **Statistics**: `FrameStats` (draw calls, triangles, fill rate) after
  every `render()`.
- **Damage tracking**: `mark_damage`/`take_damage` for partial redraws.

## Usage

```rust
use rivet_canvas::{Color, GraphicsContext, Rect, RenderQueue};

let mut queue = RenderQueue::new();
queue.clear_color(Some(Color::BLACK));
queue.rect(0 /* layer */, Rect::new(10.0, 10.0, 100.0, 40.0), Color::WHITE);
queue.blur(1, Rect::new(0.0, 0.0, 128.0, 128.0), 4.0);

// Per frame, against a context built from an EGL loader:
// let stats = ctx.render(&queue);
// ctx.present();
```

`GraphicsContext::from_loader` takes any function-symbol loader
(`eglGetProcAddress` in the `rivet-window` flow), so the crate also runs
headless (Mesa/llvmpipe) — which is how CI tests it.

## Notes

- Depends only on `rivet-base`; **does not depend on `rivet-window`**.
  `window` produces surfaces and presents buffers; `canvas` renders into
  them.
- Shaders are compiled at init (≈1 ms measured); the pipeline targets
  the OpenGL 3.3 Core profile only.
- Measured figures live in the registry
  ([`docs/RIVET.md` §12](../../docs/RIVET.md)).

## License

MIT OR Apache-2.0.
