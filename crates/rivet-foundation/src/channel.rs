//! Cross-thread MPSC channels integrated with the event loop.
//!
//! This enables work on secondary threads (parallel decoding,
//! upload staging, heavy computation): a thread sends with
//! [`Sender::send`], the main loop wakes up (eventfd registered in
//! epoll) and delivers on the same loop thread.
//!
//! In-house implementation: `Mutex<VecDeque>` queue + `eventfd` + the `epoll`
//! of [`EventLoop`](crate::event_loop::EventLoop). No external crates.

use std::collections::VecDeque;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, Mutex};

use crate::error::Result;
use crate::event_loop::{interest, Dispatcher, EventLoop};

/// Mitad emisora: clonable y compartible entre hilos (`Send + Sync`).
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

/// Mitad receptora: vive en el hilo del bucle de eventos.
pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
}

struct Shared<T> {
    queue: Mutex<VecDeque<T>>,
    wake_fd: OwnedFd,
    /// Marca si un receptor registró handler (para diagnóstico de uso).
    registered: Mutex<bool>,
}

/// Crea un par emisor/receptor.
///
/// El receptor se conecta al bucle con [`Receiver::register`]; a partir de
/// ahí cada `send` desde cualquier hilo despierta el bucle.
pub fn channel<T: Send>() -> (Sender<T>, Receiver<T>) {
    // SAFETY: eventfd con flags válidos; comprobamos el fd.
    let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
    assert!(
        fd >= 0,
        "eventfd falló: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: fd recién creado, sin dueño.
    let wake_fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let shared = Arc::new(Shared {
        queue: Mutex::new(VecDeque::new()),
        wake_fd,
        registered: Mutex::new(false),
    });
    (
        Sender {
            shared: Arc::clone(&shared),
        },
        Receiver { shared },
    )
}

impl<T> Sender<T> {
    /// Encola un valor y despierta el bucle si hay receptor registrado.
    ///
    /// No bloquea: la cola es de capacidad dinámica.
    pub fn send(&self, value: T) -> Result<()> {
        {
            let mut queue = self
                .shared
                .queue
                .lock()
                .expect("canal envenenado por un panic en el bucle");
            queue.push_back(value);
        }
        wake(&self.shared.wake_fd);
        Ok(())
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Receiver<T> {
    /// Registra la entrega en el bucle: cada valor encolado invoca `handler`
    /// en el hilo del bucle (con [`Dispatcher`] para postear eventos/quit).
    ///
    /// # Panics
    ///
    /// Si se registra dos veces.
    pub fn register<F>(&mut self, event_loop: &mut EventLoop, handler: F) -> Result<()>
    where
        T: Send + 'static,
        F: FnMut(T, &mut Dispatcher<'_>) + 'static,
    {
        {
            let mut registered = self.shared.registered.lock().expect("lock sano");
            assert!(!*registered, "Receiver::register llamado dos veces");
            *registered = true;
        }
        let shared = Arc::clone(&self.shared);
        let handler = std::rc::Rc::new(std::cell::RefCell::new(handler));
        event_loop.register_fd(
            self.shared.wake_fd.as_raw_fd(),
            interest::READABLE,
            Box::new(move |_fd, _flags, _dispatcher| {
                drain_wake(&shared.wake_fd);
                // Toma todo lo encolado y entrega; lo que llegue después
                // vuelve a despertar por el eventfd.
                let batch: Vec<T> =
                    std::mem::take(&mut *shared.queue.lock().expect("canal envenenado"))
                        .drain(..)
                        .collect();
                for value in batch {
                    let mut handler = handler.borrow_mut();
                    (handler)(value, _dispatcher);
                }
            }),
        )
    }

    /// Drenado manual (útil en tests o sin bucle).
    pub fn try_recv(&mut self) -> Option<T> {
        self.shared
            .queue
            .lock()
            .expect("canal envenenado")
            .pop_front()
    }

    /// Valores pendientes sin consumir.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.shared.queue.lock().expect("canal envenenado").len()
    }
}

fn wake(fd: &OwnedFd) {
    let one: u64 = 1;
    // SAFETY: escritura de 8 bytes en un eventfd propio; los errores se
    // ignoran (un EAGAIN del contador saturado solo retrasa el drenaje, que
    // ya tomará todo lo encolado).
    unsafe {
        let _ = libc::write(fd.as_raw_fd(), (&one as *const u64).cast(), 8);
    }
}

fn drain_wake(fd: &OwnedFd) {
    let mut buf = [0u8; 8];
    // SAFETY: lectura en buffer propio del tamaño del contador del eventfd.
    unsafe {
        let _ = libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), 8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn try_recv_sin_bucle() {
        let (tx, mut rx) = channel::<u32>();
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        assert_eq!(rx.pending(), 2);
        assert_eq!(rx.try_recv(), Some(1));
        assert_eq!(rx.try_recv(), Some(2));
        assert_eq!(rx.try_recv(), None);
    }

    #[test]
    fn emisores_clonables_envian() {
        let (tx, mut rx) = channel::<&'static str>();
        let tx2 = tx.clone();
        tx.send("a").unwrap();
        tx2.send("b").unwrap();
        let mut got = vec![rx.try_recv().unwrap(), rx.try_recv().unwrap()];
        got.sort_unstable();
        assert_eq!(got, ["a", "b"]);
    }

    #[test]
    fn hilo_secundario_despierta_el_bucle() {
        let mut el = EventLoop::new(None, 64).unwrap();
        let (tx, mut rx) = channel::<u32>();
        let got = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let got2 = Rc::clone(&got);
        rx.register(&mut el, move |value, dispatcher| {
            got2.set(value);
            dispatcher.quit();
        })
        .unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(5));
            tx.send(42).unwrap();
        });
        el.run().unwrap();
        assert_eq!(got.get(), 42, "el bucle entregó el valor del hilo");
    }

    #[test]
    fn lote_de_valores_en_una_ráfaga() {
        let mut el = EventLoop::new(None, 64).unwrap();
        let (tx, mut rx) = channel::<u32>();
        let total = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let total2 = Rc::clone(&total);
        rx.register(&mut el, move |value, dispatcher| {
            let t = total2.get() + value;
            total2.set(t);
            if t >= 10 {
                dispatcher.quit();
            }
        })
        .unwrap();
        // Ráfaga antes de arrancar: el bucle la drena completa.
        for i in 0..10 {
            tx.send(i).unwrap();
        }
        el.run().unwrap();
        assert_eq!(total.get(), 45);
    }

    #[test]
    fn sender_es_send_sync() {
        fn assert_send_sync<S: Send + Sync>() {}
        let (tx, _rx) = channel::<Vec<u8>>();
        assert_send_sync::<Sender<Vec<u8>>>();
        let moved = tx;
        std::thread::spawn(move || moved.send(vec![1]).unwrap())
            .join()
            .unwrap();
    }
}
