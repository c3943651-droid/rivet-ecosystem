//! Cola de render: comandos ordenables por capa y agrupados por estado.
//!
//! `docs/rivet-canvas.md` §2-3: la cola se agrupa por (capa, shader, textura)
//! al encolar para minimizar cambios de estado (`glBind*`, *state thrashing*);
//! el renderer ordena los lotes por capa.

use crate::color::Color;
use crate::geometry::{push_quad, Rect, Transform};
use crate::texture::TextureId;

/// Programa GL asociado a un lote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderKind {
    /// Relleno plano de color.
    Flat,
    /// Muestreo de textura × tint.
    Textured,
    /// Gaussiano separable (uso interno de los pases de blur).
    Blur,
}

/// Lote de geometría compartiendo estado (shader + textura) dentro de una capa.
#[derive(Debug, Clone)]
pub struct Batch {
    /// Capa de dibujo (mayor = encima).
    pub layer: u32,
    /// Programa a usar.
    pub kind: ShaderKind,
    /// Textura a fijar (solo `Textured`).
    pub texture: Option<TextureId>,
    /// Vértices intercalados: pos.xy uv.xy color.rgba (8 × f32).
    pub vertices: Vec<f32>,
    /// Índices de triángulos.
    pub indices: Vec<u32>,
}

/// Efecto de desenfoque gaussiano sobre una región ya dibujada.
#[derive(Debug, Clone, Copy)]
pub struct BlurCommand {
    /// Capa en la que se aplica (blurs de capa L se ejecutan tras dibujar los
    /// lotes con capa < L y antes que los de capa >= L).
    pub layer: u32,
    /// Región a desenfocar (píxeles, Y hacia abajo).
    pub rect: Rect,
    /// Radio del núcleo gaussiano en píxeles.
    pub radius: f32,
}

/// Cola de comandos de un frame.
///
/// Se rellena con `rect`/`textured_rect`/`blur` entre frames y se pasa a
/// `GraphicsContext::render`. Reutilizable con [`RenderQueue::clear`].
#[derive(Debug, Clone, Default)]
pub struct RenderQueue {
    batches: Vec<Batch>,
    blurs: Vec<BlurCommand>,
    clear: Option<Color>,
    vertex_count: usize,
}

impl RenderQueue {
    /// Cola vacía.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Color de fondo del frame (`glClear`); `None` = no limpiar.
    pub fn clear_color(&mut self, color: Option<Color>) {
        self.clear = color;
    }

    /// Rectángulo de color plano.
    pub fn rect(&mut self, layer: u32, rect: Rect, color: Color) {
        self.rect_transformed(layer, rect, color, &Transform::identity());
    }

    /// Rectángulo de color plano con transformación.
    pub fn rect_transformed(
        &mut self,
        layer: u32,
        rect: Rect,
        color: Color,
        transform: &Transform,
    ) {
        let uv = Rect::new(0.0, 0.0, 1.0, 1.0);
        let count = 6;
        let batch = self.batch_for(layer, ShaderKind::Flat, None);
        push_quad(
            &mut batch.vertices,
            &mut batch.indices,
            rect,
            uv,
            color,
            transform,
        );
        self.vertex_count += count;
    }

    /// Quad texturizado con tinte.
    pub fn textured_rect(
        &mut self,
        layer: u32,
        rect: Rect,
        texture: TextureId,
        uv: Rect,
        tint: Color,
        transform: &Transform,
    ) {
        let batch = self.batch_for(layer, ShaderKind::Textured, Some(texture));
        push_quad(
            &mut batch.vertices,
            &mut batch.indices,
            rect,
            uv,
            tint,
            transform,
        );
        self.vertex_count += 6;
    }

    /// Desenfoca la región indicada con radio en píxeles.
    ///
    /// El blur muestrea lo ya dibujado en capas inferiores; los lotes de la
    /// misma capa o superior se dibujan después encima.
    pub fn blur(&mut self, layer: u32, rect: Rect, radius: f32) {
        assert!(radius > 0.0, "blur: el radio debe ser > 0");
        self.blurs.push(BlurCommand {
            layer,
            rect,
            radius,
        });
    }

    /// Vacía la cola para reutilizarla (conserva la capacidad).
    pub fn clear(&mut self) {
        self.batches.clear();
        self.blurs.clear();
        self.clear = None;
        self.vertex_count = 0;
    }

    /// Color de fondo configurado.
    #[must_use]
    pub fn background(&self) -> Option<Color> {
        self.clear
    }

    /// Lotes encolados.
    #[must_use]
    pub fn batches(&self) -> &[Batch] {
        &self.batches
    }

    /// Comandos de blur encolados.
    #[must_use]
    pub fn blurs(&self) -> &[BlurCommand] {
        &self.blurs
    }

    /// Vértices totales encolados.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.vertex_count
    }

    /// Contabilidad interna de vértices (uso de `primitives`).
    pub(crate) fn count_vertices(&mut self, added: usize) {
        self.vertex_count += added;
    }

    /// Devuelve el lote activo, creándolo si cambia (capa, shader, textura).
    pub(crate) fn batch_for(
        &mut self,
        layer: u32,
        kind: ShaderKind,
        texture: Option<TextureId>,
    ) -> &mut Batch {
        let compatible = self
            .batches
            .last()
            .is_some_and(|b| b.layer == layer && b.kind == kind && b.texture == texture);
        if !compatible {
            self.batches.push(Batch {
                layer,
                kind,
                texture,
                vertices: Vec::new(),
                indices: Vec::new(),
            });
        }
        self.batches.last_mut().expect("lote recién creado")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lotes_se_agrupan_por_estado() {
        let mut q = RenderQueue::new();
        q.rect(0, Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        q.rect(0, Rect::new(1.0, 0.0, 1.0, 1.0), Color::BLACK); // mismo estado → mismo lote
        q.rect(1, Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE); // capa distinta → nuevo lote
        assert_eq!(q.batches().len(), 2);
        assert_eq!(q.batches()[0].layer, 0);
        assert_eq!(q.batches()[0].indices.len(), 12);
        assert_eq!(q.batches()[1].layer, 1);
        assert_eq!(q.vertex_count(), 18);
    }

    #[test]
    fn texturas_distintas_separan_lotes() {
        let mut q = RenderQueue::new();
        let t = Transform::identity();
        q.textured_rect(
            0,
            Rect::new(0.0, 0.0, 2.0, 2.0),
            1,
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::WHITE,
            &t,
        );
        q.textured_rect(
            0,
            Rect::new(2.0, 0.0, 2.0, 2.0),
            2,
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::WHITE,
            &t,
        );
        q.textured_rect(
            0,
            Rect::new(4.0, 0.0, 2.0, 2.0),
            1,
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::WHITE,
            &t,
        );
        assert_eq!(q.batches().len(), 3);
    }

    #[test]
    fn clear_reutiliza_capacidad() {
        let mut q = RenderQueue::new();
        q.rect(0, Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        q.blur(1, Rect::new(0.0, 0.0, 4.0, 4.0), 2.0);
        q.clear_color(Some(Color::BLACK));
        q.clear();
        assert!(q.batches().is_empty());
        assert!(q.blurs().is_empty());
        assert_eq!(q.background(), None);
    }
}
