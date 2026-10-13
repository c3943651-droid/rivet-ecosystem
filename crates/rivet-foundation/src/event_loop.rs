//! Bucle de eventos sobre `epoll` (Linux).
//!
//! `docs/rivet-foundation.md` §4: `epoll` *level-triggered*, temporizadores
//! vía `timerfd`, señales vía `signalfd` (bloqueadas con `sigprocmask`),
//! despacho por lotes en el hilo que ejecuta [`EventLoop::run`]. En reposo el
//! proceso queda bloqueado en `epoll_wait` (consumo tendente a ~0; objetivo a
//! medir, no prometido).
//!
//! `post_event` despierta el bucle con un `eventfd` interno.
//!
//! Este tipo usa `Rc`/`RefCell` para los manejadores: **no es `Send`**; el
//! bucle vive en un único hilo (v1).

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io;
use std::os::unix::io::RawFd;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::time::Duration;

use crate::error::{EventLoopErrorKind, FoundationError, Result};

/// Manejador de descriptor: recibe el fd y los flags `epoll` listos.
pub type FdHandler = Box<dyn FnMut(RawFd, u32, &mut Dispatcher<'_>)>;

/// Manejador de temporizador.
pub type TimerHandler = Box<dyn FnMut(&mut Dispatcher<'_>)>;

/// Manejador de señal (recibe el número de señal).
pub type SignalHandler = Box<dyn FnMut(i32, &mut Dispatcher<'_>)>;

/// Clousure diferido ejecutado por el bucle con acceso a este.
type Posted = Box<dyn FnOnce(&mut EventLoop)>;

/// Intereses epoll de uso cómodo.
pub mod interest {
    /// Descriptor legible.
    pub const READABLE: u32 = libc::EPOLLIN as u32;
    /// Descriptor escribible.
    pub const WRITABLE: u32 = libc::EPOLLOUT as u32;
}

#[derive(Clone)]
struct TimerRecord {
    id: u64,
    period: Option<Duration>,
    handler: Rc<RefCell<TimerHandler>>,
}

/// Pase de acceso limitado para manejadores: encolar eventos o terminar el bucle.
pub struct Dispatcher<'a> {
    posted: &'a mut VecDeque<Posted>,
    wake_fd: RawFd,
}

impl Dispatcher<'_> {
    /// Encola un clousure que se ejecutará en el bucle con `&mut EventLoop`.
    pub fn post_event<F: FnOnce(&mut EventLoop) + 'static>(&mut self, f: F) {
        self.posted.push_back(Box::new(f));
        wake(self.wake_fd);
    }

    /// Solicita que el bucle termine tras la iteración en curso.
    pub fn quit(&mut self) {
        self.posted
            .push_back(Box::new(|el: &mut EventLoop| el.running = false));
        wake(self.wake_fd);
    }
}

/// Bucle de eventos `epoll` con temporizadores, señales y eventos posteados.
pub struct EventLoop {
    epfd: RawFd,
    wake_fd: RawFd,
    fds: HashMap<RawFd, Rc<RefCell<FdHandler>>>,
    timers: HashMap<RawFd, TimerRecord>,
    timer_ids: HashMap<u64, RawFd>,
    next_timer_id: u64,
    sig_fd: Option<RawFd>,
    sig_handlers: HashMap<i32, Rc<RefCell<SignalHandler>>>,
    original_mask: Option<libc::sigset_t>,
    posted: VecDeque<Posted>,
    timeout_ms: libc::c_int,
    max_events: usize,
    running: bool,
}

