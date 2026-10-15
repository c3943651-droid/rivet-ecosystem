# rivet-ui — Definitive documentation

**Widgets and UI** subsystem of Rivet.

- **Version:** 2.0 (definitive)
- **Date:** 2026-10-11
- **Layer:** 4 (highest level of the toolkit)
- **Depends on:** `rivet-base`, `rivet-canvas`, `rivet-typography`, `rivet-style`, `rivet-image`, `rivet-window` (events).
- **Umbrella document:** `../RIVET.md`

---

## 1. Purpose

Build the **widget hierarchy**, route events, manage focus, measure and position (layout), and order drawing. `rivet-ui` is the layer that application developers use directly.

## 2. Widget store (ADR-03)

**Design correction:** a homogeneous slab forcing one giant `enum` of all widget kinds is NOT used. Instead:

- One **common node** `WidgetNode` with the data shared by all widgets:
  - `id: WidgetId`, `parent`, `first_child`/`last_child`, `next_sibling`/`prev_sibling`, `kind: WidgetKind`, state bits, layout flags, computed geometry.
- **Per-type slabs** (one per widget class: `ButtonSlab`, `LabelSlab`, `ContainerSlab`, ...) that store **only the type-specific** data of each kind.

```rust
pub struct WidgetNode {
    pub generation: u32,
    pub parent: Option<WidgetId>,
    pub first_child: Option<WidgetId>,
    pub last_child: Option<WidgetId>,
    pub next_sibling: Option<WidgetId>,
    pub prev_sibling: Option<WidgetId>,
    pub kind: WidgetKind,
    pub state: WidgetStateBits,
    pub rect: Rect,        // position/size resolved by layout
}
// One slab per type:
struct ButtonSlab { entries: Vec<Entry<ButtonData>> }
// ...one for each widget kind
```

- **Advantage:** the common node is small and homogeneous; type-specific data lives in typed slabs without *padding* imposed by the largest widget.
- **Correction:** the isolated figure of "24 bytes per widget" is withdrawn — the actual cost is `WidgetNode` + the type's slab entry + the widget's data. It is **measured** (see §15).

## 3. Children: intrusive links (ADR-04)

- The parent↔child relationship is modeled with **intrusive links** (`first_child`/`last_child` + `next_sibling`/`prev_sibling`), **not** with a `Vec<WidgetId>` per node.
- Advantages: no per-node `Vec` (fewer allocations, less fragmentation), O(1) relations, predictable traversal.
- Inserting/removing a child is O(1). Child order is insertion order (or reorderable via links).

## 4. Identity

- **`WidgetId` = 64 bits:** 32-bit index + 32-bit generation (consistent with `rivet-base`).
- Access is by ID validated against the generation → detects *use-after-free*.

## 5. Components

| Module | Responsibility |
|---|---|
| `widget` | `WidgetNode`, concrete types, tree. |
| `registry` | `WidgetRegistry`: registration/unregistration, ID validation. |
| `event` | `EventManager` / `EventDispatcher`: routing and *bubbling*. |
| `hit` | `HitTester`: from coordinate → widget (back-to-front tree traversal). |
| `focus` | `FocusManager`: keyboard focus, tab order, pointer focus. |
| `layout` | Measuring + positioning (box model / constraints). |
| `render` | Traverses the tree and emits commands to `rivet-canvas`. |
| `builder` | Declarative DSL/builder for the hierarchy. |
| `error` | `UiError`. |

## 6. Registry

```rust
pub type WidgetId = u64;
pub struct WidgetRegistry { /* nodes + per-type slabs */ }
impl WidgetRegistry {
    pub fn create(&mut self, kind: WidgetKind) -> WidgetId;
    pub fn destroy(&mut self, id: WidgetId);
    pub fn node(&self, id: WidgetId) -> Option<&WidgetNode>;
    pub fn add_child(&mut self, parent: WidgetId, child: WidgetId);
    pub fn remove_child(&mut self, parent: WidgetId, child: WidgetId);
}
```

## 7. Events

- `rivet-window` delivers `InputEvent`; `rivet-ui` converts them to widget events.
- **Phases:** *capture* (root→leaf) → *target* → *bubble* (leaf→root), with the ability to stop propagation.
- **Hit-testing:** by geometry + z-order (children in front); respects `pointer-events`/regions.
- **Focus:** `FocusManager` tracks the focused widget, `Tab`/`Shift+Tab` order, and pointer focus; keyboard events go to the focus.
- **Multi-window (v0.3):** `Application` holds N windows, each with its own `Ui` (focus/state independent): `create_window(WindowOptions)`, `ui_mut_of(index)`, `close_window(index)`, `present_frame_of/configured_of`. Events route by `WindowId`; `run` ends when any UI quits or the last window closes.
- **Widgets (v0.3):** `create_text_area` — multiline `TextInput` (Enter inserts newlines, Up/Down move between lines preserving the column, click positions by line+offset, no wrap yet). `ListView` (list.rs) — model-view lists: `set_items(count, build, update)` reuses live item widgets (no slab/closure churn), creating missing ones and destroying surplus.
- **Programmatic focus (v0.2):** `Ui::focus_widget/focus_next/focus_previous` expose the focus cycle to applications without going through `FocusManager` + registry.
- **Keyboard (v0.2):** every key arrives as `UiInput::Key` (keysym + UTF-8 + modifiers). Resolution order: global handler (`Ui::set_key_handler`, atajos Ctrl+S) → navigation (Tab/Shift+Tab/Escape) → focused widget. `Button` activates with Enter/Space; `TextInput` edits with cursor (`Backspace`/`Delete`/arrows/Home/End, click positions the caret) and `Application` wires `Ctrl+C/V/X` to the system clipboard.

