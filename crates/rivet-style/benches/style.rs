//! Métricas §14 (RIVET.md): consulta de estilos O(1) y compilación del DSL.
//!
//! `cargo bench -p rivet-style`

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use rivet_style::{compile, WidgetKind, WidgetState, EXAMPLE_DSL};

const KINDS: [WidgetKind; 6] = [
    WidgetKind::Button,
    WidgetKind::Label,
    WidgetKind::TextInput,
    WidgetKind::Window,
    WidgetKind::Panel,
    WidgetKind::Scrollbar,
];

const STATES: [WidgetState; 5] = [
    WidgetState::NORMAL,
    WidgetState::HOVER,
    WidgetState::ACTIVE,
    WidgetState::FOCUSED,
    WidgetState::DISABLED,
];

/// Consulta de estilos por (tipo, estado) con rotación de índices: el
/// "cambio de StyleId" del §14 (objetivo: O(1) flat, 1M ops).
fn style_lookup(c: &mut Criterion) {
    let table = compile(EXAMPLE_DSL, "dark").expect("DSL de ejemplo compila");
    eprintln!(
        "MEMORIA: StyleTable ({} tipos x {} filas + paleta): {} bytes",
        rivet_style::WIDGET_KIND_COUNT,
        rivet_style::STATE_ROW_COUNT,
        std::mem::size_of_val(&table)
    );
    c.bench_function("style/lookup (kind,estado)", |b| {
        let mut i = 0usize;
        b.iter(|| {
            let kind = KINDS[i % KINDS.len()];
            let state = STATES[(i / KINDS.len()) % STATES.len()];
            i = i.wrapping_add(1);
            let style = table.get(black_box(kind), black_box(state));
            black_box(style.border_width + style.border_width)
        })
    });
}

/// Compilación completa del DSL de ejemplo (tema dark).
fn style_compile(c: &mut Criterion) {
    c.bench_function("style/compile EXAMPLE_DSL", |b| {
        b.iter(|| compile(black_box(EXAMPLE_DSL), black_box("dark")).is_ok())
    });
}

criterion_group!(benches, style_lookup, style_compile);
criterion_main!(benches);
