//! `WidgetRegistry`: creation and tree authority (ADR-03/04).
//!
//! Common node in a foundation slab; **one slab per kind** for
//! kind-specific data (no *padding* from the largest widget). Children
//! via O(1) intrusive links.

use rivet_base::slab::Slab;
use rivet_canvas::Rect;

use crate::error::{Result, UiError};
use crate::widget::{state, WidgetId, WidgetKind, WidgetNode};

/// Datos específicos de `Box`/`Panel`.
#[derive(Debug, Clone)]
pub struct ContainerData {
    /// `true` apila verticalmente; `false` horizontalmente.
    pub vertical: bool,
    /// Espacio entre hijos (px).
    pub spacing: f32,
    /// Relleno interno.
    pub padding: rivet_style::Insets,
}

/// Callback de activación de un botón.
pub type OnClick = Box<dyn FnMut(&mut crate::Ui)>;

/// Datos de `Button`.
pub struct ButtonData {
    /// Texto del botón.
    pub text: String,
    /// Callback de activación (click completado).
    pub on_click: Option<OnClick>,
}

/// Datos de `Label`.
#[derive(Debug, Clone)]
pub struct LabelData {
    /// Texto mostrado.
    pub text: String,
}

/// Datos de `Image`.
#[derive(Debug, Clone)]
pub struct ImageData {
    /// Textura de canvas (se sube al primer dibujo).
    pub texture: Option<rivet_canvas::TextureId>,
    /// Píxeles pendientes de subir (los consume el render).
    pub pending: Option<rivet_image::ImageData>,
}

/// Callback de cambio de contenido de un `TextInput`.
pub type OnChange = Box<dyn FnMut(&str, &mut crate::Ui)>;

/// Datos de `TextInput`.
pub struct TextInputData {
    /// Contenido.
    pub text: String,
    /// Marcador cuando está vacío.
    pub placeholder: String,
    /// Callback de cambio de contenido.
    pub on_change: Option<OnChange>,
}

