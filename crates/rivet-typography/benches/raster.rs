//! Metrics (RIVET.md §12): per-glyph rasterization at 16/32/64 px and
//! estimated VRAM per glyph in the atlas (512² page, padding 1 — same
//! as rivet-ui).
//!
//! `cargo bench -p rivet-typography`

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use rivet_typography::{Font, FontFace, GlyphAtlas, BUNDLED_FONT};

fn font_at(size_px: f32) -> Font {
    let face = FontFace::from_bytes(BUNDLED_FONT.to_vec(), 0).expect("Inter empaquetada parsea");
    Font::with_fallbacks(face, Vec::new(), size_px)
}

fn raster_bench(c: &mut Criterion, size_px: f32, label: &str) {
    let font = font_at(size_px);
    let layout = font.layout("R");
    let placed = layout.glyphs.first().cloned().expect("'R' tiene contorno");
    c.bench_function(label, |b| {
        b.iter(|| {
            let g = font.rasterize(black_box(placed));
            black_box(g.as_ref().map(|g| (g.width, g.height)))
        })
    });
}

fn raster_16(c: &mut Criterion) {
    raster_bench(c, 16.0, "typography/raster glifo 16px");
}

fn raster_32(c: &mut Criterion) {
    raster_bench(c, 32.0, "typography/raster glifo 32px");
}

fn raster_64(c: &mut Criterion) {
    raster_bench(c, 64.0, "typography/raster glifo 64px");
}

/// VRAM por glifo: llena el atlas de 512² con glifos de 32 px hasta
/// abrir la segunda página y reporta bytes/gifo (páginas RGBA = 4 B/px).
fn vram_per_glyph(_c: &mut Criterion) {
    let font = font_at(32.0);
    let mut atlas = GlyphAtlas::new(512, 1);
    let placed = font.layout("R").glyphs[0];
    let rasterized = font.rasterize(placed).expect("glifo");
    let page_bytes = 512 * 512 * 4;
    let mut allocs = 0usize;
    while atlas.page_count() < 2 {
        black_box(atlas.alloc(&rasterized));
        allocs += 1;
    }
    eprintln!(
        "VRAM: página 512x512 RGBA = {page_bytes} bytes; {} glifos/página a 32px => {} bytes/glifo ({} páginas)",
        allocs,
        page_bytes * 2 / allocs.max(1),
        atlas.page_count()
    );
}

criterion_group!(benches, raster_16, raster_32, raster_64, vram_per_glyph);
criterion_main!(benches);