impl EventLoop {
    /// Crea el bucle.
    ///
    /// * `timeout_ms`: espera máxima de `epoll_wait` por iteración;
    ///   `-1` bloquea indefinidamente (reposo).
    /// * `max_events`: tamaño del lote de despacho (por defecto 64).
    pub fn new(timeout_ms: Option<u64>, max_events: usize) -> Result<Self> {
        let max_events = max_events.max(1);
        let timeout_ms = match timeout_ms {
            Some(ms) => ms.min(i32::MAX as u64) as libc::c_int,
            None => -1,
        };

        // SAFETY: llamadas a syscalls con argumentos válidos.
        let epfd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if epfd < 0 {
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::CreateFailed,
                Some(io::Error::last_os_error()),
            ));
        }
        let wake_fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if wake_fd < 0 {
            // SAFETY: fd creado por nosotros y aún abierto.
            unsafe { libc::close(epfd) };
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::CreateFailed,
                Some(io::Error::last_os_error()),
            ));
        }

        let this = Self {
            epfd,
            wake_fd,
            fds: HashMap::new(),
            timers: HashMap::new(),
            timer_ids: HashMap::new(),
            next_timer_id: 0,
            sig_fd: None,
            sig_handlers: HashMap::new(),
            original_mask: None,
            posted: VecDeque::new(),
            timeout_ms,
            max_events,
            running: false,
        };

        // El eventfd se registra como interés puro: el despacho lo drena.
        let mut ev = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: wake_fd as u64,
        };
        // SAFETY: `ev` es un `epoll_event` válido y wake_fd está abierto.
        if unsafe { libc::epoll_ctl(epfd, libc::EPOLL_CTL_ADD, wake_fd, &mut ev) } != 0 {
            let err = Some(io::Error::last_os_error());
            drop(this);
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::RegistrationFailed,
                err,
            ));
        }
        Ok(this)
    }

    /// Registra un descriptor con los intereses dados.
    ///
    /// El `fd` sigue siendo propiedad del llamador (nunca se cierra aquí).
    pub fn register_fd(&mut self, fd: RawFd, events: u32, handler: FdHandler) -> Result<()> {
        if self.fds.contains_key(&fd) {
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::RegistrationFailed,
                Some(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "descriptor ya registrado",
                )),
            ));
        }
        let mut ev = libc::epoll_event {
            events,
            u64: fd as u64,
        };
        // SAFETY: syscall con argumentos válidos.
        if unsafe { libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_ADD, fd, &mut ev) } != 0 {
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::RegistrationFailed,
                Some(io::Error::last_os_error()),
            ));
        }
        self.fds.insert(fd, Rc::new(RefCell::new(handler)));
        Ok(())
    }

    /// Elimina el registro de un descriptor (no lo cierra).
    pub fn unregister_fd(&mut self, fd: RawFd) -> Result<()> {
        if self.fds.remove(&fd).is_none() {
            return Ok(()); // idempotente
        }
        // SAFETY: syscall con argumentos válidos.
        if unsafe { libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut()) } != 0
        {
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::RemovalFailed,
                Some(io::Error::last_os_error()),
            ));
        }
        Ok(())
    }

    /// Añade un temporizador.
    ///
    /// * `after`: retardo hasta el primer disparo (debe ser > 0).
    /// * `period`: `Some(d)` lo hace periódico; `None`, de único disparo.
    ///
    /// Devuelve el ID del temporizador para poder cancelarlo.
    pub fn add_timer(
        &mut self,
        after: Duration,
        period: Option<Duration>,
        handler: TimerHandler,
    ) -> Result<u64> {
        assert!(after > Duration::ZERO, "add_timer: el retardo debe ser > 0");
        // SAFETY: syscall con flags válidos; comprobamos el resultado.
        let fd = unsafe {
            libc::timerfd_create(
                libc::CLOCK_MONOTONIC,
                libc::TFD_CLOEXEC | libc::TFD_NONBLOCK,
            )
        };
        if fd < 0 {
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::CreateFailed,
                Some(io::Error::last_os_error()),
            ));
        }
        let interval = period.map(duration_to_timespec).unwrap_or(libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        });
        let its = libc::itimerspec {
            it_interval: interval,
            it_value: duration_to_timespec(after),
        };
        // SAFETY: syscall con argumentos válidos; comprobamos el resultado.
        if unsafe { libc::timerfd_settime(fd, 0, &its, std::ptr::null_mut()) } != 0 {
            let err = Some(io::Error::last_os_error());
            // SAFETY: fd creado por nosotros y aún abierto.
            unsafe { libc::close(fd) };
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::RegistrationFailed,
                err,
            ));
        }
        let mut ev = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: fd as u64,
        };
        // SAFETY: syscall con argumentos válidos.
        if unsafe { libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_ADD, fd, &mut ev) } != 0 {
            let err = Some(io::Error::last_os_error());
            // SAFETY: fd creado por nosotros y aún abierto.
            unsafe { libc::close(fd) };
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::RegistrationFailed,
                err,
            ));
        }
        let id = self.next_timer_id;
        self.next_timer_id = id
            .checked_add(1)
            .expect("desbordamiento de IDs de temporizador");
        self.timers.insert(
            fd,
            TimerRecord {
                id,
                period,
                handler: Rc::new(RefCell::new(handler)),
            },
        );
        self.timer_ids.insert(id, fd);
        Ok(id)
    }

    /// Cancela un temporizador por ID. Idempotente.
    pub fn cancel_timer(&mut self, id: u64) -> Result<()> {
        match self.timer_ids.remove(&id) {
            Some(fd) => {
                self.timers.remove(&fd);
                // SAFETY: fd de temporizador creado por nosotros.
                unsafe {
                    libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut())
                };
                // SAFETY: fd de temporizador creado por nosotros.
                unsafe { libc::close(fd) };
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Registra un manejador de señal.
    ///
    /// La señal se bloquea con `sigprocmask` en este hilo y se entrega como
    /// descriptor legible (`signalfd`), sin manejadores asíncronos inseguros.
    pub fn add_signal(&mut self, signum: i32, handler: SignalHandler) -> Result<()> {
        self.sig_handlers
            .insert(signum, Rc::new(RefCell::new(handler)));
        self.apply_signal_mask()
    }

    /// Elimina el manejador de una señal. Idempotente.
    pub fn remove_signal(&mut self, signum: i32) -> Result<()> {
        if self.sig_handlers.remove(&signum).is_none() {
            return Ok(());
        }
        self.apply_signal_mask()
    }

    /// Encola un clousure para el bucle y lo despierta si estaba bloqueado.
    pub fn post_event<F: FnOnce(&mut EventLoop) + 'static>(&mut self, f: F) {
        self.posted.push_back(Box::new(f));
        wake(self.wake_fd);
    }

    /// Marca el bucle para terminar y lo despierta.
    pub fn quit(&mut self) {
        self.running = false;
        wake(self.wake_fd);
    }

    /// Ejecuta el bucle hasta que algún manejador llame a `quit`.
    pub fn run(&mut self) -> Result<()> {
        self.running = true;
        let mut events: Vec<libc::epoll_event> =
            vec![libc::epoll_event { events: 0, u64: 0 }; self.max_events];
        while self.running {
            // Si hay eventos posteados pendientes no bloqueamos: hay trabajo.
            let timeout = if self.posted.is_empty() {
                self.timeout_ms
            } else {
                0
            };
            // SAFETY: buffer y tamaño coherentes; comprobamos el resultado.
            let n = unsafe {
                libc::epoll_wait(
                    self.epfd,
                    events.as_mut_ptr(),
                    events.len() as libc::c_int,
                    timeout,
                )
            };
            if n < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    continue; // EINTR: reintenta la espera.
                }
                return Err(FoundationError::event_loop(
                    EventLoopErrorKind::WaitFailed,
                    Some(err),
                ));
            }
            for ev in &events[..n as usize] {
                let ev = *ev;
                self.dispatch(ev.u64 as RawFd, ev.events);
            }
            self.run_posted();
        }
        Ok(())
    }

    fn run_posted(&mut self) {
        loop {
            let mut batch = std::mem::take(&mut self.posted);
            if batch.is_empty() {
                return;
            }
            // Los clousures pueden postear más eventos (se re-itera).
            while let Some(f) = batch.pop_front() {
                f(self);
            }
        }
    }

    fn dispatch(&mut self, fd: RawFd, flags: u32) {
        if fd == self.wake_fd {
            drain_wake(self.wake_fd);
        } else if self.timers.contains_key(&fd) {
            self.fire_timer(fd);
        } else if self.sig_fd == Some(fd) {
            self.deliver_signals();
        } else if self.fds.contains_key(&fd) {
            self.deliver_fd(fd, flags);
        } else {
            // Descriptor no registrado (p. ej. fd cerrado sin unregister): ignorar.
        }
    }

    fn fire_timer(&mut self, fd: RawFd) {
        let record = self
            .timers
            .get(&fd)
            .expect("temporizador registrado")
            .clone();
        // Drena el contador de expiraciones del timerfd.
        let mut expirations = [0u8; 8];
        // SAFETY: lectura de 8 bytes en un buffer válido del mismo tamaño.
        unsafe {
            libc::read(fd, expirations.as_mut_ptr().cast(), expirations.len());
        }
        if record.period.is_none() {
            // Único disparo: retirar y cerrar antes de invocar al manejador.
            self.timers.remove(&fd);
            self.timer_ids.remove(&record.id);
            // SAFETY: fd de temporizador creado por nosotros.
            unsafe { libc::close(fd) };
        }
        let handler = record.handler.clone();
        let wake_fd = self.wake_fd;
        let posted = &mut self.posted;
        let mut dispatcher = Dispatcher { posted, wake_fd };
        call_handler("timer", || (handler.borrow_mut())(&mut dispatcher));
    }

    fn deliver_fd(&mut self, fd: RawFd, flags: u32) {
        let handler = self.fds.get(&fd).expect("descriptor registrado").clone();
        let wake_fd = self.wake_fd;
        let posted = &mut self.posted;
        let mut dispatcher = Dispatcher { posted, wake_fd };
        call_handler("fd", || (handler.borrow_mut())(fd, flags, &mut dispatcher));
    }

    fn deliver_signals(&mut self) {
        // signalfd_siginfo ocupa 128 bytes por señal pendiente.
        let mut buf = [0u8; 128 * 8];
        // SAFETY: lectura en un buffer propio del tamaño indicado.
        let n = unsafe { libc::read(self.sig_fd.unwrap(), buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            return; // EAGAIN o error: nada que despachar.
        }
        for info in buf[..n as usize].chunks_exact(128) {
            let signo =
                i32::from_le_bytes(info[0..4].try_into().expect("signalfd_siginfo: 4 bytes"));
            let Some(handler) = self.sig_handlers.get(&signo).cloned() else {
                continue;
            };
            let wake_fd = self.wake_fd;
            let posted = &mut self.posted;
            let mut dispatcher = Dispatcher { posted, wake_fd };
            call_handler("signal", || (handler.borrow_mut())(signo, &mut dispatcher));
        }
    }

    fn apply_signal_mask(&mut self) -> Result<()> {
        // SAFETY: sigset_t inicializado con sigemptyset antes de usarse.
        let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
        // SAFETY: puntero a sigset_t válido.
        unsafe { libc::sigemptyset(&mut mask) };
        for &signum in self.sig_handlers.keys() {
            // SAFETY: signum > 0 garantizado por el llamador; comprobamos resultado.
            if unsafe { libc::sigaddset(&mut mask, signum) } != 0 {
                return Err(FoundationError::event_loop(
                    EventLoopErrorKind::SignalSetupFailed,
                    Some(io::Error::last_os_error()),
                ));
            }
        }
        if self.sig_handlers.is_empty() {
            // Sin señales vigiladas: cerrar signalfd y restaurar la máscara.
            if let Some(fd) = self.sig_fd.take() {
                // SAFETY: fd de signalfd creado por nosotros.
                unsafe { libc::close(fd) };
            }
            if let Some(original) = self.original_mask.take() {
                // SAFETY: puntero a sigset_t guardado previamente.
                unsafe { libc::sigprocmask(libc::SIG_SETMASK, &original, std::ptr::null_mut()) };
            }
            return Ok(());
        }
        // Bloquea exactamente el conjunto vigilado (hilo actual) y guarda la
        // máscara original solo la primera vez.
        // SAFETY: punteros a sigset_t válidos.
        let mut old: libc::sigset_t = unsafe { std::mem::zeroed() };
        if unsafe { libc::sigprocmask(libc::SIG_SETMASK, &mask, &mut old) } != 0 {
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::SignalSetupFailed,
                Some(io::Error::last_os_error()),
            ));
        }
        if self.original_mask.is_none() {
            self.original_mask = Some(old);
        }
        let flags = libc::SFD_CLOEXEC | libc::SFD_NONBLOCK;
        let fd = match self.sig_fd {
            // SAFETY: syscall con argumentos válidos; actualiza la máscara.
            Some(existing) => unsafe { libc::signalfd(existing, &mask, flags) },
            // SAFETY: syscall con argumentos válidos; crea el descriptor.
            None => unsafe { libc::signalfd(-1, &mask, flags) },
        };
        if fd < 0 {
            return Err(FoundationError::event_loop(
                EventLoopErrorKind::SignalSetupFailed,
                Some(io::Error::last_os_error()),
            ));
        }
        if self.sig_fd.is_none() {
            self.sig_fd = Some(fd);
            let mut ev = libc::epoll_event {
                events: libc::EPOLLIN as u32,
                u64: fd as u64,
            };
            // SAFETY: syscall con argumentos válidos.
            if unsafe { libc::epoll_ctl(self.epfd, libc::EPOLL_CTL_ADD, fd, &mut ev) } != 0 {
                return Err(FoundationError::event_loop(
                    EventLoopErrorKind::SignalSetupFailed,
                    Some(io::Error::last_os_error()),
                ));
            }
        }
        Ok(())
    }
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        // Restaura la máscara de señales del hilo antes de cerrar nada.
        if let Some(original) = self.original_mask.take() {
            // SAFETY: puntero a sigset_t guardado en apply_signal_mask.
            unsafe { libc::sigprocmask(libc::SIG_SETMASK, &original, std::ptr::null_mut()) };
        }
        // Cierra solo los fds que son propiedad del bucle.
        for fd in self.timers.keys().copied().collect::<Vec<_>>() {
            // SAFETY: fds de temporizador creados por nosotros.
            unsafe { libc::close(fd) };
        }
        if let Some(fd) = self.sig_fd.take() {
            // SAFETY: fd de signalfd creado por nosotros.
            unsafe { libc::close(fd) };
        }
        // SAFETY: fds creados por nosotros en `new`.
        unsafe {
            libc::close(self.wake_fd);
            libc::close(self.epfd);
        }
    }
}