/// Registro de widgets: nodos comunes + slabs por tipo.
pub struct WidgetRegistry {
    nodes: Slab<WidgetNode>,
    containers: Slab<ContainerData>,
    buttons: Slab<ButtonData>,
    labels: Slab<LabelData>,
    images: Slab<ImageData>,
    inputs: Slab<TextInputData>,
    /// Id de datos por nodo (índice del slab del tipo).
    data: Slab<u64>,
    root: Option<WidgetId>,
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetRegistry {
    /// Registro vacío.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Slab::new(),
            containers: Slab::new(),
            buttons: Slab::new(),
            labels: Slab::new(),
            images: Slab::new(),
            inputs: Slab::new(),
            data: Slab::new(),
            root: None,
        }
    }

    /// Widget raíz (típicamente un `Box`/`Panel`).
    #[must_use]
    pub fn root(&self) -> Option<WidgetId> {
        self.root
    }

    /// Fija el widget raíz (debe existir y no tener padre).
    pub fn set_root(&mut self, id: WidgetId) -> Result<()> {
        let node = self.node(id)?;
        if node.parent.is_some() {
            return Err(UiError::InvalidTree("la raíz no puede tener padre".into()));
        }
        self.root = Some(id);
        Ok(())
    }

    /// Nodo común de un widget (validado por generación).
    pub fn node(&self, id: WidgetId) -> Result<&WidgetNode> {
        self.nodes.get(id).ok_or(UiError::InvalidWidget(id))
    }

    /// Nodo mutable.
    pub fn node_mut(&mut self, id: WidgetId) -> Result<&mut WidgetNode> {
        self.nodes.get_mut(id).ok_or(UiError::InvalidWidget(id))
    }

    /// `true` si el widget existe (ID vigente).
    #[must_use]
    pub fn is_alive(&self, id: WidgetId) -> bool {
        self.nodes.get(id).is_some()
    }

    /// Widgets vivos.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` sin widgets.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn insert_node(&mut self, mut node: WidgetNode) -> WidgetId {
        node.state |= state::VISIBLE;
        self.data.insert(0);
        self.nodes.insert(node)
    }

    // ------------------------------------------------------------------
    // Creación de widgets.
    // ------------------------------------------------------------------

    /// Crea un `Box` contenedor.
    pub fn create_box(&mut self, vertical: bool) -> WidgetId {
        let id = self.insert_node(WidgetNode {
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
            kind: WidgetKind::Box,
            state: 0,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            dirty: true,
        });
        self.store_data(
            id,
            DataSlot::Container(ContainerData {
                vertical,
                spacing: 6.0,
                padding: rivet_style::Insets::all(6.0),
            }),
        );
        id
    }

    /// Crea un `Panel` (contenedor con borde).
    pub fn create_panel(&mut self, vertical: bool) -> WidgetId {
        let id = self.insert_node(WidgetNode {
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
            kind: WidgetKind::Panel,
            state: 0,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            dirty: true,
        });
        self.store_data(
            id,
            DataSlot::Container(ContainerData {
                vertical,
                spacing: 6.0,
                padding: rivet_style::Insets::all(10.0),
            }),
        );
        id
    }

    /// Crea un `Button` con texto y callback.
    pub fn create_button<F>(&mut self, text: impl Into<String>, on_click: F) -> WidgetId
    where
        F: FnMut(&mut crate::Ui) + 'static,
    {
        let id = self.insert_node(WidgetNode {
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
            kind: WidgetKind::Button,
            state: 0,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            dirty: true,
        });
        self.store_data(
            id,
            DataSlot::Button(ButtonData {
                text: text.into(),
                on_click: Some(Box::new(on_click)),
            }),
        );
        id
    }

    /// Crea un `Label`.
    pub fn create_label(&mut self, text: impl Into<String>) -> WidgetId {
        let id = self.insert_node(WidgetNode {
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
            kind: WidgetKind::Label,
            state: 0,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            dirty: true,
        });
        self.store_data(id, DataSlot::Label(LabelData { text: text.into() }));
        id
    }

    /// Crea una `Image` (píxeles pendientes; se suben al primer render).
    pub fn create_image(&mut self, image: rivet_image::ImageData) -> WidgetId {
        let id = self.insert_node(WidgetNode {
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
            kind: WidgetKind::Image,
            state: 0,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            dirty: true,
        });
        self.store_data(
            id,
            DataSlot::Image(ImageData {
                texture: None,
                pending: Some(image),
            }),
        );
        id
    }

    /// Crea un `TextInput`.
    pub fn create_text_input(&mut self, placeholder: impl Into<String>) -> WidgetId {
        let id = self.insert_node(WidgetNode {
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
            kind: WidgetKind::TextInput,
            state: 0,
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            dirty: true,
        });
        self.store_data(
            id,
            DataSlot::Input(TextInputData {
                text: String::new(),
                placeholder: placeholder.into(),
                on_change: None,
            }),
        );
        id
    }

    // ------------------------------------------------------------------
    // Árbol: enlaces intrusivos (ADR-04).
    // ------------------------------------------------------------------

    /// Engancha `child` como último hijo de `parent` (O(1)).
    pub fn add_child(&mut self, parent: WidgetId, child: WidgetId) -> Result<()> {
        if !self.is_alive(parent) {
            return Err(UiError::InvalidWidget(parent));
        }
        if !self.is_alive(child) {
            return Err(UiError::InvalidWidget(child));
        }
        // Ciclo si el HIJO es ancestro del PADRE (o son el mismo).
        if child == parent || self.is_descendant(child, parent) {
            return Err(UiError::InvalidTree(
                "no se puede enganchar un ancestro como descendiente (ciclo)".into(),
            ));
        }
        if self.node(child)?.parent.is_some() {
            return Err(UiError::InvalidTree(format!(
                "el widget {child:#018x} ya tiene padre"
            )));
        }
        self.node_mut(child)?.parent = Some(parent);
        let last = self.node(parent)?.last_child;
        self.node_mut(child)?.prev_sibling = last;
        self.node_mut(child)?.next_sibling = None;
        if let Some(last) = last {
            self.node_mut(last)?.next_sibling = Some(child);
        } else {
            self.node_mut(parent)?.first_child = Some(child);
        }
        self.node_mut(parent)?.last_child = Some(child);
        Ok(())
    }

    /// Desengancha `child` de su padre (O(1)); el widget sigue vivo.
    pub fn remove_child(&mut self, child: WidgetId) -> Result<()> {
        let (parent, prev, next) = {
            let node = self.node(child)?;
            (node.parent, node.prev_sibling, node.next_sibling)
        };
        let Some(parent) = parent else {
            return Ok(()); // sin padre: nada que hacer
        };
        if let Some(prev) = prev {
            self.node_mut(prev)?.next_sibling = next;
        } else {
            self.node_mut(parent)?.first_child = next;
        }
        if let Some(next) = next {
            self.node_mut(next)?.prev_sibling = prev;
        } else {
            self.node_mut(parent)?.last_child = prev;
        }
        let node = self.node_mut(child)?;
        node.parent = None;
        node.prev_sibling = None;
        node.next_sibling = None;
        Ok(())
    }

    fn destroy_impl(&mut self, id: WidgetId) {
        // Recolecta el subárbol en profundidad (pre-orden) con recursión
        // iterativa segura (los IDs no mueren hasta el final).
        let mut order = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if !self.is_alive(current) {
                continue;
            }
            order.push(current);
            let mut child = self.node(current).expect("vivo").first_child;
            let mut kids = Vec::new();
            while let Some(c) = child {
                kids.push(c);
                child = self.node(c).expect("vivo").next_sibling;
            }
            stack.extend(kids.into_iter().rev());
        }
        for target in order {
            let _ = self.remove_child(target);
            let kind = self.node(target).expect("vivo").kind;
            let data_id = self.data.get(target).copied().expect("data presente");
            match kind {
                WidgetKind::Box | WidgetKind::Panel => {
                    self.containers.remove(data_id);
                }
                WidgetKind::Button => {
                    self.buttons.remove(data_id);
                }
                WidgetKind::Label => {
                    self.labels.remove(data_id);
                }
                WidgetKind::Image => {
                    self.images.remove(data_id);
                }
                WidgetKind::TextInput => {
                    self.inputs.remove(data_id);
                }
            }
            self.data.remove(target);
            self.nodes.remove(target);
        }
    }

    /// Destruye un subárbol (API pública sobre `destroy_impl`).
    pub fn destroy_subtree(&mut self, id: WidgetId) -> Result<()> {
        if !self.is_alive(id) {
            return Ok(());
        }
        if self.root == Some(id) {
            self.root = None;
        }
        self.destroy_impl(id);
        Ok(())
    }

    /// `true` si `candidate` es ancestro (o igual) de `id`.
    #[must_use]
    pub fn is_descendant(&self, ancestor: WidgetId, candidate: WidgetId) -> bool {
        let mut current = Some(candidate);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.nodes.get(id).and_then(|n| n.parent);
        }
        false
    }

    /// Hijos en orden (iterador de IDs).
    pub fn children(&self, parent: WidgetId) -> impl Iterator<Item = WidgetId> + '_ {
        let mut next = self
            .nodes
            .get(parent)
            .map(|n| n.first_child)
            .unwrap_or(None);
        std::iter::from_fn(move || {
            let current = next?;
            next = self.nodes.get(current).and_then(|n| n.next_sibling);
            Some(current)
        })
    }

    // ------------------------------------------------------------------
    // Datos por tipo.
    // ------------------------------------------------------------------

    fn store_data(&mut self, id: WidgetId, data: DataSlot) {
        let data_id = match data {
            DataSlot::Container(d) => self.containers.insert(d),
            DataSlot::Button(d) => self.buttons.insert(d),
            DataSlot::Label(d) => self.labels.insert(d),
            DataSlot::Image(d) => self.images.insert(d),
            DataSlot::Input(d) => self.inputs.insert(d),
        };
        let entry = self
            .data
            .get_mut(id)
            .expect("insert_node acaba de crear la entrada");
        *entry = data_id;
    }

    pub(crate) fn container(&self, id: WidgetId) -> Result<&ContainerData> {
        let node = self.node(id)?;
        match node.kind {
            WidgetKind::Box | WidgetKind::Panel => {
                let data = self.data.get(id).copied().unwrap_or(0);
                self.containers.get(data).ok_or(UiError::InvalidWidget(id))
            }
            _ => Err(UiError::WrongKind {
                id,
                expected: "contenedor",
            }),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn container_mut(&mut self, id: WidgetId) -> Result<&mut ContainerData> {
        let kind = self.node(id)?.kind;
        match kind {
            WidgetKind::Box | WidgetKind::Panel => {
                let data = self.data.get(id).copied().unwrap_or(0);
                self.containers
                    .get_mut(data)
                    .ok_or(UiError::InvalidWidget(id))
            }
            _ => Err(UiError::WrongKind {
                id,
                expected: "contenedor",
            }),
        }
    }

    pub(crate) fn button(&self, id: WidgetId) -> Result<&ButtonData> {
        let data = self.data_of(id, WidgetKind::Button)?;
        self.buttons.get(data).ok_or(UiError::InvalidWidget(id))
    }

    pub(crate) fn button_mut(&mut self, id: WidgetId) -> Result<&mut ButtonData> {
        let data = self.data_of(id, WidgetKind::Button)?;
        self.buttons.get_mut(data).ok_or(UiError::InvalidWidget(id))
    }

    pub(crate) fn label(&self, id: WidgetId) -> Result<&LabelData> {
        let data = self.data_of(id, WidgetKind::Label)?;
        self.labels.get(data).ok_or(UiError::InvalidWidget(id))
    }

    #[allow(dead_code)]
    pub(crate) fn label_mut(&mut self, id: WidgetId) -> Result<&mut LabelData> {
        let data = self.data_of(id, WidgetKind::Label)?;
        self.labels.get_mut(data).ok_or(UiError::InvalidWidget(id))
    }

    pub(crate) fn image(&self, id: WidgetId) -> Result<&ImageData> {
        let data = self.data_of(id, WidgetKind::Image)?;
        self.images.get(data).ok_or(UiError::InvalidWidget(id))
    }

    pub(crate) fn image_mut(&mut self, id: WidgetId) -> Result<&mut ImageData> {
        let data = self.data_of(id, WidgetKind::Image)?;
        self.images.get_mut(data).ok_or(UiError::InvalidWidget(id))
    }

    /// Contenido de un `TextInput`.
    pub fn text_input_data(&self, id: WidgetId) -> Result<&TextInputData> {
        self.text_input(id)
    }

    /// Contenido mutable de un `TextInput` (para conectar callbacks).
    pub fn text_input_data_mut(&mut self, id: WidgetId) -> Result<&mut TextInputData> {
        self.text_input_mut(id)
    }

    pub(crate) fn text_input(&self, id: WidgetId) -> Result<&TextInputData> {
        let data = self.data_of(id, WidgetKind::TextInput)?;
        self.inputs.get(data).ok_or(UiError::InvalidWidget(id))
    }

    pub(crate) fn text_input_mut(&mut self, id: WidgetId) -> Result<&mut TextInputData> {
        let data = self.data_of(id, WidgetKind::TextInput)?;
        self.inputs.get_mut(data).ok_or(UiError::InvalidWidget(id))
    }

    fn data_of(&self, id: WidgetId, expected: WidgetKind) -> Result<u64> {
        let node = self.node(id)?;
        if node.kind != expected {
            return Err(UiError::WrongKind {
                id,
                expected: kind_name(expected),
            });
        }
        Ok(self.data.get(id).copied().unwrap_or(0))
    }
}

fn kind_name(kind: WidgetKind) -> &'static str {
    match kind {
        WidgetKind::Box => "Box",
        WidgetKind::Panel => "Panel",
        WidgetKind::Button => "Button",
        WidgetKind::Label => "Label",
        WidgetKind::Image => "Image",
        WidgetKind::TextInput => "TextInput",
    }
}

