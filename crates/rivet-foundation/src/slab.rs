//! Slab allocator with generations and 64-bit IDs.
//!
//! Object store of the ecosystem (`docs/rivet-foundation.md` §3):
//! flat contiguous memory, O(1) access by index, LIFO reuse of
//! slots via `free_list` and stale-ID invalidation by generation.

use crate::utils::ids::{id_generation, id_index, make_id};

#[derive(Debug, Clone)]
struct Entry<T> {
    generation: u32,
    value: Option<T>,
}

/// Slab con generaciones: `insert` devuelve un ID de 64 bits
/// (32 bits de índice + 32 de generación) válido hasta que el valor se
/// elimina y el slot se reutiliza.
///
/// Reglas cerradas:
/// - `remove` **no** incrementa la generación; el `insert` que reutiliza el
///   slot **sí** la incrementa → los IDs viejos quedan invalidados
///   (evita *use-after-free* silencioso).
/// - `insert`/`get`/`get_mut`/`remove` son O(1); la iteración es O(n).
#[derive(Debug, Clone)]
pub struct Slab<T> {
    entries: Vec<Entry<T>>,
    free_list: Vec<usize>,
    len: usize,
}

impl<T> Slab<T> {
    /// Crea un slab vacío.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            free_list: Vec::new(),
            len: 0,
        }
    }

    /// Crea un slab vacío con capacidad preasignada para `capacity` entradas.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            free_list: Vec::new(),
            len: 0,
        }
    }

    /// Inserta un valor y devuelve su ID.
    ///
    /// # Panics
    ///
    /// Si el slab supera `u32::MAX` entradas (el índice no cabría en el ID).
    pub fn insert(&mut self, value: T) -> u64 {
        let index = if let Some(index) = self.free_list.pop() {
            // Reutilización de slot: la generación avanza e invalida IDs viejos.
            let entry = &mut self.entries[index];
            entry.generation = entry.generation.wrapping_add(1);
            entry.value = Some(value);
            index
        } else {
            let index = self.entries.len();
            assert!(
                index <= u32::MAX as usize,
                "slab: índice {index} excede los 32 bits del ID"
            );
            self.entries.push(Entry {
                generation: 0,
                value: Some(value),
            });
            index
        };
        self.len += 1;
        let generation = self.entries[index].generation;
        make_id(generation, index as u32)
    }

    /// Devuelve una referencia al valor si el ID es válido.
    ///
    /// ID inválido = índice fuera de rango, generación que no coincide o
    /// slot vacío.
    pub fn get(&self, id: u64) -> Option<&T> {
        self.entry(id)
            .map(|e| e.value.as_ref().expect("slot ocupado"))
    }

    /// Devuelve una referencia mutable al valor si el ID es válido.
    pub fn get_mut(&mut self, id: u64) -> Option<&mut T> {
        self.entry_mut(id)
            .map(|e| e.value.as_mut().expect("slot ocupado"))
    }

    /// Elimina el valor asociado al ID y lo devuelve.
    ///
    /// La generación **no** avanza aquí; avanza cuando el slot se reutilice.
    /// Devuelve `None` si el ID es inválido.
    pub fn remove(&mut self, id: u64) -> Option<T> {
        let index = id_index(id) as usize;
        if index >= self.entries.len() {
            return None;
        }
        let entry = &mut self.entries[index];
        if entry.generation != id_generation(id) {
            return None;
        }
        let value = entry.value.take();
        if value.is_some() {
            self.free_list.push(index); // LIFO: conserva localidad.
            self.len -= 1;
        }
        value
    }

    /// Elimina todos los valores. Los IDs emitidos antes quedan invalidados
    /// (la generación arranca de 0 al reinsertar en un slab vacío, pero el
    /// `len` vuelve a 0 y todo índice previo queda libre).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.free_list.clear();
        self.len = 0;
    }

    /// Número de valores almacenados.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` si no hay valores almacenados.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Capacidad actual del vector de entradas.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.entries.len()
    }

    /// Itera sobre los valores ocupados (O(n)).
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().filter_map(|e| e.value.as_ref())
    }

    /// Itera mutablemente sobre los valores ocupados (O(n)).
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.entries.iter_mut().filter_map(|e| e.value.as_mut())
    }

    fn entry(&self, id: u64) -> Option<&Entry<T>> {
        let index = id_index(id) as usize;
        let entry = self.entries.get(index)?;
        // La generación debe coincidir y el slot debe estar ocupado.
        (entry.generation == id_generation(id) && entry.value.is_some()).then_some(entry)
    }

    fn entry_mut(&mut self, id: u64) -> Option<&mut Entry<T>> {
        let index = id_index(id) as usize;
        let entry = self.entries.get_mut(index)?;
        (entry.generation == id_generation(id) && entry.value.is_some()).then_some(entry)
    }
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> IntoIterator for &'a Slab<T> {
    type Item = &'a T;
    type IntoIter = Box<dyn Iterator<Item = &'a T> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove_basico() {
        let mut slab = Slab::new();
        let id = slab.insert(10u32);
        assert_eq!(slab.len(), 1);
        assert_eq!(slab.get(id), Some(&10));
        assert_eq!(slab.get_mut(id), Some(&mut 10));
        assert_eq!(slab.remove(id), Some(10));
        assert_eq!(slab.remove(id), None);
        assert_eq!(slab.get(id), None);
        assert!(slab.is_empty());
    }

    #[test]
    fn id_invalidado_tras_reutilizacion() {
        let mut slab = Slab::new();
        let id_viejo = slab.insert("a");
        assert_eq!(slab.remove(id_viejo), Some("a"));

        // El slot se reutiliza y la generación avanza: el ID viejo muere.
        let id_nuevo = slab.insert("b");
        assert_eq!(id_index(id_nuevo), id_index(id_viejo));
        assert_ne!(id_generation(id_nuevo), id_generation(id_viejo));
        assert_eq!(slab.get(id_viejo), None);
        assert_eq!(slab.get(id_nuevo), Some(&"b"));
    }

    #[test]
    fn ids_unicos_mientras_viven() {
        let mut slab = Slab::new();
        let a = slab.insert(1);
        let b = slab.insert(2);
        assert_ne!(a, b);
        assert_eq!(slab.get(a), Some(&1));
        assert_eq!(slab.get(b), Some(&2));
    }

    #[test]
    fn remove_no_avanza_generacion() {
        let mut slab = Slab::new();
        let id = slab.insert(1);
        slab.remove(id);
        // Mismo índice, misma generación pero slot vacío → inválido.
        assert_eq!(slab.get(id), None);
        let id2 = slab.insert(2);
        assert_eq!(id_generation(id2), id_generation(id) + 1);
    }

    #[test]
    fn iteracion_solo_ocupados() {
        let mut slab = Slab::with_capacity(8);
        let ids: Vec<u64> = (0..5).map(|i| slab.insert(i * 10)).collect();
        slab.remove(ids[2]);
        let mut valores: Vec<i32> = slab.iter().copied().collect();
        valores.sort_unstable();
        assert_eq!(valores, [0, 10, 30, 40]);
        assert_eq!(slab.len(), 4);

        for v in slab.iter_mut() {
            *v += 1;
        }
        assert_eq!(slab.get(ids[0]), Some(&1));
    }

    #[test]
    fn clear_vacia_todo() {
        let mut slab = Slab::new();
        let ids: Vec<u64> = (0..3).map(|_| slab.insert(())).collect();
        slab.clear();
        assert!(slab.is_empty());
        for id in ids {
            assert_eq!(slab.get(id), None);
            assert_eq!(slab.remove(id), None);
        }
        assert_eq!(slab.insert(()), 0); // gen 0, índice 0 tras clear.
    }

    #[test]
    fn reutilizacion_lifo_conserva_localidad() {
        let mut slab = Slab::new();
        let a = slab.insert(1);
        let b = slab.insert(2);
        slab.remove(a);
        slab.remove(b);
        let c = slab.insert(3);
        // El último liberado (b) es el primero en reutilizarse.
        assert_eq!(id_index(c), id_index(b));
        assert_eq!(slab.get(c), Some(&3));
    }
}
