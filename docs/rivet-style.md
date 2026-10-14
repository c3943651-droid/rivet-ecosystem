# rivet-style — Definitive documentation

**Styles and themes** subsystem of Rivet.

- **Version:** 2.0 (definitive)
- **Date:** 2026-10-11
- **Layer:** 3 (presentation)
- **Depends on:** `rivet-foundation`; consumed by `rivet-ui` / `rivet-canvas`.
- **Replaces:** GTK CSS (GTK3 `GtkStyleContext` / GTK4 CSS).
- **Umbrella document:** `../RIVET.md`

---

## 1. Purpose

Separate **appearance** from **widget logic**. Defines a purpose-built style language (DSL), readable and **precompiled into flat tables** that widgets query at render time. No runtime CSS parsing, no expensive cascade, no complex selectors.

## 2. Model

- **DSL `.rivet-style`** (text) → **compiler** → **flat tables** (arrays of structs) indexed by (widget kind, state).
- At runtime, the widget only **indexes** its style: **O(1)**.
- **Theme-derived colors** (ADR-06): `lighten`/`darken`/`alpha`/`mix` are **computed at theme compilation time**, not per frame. A theme change produces precomputed tables; switching themes is **O(1)** (table swap).

## 3. DSL (example)

```
widget button {
    background: #3a7bd5;
    color: #ffffff;
    border-radius: 6;
    padding: 8 12;
    font-size: 14;
    transition: background 150ms ease;

    state hover  { background: lighten(background, 10%); }
    state active { background: darken(background, 10%); }
    state disabled { color: alpha(color, 40%); }
}
theme dark {
    base: #1e1e1e;
    text: #e0e0e0;
}
```

## 4. Components

| Module | Responsibility |
|---|---|
| `lexer` / `parser` | Tokenize and parse the DSL. |
| `ast` | Intermediate representation. |
| `theme` | Theme variable resolution and **color derivation**. |
| `compiler` | AST → flat tables; validation. |
| `tables` | `StyleTable`/`WidgetStyle` structures (indexable). |
| `runtime` | Query by (widget, state); transitions. |
| `error` | `StyleError`. |

## 5. Tables and lookup

```rust
pub struct StyleTable { /* flat arrays per kind */ }
pub struct WidgetStyle {
    pub background: Color, pub foreground: Color,
    pub border: Border, pub padding: Insets, pub font_size: f32, /* ... */
}
pub enum WidgetState { Normal, Hover, Active, Disabled, Focused }

impl StyleTable { pub fn get(&self, kind: WidgetKind, state: WidgetState) -> &WidgetStyle; }
```

## 6. States and transitions

- States as a **bitset**; the widget resolves its state and queries its row.
- Declarative **transitions** (e.g. `background 150ms ease`); the interpolated animation runs in the canvas engine, not in the style tree.
- State priority is documented (Disabled > Active > Focused > Hover > Normal) to avoid ambiguity.

## 7. Themes

- A theme defines base colors, typographic scale, and radii.
- **Theme change = `StyleTable` swap** (O(1)).
- Light/dark themes and variants; no derived color recomputation at runtime.

## 8. Dependencies

- **Pure Rust:** own parser, color utilities (`palette`/in-house).
- **System C:** none in the styles runtime.

## 9. Scope

**v1:** DSL, compiler to tables, states + transitions, light/dark themes, color derivation at compile time.

**v1.x/v2:** style inheritance, class selectors, `@media`/`prefers-color-scheme`, external style loading at runtime (hot-reload), keyframe animations.

## 10. Decisions and applied corrections

- **Theme-derived colors in codegen** (ADR-06): no per-frame cost.
- Any claim of "0% CPU" on theme change is withdrawn: the valid claim is that the change is **O(1)** (table swap).
- **Flat-table** model instead of a CSS cascade.

## 11. Metrics (targets, to be measured — `../RIVET.md` §14)

Table size in RAM, ns per style lookup, µs of DSL compilation, theme swap time. Method: `criterion` + `heaptrack`.

## 12. Preserved ideas (from the previous design)

Strict appearance/logic separation · styles as indexable data · O(1) on the render hot path · compilation instead of runtime interpretation.