/// Un error dentro de un manejador se registra y no detiene el bucle (§6).
fn call_handler(what: &str, f: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(f)).is_err() {
        eprintln!("[ERROR] [event_loop] el manejador de {what} hizo panic; el bucle continúa");
    }
}

fn wake(fd: RawFd) {
    let one: u64 = 1;
    // SAFETY: escritura de 8 bytes en un eventfd abierto; errores se ignoran
    // (si fallara, el post se procesaría en la próxima iteración con timeout 0).
    unsafe {
        let _ = libc::write(fd, (&one as *const u64).cast(), 8);
    }
}

fn drain_wake(fd: RawFd) {
    let mut buf = [0u8; 8];
    // SAFETY: lectura en buffer propio del tamaño indicado.
    unsafe {
        let _ = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
    }
}

fn duration_to_timespec(d: Duration) -> libc::timespec {
    debug_assert!(d > Duration::ZERO);
    libc::timespec {
        tv_sec: d.as_secs() as libc::time_t,
        tv_nsec: d.subsec_nanos() as libc::c_long,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn pipe() -> (RawFd, RawFd) {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: buffer de 2 fds para pipe2; comprobamos el resultado.
        assert_eq!(
            unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) },
            0
        );
        (fds[0], fds[1])
    }

    fn write_byte(fd: RawFd) {
        // SAFETY: fd de escritura del pipe abierto por el test.
        unsafe {
            assert!(libc::write(fd, b"x".as_ptr().cast(), 1) >= 0);
        }
    }

    #[test]
    fn fd_legible_despierta_el_bucle() {
        let (r, w) = pipe();
        let mut el = EventLoop::new(None, 64).unwrap();
        let called = Rc::new(AtomicBool::new(false));
        let called2 = called.clone();
        el.register_fd(
            r,
            interest::READABLE,
            Box::new(move |_fd, _flags, d| {
                called2.store(true, Ordering::SeqCst);
                d.quit();
            }),
        )
        .unwrap();
        write_byte(w);
        el.run().unwrap();
        assert!(called.load(Ordering::SeqCst));
        // SAFETY: fds del test; el cierre es manual (EventLoop no es dueño).
        unsafe {
            libc::close(r);
            libc::close(w);
        }
    }

    #[test]
    fn post_event_ejecuta_y_despierta() {
        let mut el = EventLoop::new(None, 64).unwrap();
        let hit = Rc::new(AtomicBool::new(false));
        let hit2 = hit.clone();
        el.post_event(move |el| {
            hit2.store(true, Ordering::SeqCst);
            el.quit();
        });
        el.run().unwrap();
        assert!(hit.load(Ordering::SeqCst));
    }

    #[test]
    fn temporizador_unico_disparo_termina_el_bucle() {
        let mut el = EventLoop::new(None, 64).unwrap();
        let fired = Rc::new(AtomicBool::new(false));
        let fired2 = fired.clone();
        el.add_timer(
            Duration::from_millis(5),
            None,
            Box::new(move |d: &mut Dispatcher<'_>| {
                fired2.store(true, Ordering::SeqCst);
                d.quit();
            }),
        )
        .unwrap();
        el.run().unwrap();
        assert!(fired.load(Ordering::SeqCst));
    }

    #[test]
    fn temporizador_periodico_y_cancelacion() {
        let mut el = EventLoop::new(None, 64).unwrap();
        let ticks = Rc::new(Cell::new(0u32));
        let ticks2 = ticks.clone();
        let periodic = el
            .add_timer(
                Duration::from_millis(5),
                Some(Duration::from_millis(5)),
                Box::new(move |_d: &mut Dispatcher<'_>| {
                    ticks2.set(ticks2.get() + 1);
                }),
            )
            .unwrap();
        // Cancelado antes de dispararse: nunca debe ejecutarse.
        let cancelled = el
            .add_timer(
                Duration::from_millis(10),
                None,
                Box::new(|_d: &mut Dispatcher<'_>| panic!("temporizador cancelado disparó")),
            )
            .unwrap();
        el.cancel_timer(cancelled).unwrap();
        el.add_timer(
            Duration::from_millis(30),
            None,
            Box::new(move |d: &mut Dispatcher<'_>| {
                let p = periodic;
                d.post_event(move |el| {
                    el.cancel_timer(p).unwrap();
                    el.quit();
                });
            }),
        )
        .unwrap();
        el.run().unwrap();
        assert!(ticks.get() >= 2, "debe recibir varios disparos periódicos");
    }

    #[test]
    fn senal_entregada_por_signalfd() {
        let mut el = EventLoop::new(None, 64).unwrap();
        let got = Rc::new(AtomicBool::new(false));
        let got2 = got.clone();
        el.add_signal(
            libc::SIGUSR2,
            Box::new(move |signo, d| {
                assert_eq!(signo, libc::SIGUSR2);
                got2.store(true, Ordering::SeqCst);
                d.quit();
            }),
        )
        .unwrap();
        // SAFETY: raise entrega la señal al hilo que la invoca; ahí está
        // bloqueada y pendiente para el signalfd de este bucle.
        unsafe {
            libc::raise(libc::SIGUSR2);
        }
        el.post_event(|el| el.quit());
        el.run().unwrap();
        assert!(got.load(Ordering::SeqCst));
    }
}
