//! Rivet Style: estilos precompilados en tablas planas.
//!
//! Pipeline: DSL `.rivet-style` → lexer/parser propios → compilador que
//! resuelve temas y deriva colores (`lighten`/`darken`/`alpha`/`mix` en
//! compilación, ADR-06) → [`StyleTable`] indexable por (widget, estado).
//!
//! Aplicar un estilo en el render es un doble índice a arrays: **O(1)**.
//! Cambiar de tema es intercambiar la tabla: **O(1)** (swap, ADR-06).

#![forbid(unsafe_op_in_unsafe_fn)]

mod ast;
mod compiler;
mod error;
mod lexer;
mod parser;
pub mod tables;

pub use color::Color;
pub use compiler::compile;
pub use error::{Result, StyleError};
pub use tables::{
    state_row, Ease, Insets, StyleTable, Transition, WidgetKind, WidgetState, WidgetStyle,
    STATE_ROW_COUNT, WIDGET_KIND_COUNT,
};

mod color;

/// DSL de ejemplo incluido en la doc (`docs/rivet-style.md` §3).
pub const EXAMPLE_DSL: &str = r#"
widget button {
    background: #3a7bd5;
    color: #ffffff;
    border-radius: 6;
    padding: 8 12;
    font-size: 14;
    transition: 150ms ease;

    state hover  { background: lighten(background, 10%); }
    state active { background: darken(background, 10%); }
    state disabled { color: alpha(color, 40%); }
}
theme dark {
    base: #1e1e1e;
    text: #e0e0e0;
}
theme light {
    base: #fafafa;
    text: #202020;
}
"#;