/// Ranuras de datos por tipo para `store_data` (interno).
enum DataSlot {
    Container(ContainerData),
    Button(ButtonData),
    Label(LabelData),
    Image(ImageData),
    Input(TextInputData),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbol_intrusivo() {
        let mut reg = WidgetRegistry::new();
        let root = reg.create_box(true);
        let a = reg.create_label("a");
        let b = reg.create_label("b");
        let c = reg.create_label("c");
        reg.add_child(root, a).unwrap();
        reg.add_child(root, b).unwrap();
        reg.add_child(root, c).unwrap();
        assert_eq!(
            reg.children(root).collect::<Vec<_>>(),
            vec![a, b, c],
            "orden de inserción"
        );
        // Enlaces O(1): b está entre a y c.
        let node_b = reg.node(b).unwrap();
        assert_eq!(node_b.prev_sibling, Some(a));
        assert_eq!(node_b.next_sibling, Some(c));
        // Quitar del medio re-engancha hermanos.
        reg.remove_child(b).unwrap();
        assert_eq!(reg.node(a).unwrap().next_sibling, Some(c));
        assert_eq!(reg.node(c).unwrap().prev_sibling, Some(a));
        assert_eq!(reg.children(root).collect::<Vec<_>>(), vec![a, c]);
    }

    #[test]
    fn destruccion_e_ids_obsoletos() {
        let mut reg = WidgetRegistry::new();
        let root = reg.create_box(true);
        let child = reg.create_button("x", |_| {});
        reg.add_child(root, child).unwrap();
        assert!(reg.is_alive(child));
        reg.destroy_subtree(root).unwrap();
        assert!(!reg.is_alive(root));
        assert!(!reg.is_alive(child), "el subárbol murió completo");
        assert!(reg.node(root).is_err());
        // Reutilización de slot invalida el ID viejo (generaciones).
        let new_root = reg.create_box(false);
        assert_ne!(new_root, root);
    }

    #[test]
    fn ciclos_y_padres_duplicados() {
        let mut reg = WidgetRegistry::new();
        let a = reg.create_box(true);
        let b = reg.create_box(true);
        reg.add_child(a, b).unwrap();
        assert!(reg.add_child(b, a).is_err(), "ciclo detectado");
        let c = reg.create_label("c");
        assert!(reg.add_child(a, b).is_err(), "ya tiene padre");
        reg.remove_child(b).unwrap();
        reg.add_child(a, c).unwrap();
        assert!(reg.add_child(b, c).is_err(), "c ya tiene padre");
    }
}