## 8. Layout

- **Box** model (padding/border/margin) with min/preferred/expandable sizes.
- Containers: `Box` (horizontal/vertical), `Grid`, `Overlay`, `ScrollView`, alignment and spacing.
- **Two passes:** measure (desired sizes) → position (assign rects). v1 is a simplified *single-pass*; advanced layout constraints in v1.x.
- Text: queries `rivet-typography` metrics for measuring.
- **v0.2:** each widget is measured exactly once per frame (`MeasureCache`); `position` reads from the cache instead of re-running `measure` per child.
- **v0.2 (fill):** `WidgetRegistry::set_fill(id, horizontal, vertical)` stretches children beyond their preferred size — leftover space is split among main-axis fillers; cross-axis fillers take the container's inner size. `Panel`'s `border-width` is discounted from the children's area.
- **v0.2 (clipping):** containers clip overflowing children to their rect via `RenderQueue::push_clip`/`pop_clip` (batched scissor in `rivet-canvas`).
- **v0.3 (scroll):** `WidgetRegistry::set_scrollable(id, true)` turns a `Box`/`Panel` into a vertical scroll viewport: children lay out over the content area (preferred size), the subtree renders shifted by `scroll_y` and clipped; the wheel (`UiInput::Scroll`) moves the deepest scrollable ancestor under the pointer with clamping. The root is the window viewport (no longer grows to the preferred size); scrollable children never exceed the parent's remaining space.

## 9. Drawing

- In-order tree traversal, skipping the non-visible/`clip`.
- Per widget: queries style (`rivet-style`), state, and emits ordered commands to `rivet-canvas`.
- **v0.2 (dirty-driven frames):** `Ui::is_dirty`/`clear_dirty` walk the `dirty` bits (set by state changes, text edits, cursor moves, focus, `invalidate`); `Application` only renders+presents when something actually changed — mouse moves without visual effect no longer repaint. Full damage-rect scissoring remains a v0.3 target.
- **v0.3-fix (text sharpness):** glyph quads are snapped to the pixel grid (`x.round()`, `baseline.round()`); fractional baselines (ascent 15.5…) spread glyphs across two pixel rows under bilinear sampling. Rasterizer coverage is gamma-corrected (`COVERAGE_GAMMA = 1/2.2`): linear 0.5 coverage on dark backgrounds reads washed-out; the boost thickens thin strokes (+38% core pixels measured).
- **v0.3-fix (shapes & color stacking):** the rounded-rect fan traverses the perimeter in strict clockwise order (the misplaced top-left arc and the spurious closing point left a triangular hole in the bottom-left corner of every Panel/TextInput and overlapped translucent fills). Style colors are **premultiplied** at the canvas boundary (`canvas_color`, text tint) to match the renderer's premultiplied blending — translucent elements (placeholder) no longer glow. `Ui::new_dark` uses `DEFAULT_DARK_DSL`: one coherent palette for panel/label/button/text-input instead of button-only styling.
- **v0.3-fix (text placement):** glyph quads are placed at `baseline + rasterized.top` (`top` is Y-down from the baseline); the original `baseline - top` dropped all text ~2× ascent below its rect. GL viewport/projection are re-synced (`gl.resize`) on every render-target size change — without it the whole scene squeezed into the initial 64×64 pbuffer.
- **v0.2 (zero-alloc present):** `read_pixels_into` fills a reusable buffer and the Y-flip is done in place (no per-frame `Vec` of the whole screen).

## 10. Public API (indicative)

```rust
pub struct Widget;
pub struct Button; pub struct Label; pub struct Box_; pub struct TextInput;
pub struct Application;                 // init/run/quit cycle
pub enum UiEvent { Pointer(..), Key(..), .. }
```

Crate users: create widgets, nest them, connect signals/callbacks, and run the loop.

## 11. v1 widgets

`Container`/`Box`, `Button`, `Label`, `Image`, `TextInput`, `List`/`Scroll`, `Panel`/surface (for seed apps: panel and launcher). The catalog grows in v1.x.

## 12. Dependencies

- Internal: foundation, canvas, typography, style, image, window.
- **Pure Rust** + system C already permitted by the lower layers.

## 13. Scope

**v1:** store (node + slabs), intrusive children, 64-bit IDs, registry, hit-test, focus, capture/bubble routing, basic layout (box/list), essential widgets, incremental-damage repaint.

**v1.x (pending measurement):** more widgets, full flex/grid-style layout, accessibility (roles, `AT-SPI`), IME, high-level drag & drop, layout animations.

**Out of v1:** the full style engine (see `rivet-style`), X11/XWayland, multi-language bindings.

## 14. Decisions and applied corrections

- **`WidgetNode` + per-type slabs** (ADR-03); the "homogeneous store with `Vec<WidgetId>` of children" is withdrawn.
- **Children via intrusive links** (ADR-04).
- **64-bit IDs** (ADR of §7 of the umbrella document).
- The "24 bytes per widget" figure is withdrawn (ADR-08): it depends on the kind; it is measured.
- No "0% CPU" claims.

## 15. Metrics (targets, to be measured — `../RIVET.md` §12)

ns of `create`/`destroy`/`add_child`, ns of hit-test over N widgets, µs of layout of a typical tree, RAM per widget (per kind), events/s dispatched, cost of incremental damage. Method: `criterion` + `heaptrack` + `FrameStats`.

## 16. Preserved ideas (from the previous design)

`WidgetRegistry` as the creation authority · events with capture/bubble phases · geometric `HitTester` · separate `FocusManager` · declarative layout · strict separation between state, style, and drawing.
