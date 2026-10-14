# rivet-style

Precompiled styling layer of the [Rivet](https://github.com/c3943651-droid/rivet-ecosystem)
GUI toolkit: a small DSL that compiles once into flat tables with
O(1) lookups at runtime — no per-frame tree walking, no string keys.

## What it provides

- **DSL compiler** (`compile`): a declarative stylesheet language with
  selectors by widget kind and state, palettes, insets, radii, and
  transitions.
- **Flat tables** (`StyleTable`): the entire compiled result is a
  `kind × state` table of plain values; lookups are an array index.
- **Themes**: multiple named themes per stylesheet; the application
  selects one at compile time.
- **Transitions**: easing curves (`Ease`) and durations carried in the
  table for the widget layer to animate.

## Usage

```rust
use rivet_style::{compile, WidgetKind, WidgetState};

const SHEET: &str = r#"
widget button {
    background: #3a7bd5;
    color: #ffffff;
    border-radius: 6;
    padding: 8 12;

    state hover  { background: lighten(background, 10%); }
    state active { background: darken(background, 10%); }
}

theme light {
    base: #fafafa;
    text: #202020;
}
"#;

let table = compile(SHEET, "light")?;

let style = table.get(WidgetKind::Button, WidgetState::NORMAL);
println!("button bg: {:?}", style.background);
```

## Notes

- Depends only on `rivet-base`.
- Measured: 3.33 ns per lookup and a 48-byte compiled `StyleTable`
  (see [`docs/RIVET.md` §12](../../docs/RIVET.md)).
- The DSL grammar is documented in
  [`docs/rivet-style.md`](../../docs/rivet-style.md); `rivet-ui`
  queries this table on every widget draw.

## License

MIT OR Apache-2.0.
