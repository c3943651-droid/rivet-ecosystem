//! Metrics (RIVET.md §12): `memfd_create` vs `shm_open` (baseline) and
//! 1080p buffer pool creation (memfd + ftruncate + mmap).
//!
//! `cargo bench -p rivet-window`

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use rivet_window::ShmPixels;

/// memfd_create + close (lo que usa `ShmPixels`).
fn memfd_create(c: &mut Criterion) {
    c.bench_function("window/memfd_create+close", |b| {
        b.iter(|| {
            let name = b"rivet-bench\0";
            // SAFETY: syscall directa con nombre NUL-terminado válido.
            let fd = unsafe { libc::memfd_create(name.as_ptr().cast(), 0) };
            assert!(fd >= 0, "memfd_create falló");
            // SAFETY: fd válido recién creado.
            unsafe { libc::close(fd) };
        })
    });
}

/// Baseline POSIX: shm_open + shm_unlink.
fn shm_open(c: &mut Criterion) {
    c.bench_function("window/shm_open+unlink (baseline)", |b| {
        b.iter(|| {
            let name = b"/rivet-bench\0";
            // SAFETY: nombre válido con barra inicial y NUL final.
            let fd = unsafe {
                libc::shm_open(name.as_ptr().cast(), libc::O_CREAT | libc::O_RDWR, 0o600)
            };
            assert!(fd >= 0, "shm_open falló");
            // SAFETY: fd válido; nombre idéntico al de creación.
            unsafe { libc::close(fd) };
            let rc = unsafe { libc::shm_unlink(name.as_ptr().cast()) };
            assert_eq!(rc, 0, "shm_unlink falló");
        })
    });
}

/// Pool completo 1920x1080 x3 slots (memfd + ftruncate + mmap + munmap):
/// el costo real de `ShmPixels::new` de la ruta wl_shm.
fn pool_1080p(c: &mut Criterion) {
    c.bench_function("window/pool ShmPixels 1080p x3", |b| {
        b.iter(|| {
            let pool = ShmPixels::new(1920, 1080, 3);
            black_box(pool.frame_len())
        })
    });
}

criterion_group!(benches, memfd_create, shm_open, pool_1080p);
criterion_main!(benches);
