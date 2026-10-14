//! Métricas §14 (RIVET.md): tamaño del nodo, hit-test, layout de árbol
//! completo y recorrido de hijos sobre un árbol de ~1050 widgets.
//!
//! `cargo bench -p rivet-ui`

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use rivet_ui::hit::hit_test;
use rivet_ui::layout::layout_tree;
use rivet_ui::{Ui, WidgetNode};

const CAJAS: usize = 50;
const BOTONES_POR_CAJA: usize = 20;

/// Árbol: raíz Box vertical → 50 cajas horizontales de 20 botones.
fn arbol() -> Ui {
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).expect("raíz");
    for _ in 0..CAJAS {
        let fila = ui.registry.create_box(false);
        let fila_id = fila;
        ui.registry.add_child(root, fila_id).expect("fila");
        for j in 0..BOTONES_POR_CAJA {
            let b = ui.registry.create_button(format!("b{j}"), |_| {});
            ui.registry.add_child(fila_id, b).expect("botón");
        }
    }
    ui
}

fn node_size(_c: &mut Criterion) {
    eprintln!(
        "MEMORIA: size_of::<WidgetNode>() = {} bytes (nodo común, ADR-03)",
        std::mem::size_of::<WidgetNode>()
    );
    let _ = black_box(0u8);
}

/// Layout completo (medir + posicionar) del árbol en 1280x720.
fn layout_completo(c: &mut Criterion) {
    let mut ui = arbol();
    let root = ui.registry.root().expect("raíz");
    c.bench_function("ui/layout arbol 1050 widgets", |b| {
        b.iter(|| {
            layout_tree(
                black_box(&mut ui.registry),
                black_box(&mut ui.text),
                black_box(&ui.style),
                black_box(root),
                1280.0,
                720.0,
            )
            .is_ok()
        })
    });
}

/// Hit-test de un punto con recorrido de atrás hacia delante.
fn hit_test_punto(c: &mut Criterion) {
    let mut ui = arbol();
    let root = ui.registry.root().expect("raíz");
    layout_tree(
        &mut ui.registry,
        &mut ui.text,
        &ui.style,
        root,
        1280.0,
        720.0,
    )
    .expect("layout inicial");
    c.bench_function("ui/hit-test 1 punto (arbol 1050)", |b| {
        let mut i = 0f32;
        b.iter(|| {
            i += 3.7;
            black_box(hit_test(
                black_box(&ui.registry),
                black_box(root),
                black_box(40.0 + i.rem_euclid(1200.0)),
                black_box(15.0 + i.rem_euclid(700.0)),
            ))
        })
    });
}

/// Recorrido completo del árbol por enlaces intrusivos.
fn recorrido(c: &mut Criterion) {
    let ui = arbol();
    let root = ui.registry.root().expect("raíz");
    c.bench_function("ui/recorrido arbol completo", |b| {
        b.iter(|| {
            let mut visitas = 0usize;
            let mut pila = vec![black_box(root)];
            while let Some(id) = pila.pop() {
                visitas += 1;
                pila.extend(ui.registry.children(id));
            }
            black_box(visitas)
        })
    });
}

criterion_group!(
    benches,
    node_size,
    layout_completo,
    hit_test_punto,
    recorrido
);
criterion_main!(benches);
