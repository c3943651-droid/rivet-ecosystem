//! Per-frame telemetry (`docs/rivet-canvas.md` §8).
//!
//! Basis of the measurement protocol (`RIVET.md` §14): the values are
//! **frame measurements**, not promises.

use std::time::{Duration, Instant};

/// Estadísticas del último frame renderizado.
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    /// Tiempo total de CPU de `render` (todas las fases).
    pub cpu_frame: Duration,
    /// Lotes dibujados (agrupaciones por shader/textura).
    pub batches: usize,
    /// Llamadas de dibujo (`glDrawElements`).
    pub draw_calls: usize,
    /// Vértices procesados.
    pub vertices: usize,
    /// Comandos de blur aplicados.
    pub blurs: usize,
    /// Subidas/actualizaciones de textura del frame.
    pub texture_uploads: usize,
}

/// Cronómetro interno de fases.
#[derive(Debug)]
pub(crate) struct FrameTimer {
    started: Instant,
}

impl FrameTimer {
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    pub(crate) fn finish(self, stats: &mut FrameStats) {
        stats.cpu_frame = self.started.elapsed();
    }
}
