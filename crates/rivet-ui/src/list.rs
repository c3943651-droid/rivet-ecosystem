//! `ListView`: patrón Modelo-Vista para listas (v0.3, auditoría §12).
//!
//! Mantén el **modelo** fuera (Vec propio o `Ui::insert_state`) y sincroniza
//! con [`ListView::set_items`]: los widgets de los ítems se **reutilizan**
//! mientras sigan vivos (sin churn de Slab/closures por actualización);
//! solo se crean los que faltan y se destruyen los sobrantes.

use crate::error::Result;
use crate::registry::WidgetRegistry;
use crate::widget::WidgetId;

/// Lista sincronizada con un modelo externo.
///
/// ```ignore
/// let mut list = ListView::new(&mut ui.registry, true);
/// ui.registry.add_child(root, list.container()).unwrap();
/// // Al cambiar el modelo:
/// list.set_items(&mut ui.registry, notas.len(), notas.len(),
///     |reg, i| reg.create_label(""),
///     |reg, id, i| reg.set_label_text(id, notas[i].titulo.clone()).unwrap());
/// ```
pub struct ListView {
    container: WidgetId,
    items: Vec<WidgetId>,
}

impl ListView {
    /// Crea la lista: un contenedor `Box` propio que se engancha al árbol
    /// como cualquier widget.
    #[must_use]
    pub fn new(registry: &mut WidgetRegistry, vertical: bool) -> Self {
        Self {
            container: registry.create_box(vertical),
            items: Vec::new(),
        }
    }

    /// Contenedor subyacente (para `add_child` al árbol).
    #[must_use]
    pub fn container(&self) -> WidgetId {
        self.container
    }

    /// Número de ítems sincronizados actualmente.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` sin ítems.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Sincroniza la vista con `count` ítems del modelo.
    ///
    /// * `build`: crea el widget del índice `i` (solo cuando no hay
    ///   reusable vivo). El widget no debe tener padre (la lista lo engancha).
    /// * `update`: reescribe el widget `id` con los datos del índice `i`
    ///   (también con los recién creados).
    ///
    /// Los ítems sobrantes se destruyen; los widgets reutilizados **no se
    /// recrean** (mismo `WidgetId`): sin churn de slabs ni callbacks.
    pub fn set_items<B, U>(
        &mut self,
        registry: &mut WidgetRegistry,
        count: usize,
        build: B,
        update: U,
    ) -> Result<()>
    where
        B: FnMut(&mut WidgetRegistry, usize) -> WidgetId,
        U: FnMut(&mut WidgetRegistry, WidgetId, usize),
    {
        let mut build = build;
        let mut update = update;
        for index in 0..count {
            let reusable = self
                .items
                .get(index)
                .copied()
                .filter(|&id| registry.is_alive(id));
            let id = match reusable {
                Some(id) => id,
                None => {
                    let id = build(registry, index);
                    // El índice puede superar len (lista creciendo):
                    // rellena el hueco insertando al final del contenedor.
                    self.items.insert(index.min(self.items.len()), id);
                    registry.add_child(self.container, id)?;
                    id
                }
            };
            update(registry, id, index);
        }
        // Sobrantes fuera del modelo: destruir y recortar.
        if count < self.items.len() {
            for &id in &self.items[count..] {
                if registry.is_alive(id) {
                    registry.destroy_subtree(id)?;
                }
            }
            self.items.truncate(count);
        }
        if let Ok(node) = registry.node_mut(self.container) {
            node.dirty = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reutiliza_widgets_y_recorta_sobrantes() {
        let mut reg = WidgetRegistry::new();
        let mut list = ListView::new(&mut reg, true);
        assert!(list.is_empty());
        list.set_items(
            &mut reg,
            3,
            |reg, i| reg.create_button(format!("item {i}"), |_| {}),
            |reg, id, i| {
                reg.set_button_text(id, format!("título {i}")).unwrap();
            },
        )
        .unwrap();
        let first: Vec<WidgetId> = list.items.clone();
        assert_eq!(list.len(), 3);
        // Misma cuenta: mismos IDs (sin recrear).
        list.set_items(
            &mut reg,
            3,
            |reg, i| reg.create_button(format!("item {i}"), |_| {}),
            |reg, id, i| {
                reg.set_button_text(id, format!("nuevo {i}")).unwrap();
            },
        )
        .unwrap();
        assert_eq!(list.items, first, "widgets reutilizados");
        assert_eq!(
            reg.button_data(first[1]).unwrap().text,
            "nuevo 1",
            "update reescribió el reusable"
        );
        // Encoger destruye el sobrante.
        list.set_items(
            &mut reg,
            1,
            |reg, i| reg.create_button(format!("item {i}"), |_| {}),
            |reg, id, i| {
                reg.set_button_text(id, format!("x {i}")).unwrap();
            },
        )
        .unwrap();
        assert_eq!(list.len(), 1);
        assert!(!reg.is_alive(first[2]), "sobrante destruido");
        assert!(reg.is_alive(first[0]), "reutilizado vivo");
        // Crecer de nuevo reutiliza el índice 0 y crea el 1.
        list.set_items(
            &mut reg,
            2,
            |reg, i| reg.create_button(format!("item {i}"), |_| {}),
            |reg, id, i| {
                reg.set_button_text(id, format!("y {i}")).unwrap();
            },
        )
        .unwrap();
        assert_eq!(list.items[0], first[0], "índice 0 sigue reutilizado");
        assert_eq!(
            reg.button_data(list.items[1]).unwrap().text,
            "y 1",
            "índice 1 creado y actualizado"
        );
    }
}
