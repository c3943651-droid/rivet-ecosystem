//! Buffer de píxeles en memoria compartida (`memfd` + `mmap`).
//!
//! Parte CPU pura de `wl_shm` (ADR-02: v1 usa `wl_shm`; hay una copia por
//! frame). El pool Wayland (`WlShmPool`/`WlBuffer`) vive en [`crate::state`];
//! este tipo solo gestiona la memoria: `slots` frames contiguos.

use std::ffi::CString;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};

/// Bloque de píxeles BGRA con memoria compartida vía `memfd`, con N *slots*
/// de frame para multibuffering.
pub struct ShmPixels {
    fd: OwnedFd,
    ptr: *mut u8,
    mapping_len: usize,
    width: u32,
    height: u32,
    stride: usize,
    slots: usize,
}

impl ShmPixels {
    /// Reserva `slots` frames de `width × height` píxeles BGRA.
    ///
    /// # Panics
    ///
    /// Si las dimensiones son 0 o falla `memfd`/`mmap`/`ftruncate`.
    pub fn new(width: u32, height: u32, slots: usize) -> Self {
        assert!(width > 0 && height > 0, "ShmPixels: dimensiones vacías");
        assert!(slots > 0, "ShmPixels: al menos un slot");
        let mut this = Self {
            fd: memfd().expect("memfd_create disponible en Linux >= 3.17"),
            ptr: std::ptr::null_mut(),
            mapping_len: 0,
            width,
            height,
            stride: width as usize * 4,
            slots,
        };
        this.reallocate();
        this
    }

    /// Remapea el memfd al tamaño actual de los campos.
    fn reallocate(&mut self) {
        let len = self.mapping_len_total();
        // SAFETY: ftruncate sobre un fd propio con tamaño válido; mmap RW
        // del mismo fd; comprobamos ambos resultados y el mapping se
        // deshace en Drop con munmap.
        unsafe {
            if libc::ftruncate(self.fd.as_raw_fd(), len as libc::off_t) != 0 {
                panic!("ftruncate del shm falló");
            }
            if !self.ptr.is_null() && libc::munmap(self.ptr.cast(), self.mapping_len) != 0 {
                panic!("munmap del shm falló");
            }
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                self.fd.as_raw_fd(),
                0,
            );
            if ptr == libc::MAP_FAILED || ptr.is_null() {
                panic!("mmap del shm falló");
            }
            self.ptr = ptr.cast();
            self.mapping_len = len;
        }
    }

    /// Cambia el tamaño de los frames (remapea; los píxeles previos se pierden).
    pub fn resize(&mut self, width: u32, height: u32) {
        assert!(width > 0 && height > 0, "ShmPixels: dimensiones vacías");
        self.width = width;
        self.height = height;
        self.stride = width as usize * 4;
        self.reallocate();
    }

    /// Bytes de un frame individual.
    #[must_use]
    pub fn frame_len(&self) -> usize {
        self.stride * self.height as usize
    }

    /// Bytes totales del mapping (`slots × frame_len`).
    #[must_use]
    pub fn mapping_len_total(&self) -> usize {
        self.frame_len() * self.slots
    }

    /// Número de slots.
    #[must_use]
    pub fn slots(&self) -> usize {
        self.slots
    }

    /// FD prestado para `wl_shm.create_pool`.
    #[must_use]
    pub fn borrowed_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    /// Ancho en píxeles de cada frame.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Alto en píxeles de cada frame.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Bytes por fila.
    #[must_use]
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Acceso mutable al frame completo de un slot.
    ///
    /// # Panics
    ///
    /// Si `slot >= slots()`.
    pub fn slot_slice_mut(&mut self, slot: usize) -> &mut [u8] {
        assert!(slot < self.slots, "slot {slot} fuera de rango");
        let start = slot * self.frame_len();
        // SAFETY: mapping válido de `mapping_len` bytes durante el borrow.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.add(start), self.frame_len()) }
    }

    /// Copia píxeles BGRA al frame del slot indicado.
    ///
    /// # Panics
    ///
    /// Si `bgra` no tiene `frame_len()` bytes o el slot no existe.
    pub fn copy_bgra(&mut self, slot: usize, bgra: &[u8]) {
        let expected = self.frame_len();
        assert_eq!(
            bgra.len(),
            expected,
            "copy_bgra: se esperaban {expected} bytes"
        );
        self.slot_slice_mut(slot).copy_from_slice(bgra);
    }
}

impl Drop for ShmPixels {
    fn drop(&mut self) {
        // SAFETY: puntero y longitud provienen de nuestro propio mmap.
        unsafe {
            libc::munmap(self.ptr.cast(), self.mapping_len);
        }
    }
}

fn memfd() -> std::io::Result<OwnedFd> {
    let name = CString::new("rivet-shm").expect("CString sin NUL");
    // SAFETY: syscall con nombre válido; el fd se envuelve en OwnedFd.
    let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: fd recién creado y sin dueño.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mmap_lectura_escritura() {
        let mut pixels = ShmPixels::new(4, 3, 1);
        assert_eq!(pixels.frame_len(), 4 * 4 * 3);
        assert_eq!(pixels.stride(), 16);
        pixels.copy_bgra(0, &[0xABu8; 48]);
        pixels.copy_bgra(0, &[0xCDu8; 48]);
    }

    #[test]
    fn slots_independientes() {
        let mut pixels = ShmPixels::new(2, 1, 2);
        assert_eq!(pixels.mapping_len_total(), 16);
        pixels.copy_bgra(0, &[1, 2, 3, 4, 5, 6, 7, 8]);
        pixels.copy_bgra(1, &[9, 9, 9, 9, 8, 8, 8, 8]);
        assert_eq!(pixels.slot_slice_mut(0), &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(pixels.slot_slice_mut(1), &[9, 9, 9, 9, 8, 8, 8, 8]);
    }

    #[test]
    fn resize_cambia_longitud() {
        let mut pixels = ShmPixels::new(8, 8, 2);
        assert_eq!(pixels.mapping_len_total(), 8 * 8 * 4 * 2);
        pixels.resize(16, 2);
        assert_eq!(pixels.mapping_len_total(), 16 * 2 * 4 * 2);
        pixels.copy_bgra(1, &[0u8; 128]);
    }
}
