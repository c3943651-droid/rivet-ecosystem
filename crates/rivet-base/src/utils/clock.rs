//! Monotonic and simulatable clocks.
//!
//! `SystemClock` wraps a monotonic instant (equivalent to
//! `clock_gettime(CLOCK_MONOTONIC)`); `MockClock` allows advancing time
//! manually in tests.

use std::fmt;
use std::time::{Duration, Instant};

/// Fuente de tiempo monotónica intercambiable.
pub trait Clock: fmt::Debug {
    /// Nanosegundos transcurridos desde el origen del reloj.
    fn now_nanos(&self) -> u128;

    /// Microsegundos transcurridos desde el origen del reloj.
    fn now_micros(&self) -> u128 {
        self.now_nanos() / 1_000
    }

    /// Milisegundos transcurridos desde el origen del reloj.
    fn now_millis(&self) -> u128 {
        self.now_nanos() / 1_000_000
    }

    /// Segundos transcurridos desde el origen del reloj.
    fn now_secs(&self) -> u128 {
        self.now_nanos() / 1_000_000_000
    }

    /// Tiempo transcurrido desde una marca anterior de este reloj.
    fn elapsed(&self, since: u128) -> Duration {
        Duration::from_nanos((self.now_nanos() - since) as u64)
    }
}

/// Reloj monotónico del sistema.
#[derive(Debug, Clone)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// Crea un reloj cuyo origen es el momento de la llamada.
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_nanos(&self) -> u128 {
        self.origin.elapsed().as_nanos()
    }
}

/// Reloj simulado para tests: [`MockClock::advance`] mueve el tiempo.
#[derive(Debug, Clone)]
pub struct MockClock {
    nanos: u128,
}

impl MockClock {
    /// Crea un reloj simulado en `t = 0`.
    pub fn new() -> Self {
        Self { nanos: 0 }
    }

    /// Avanza el tiempo simulado (no retrocede).
    ///
    /// # Panics
    ///
    /// Si `by` es [`Duration::ZERO`] o el reloj superaría `u128` de nanos.
    pub fn advance(&mut self, by: Duration) {
        let ns = by.as_nanos();
        assert!(ns > 0, "MockClock::advance requiere duración positiva");
        self.nanos = self
            .nanos
            .checked_add(ns)
            .expect("desbordamiento del reloj simulado");
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MockClock {
    fn now_nanos(&self) -> u128 {
        self.nanos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_avanza() {
        let clock = SystemClock::new();
        let t0 = clock.now_nanos();
        std::thread::sleep(Duration::from_millis(2));
        assert!(clock.now_nanos() > t0);
    }

    #[test]
    fn mock_clock_advance_convierte_unidades() {
        let mut clock = MockClock::new();
        clock.advance(Duration::from_millis(1));
        assert_eq!(clock.now_millis(), 1);
        assert_eq!(clock.now_micros(), 1_000);
        assert_eq!(clock.now_secs(), 0);
        clock.advance(Duration::from_secs(2));
        assert_eq!(clock.now_secs(), 2);
        assert_eq!(clock.now_millis(), 2_001);
    }

    #[test]
    fn elapsed_mide_desde_marca() {
        let mut clock = MockClock::new();
        let mark = clock.now_nanos();
        clock.advance(Duration::from_millis(5));
        assert_eq!(clock.elapsed(mark), Duration::from_millis(5));
    }
}
