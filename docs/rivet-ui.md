# rivet-ui — Documentación definitiva

Subsistema de **widgets y UI** de Rivet.

- **Versión:** 2.0 (definitiva)
- **Fecha:** 2026-10-11
- **Capa:** 4 (nivel más alto del toolkit)
- **Depende de:** `rivet-foundation`, `rivet-canvas`, `rivet-typography`, `rivet-style`, `rivet-image`, `rivet-window` (eventos).
- **Reemplaza a:** el nivel de widgets de GTK (`GtkWidget`, cajas, botones, etc.).
- **Documento paraguas:** `../RIVET.md`

---

## 1. Propósito

Construir la **jerarquía de widgets**, enrutar eventos, gestionar foco, medir y posicionar (layout), y ordenar el dibujo. `rivet-ui` es la capa que el desarrollador de apps usa directamente.

## 2. Almacén de widgets (ADR-03)

**Corrección de diseño:** NO se usa un slab homogéneo que fuerce un `enum` gigante de todos los tipos de widget. En su lugar:

- Un **nodo común** `WidgetNode` con los datos compartidos por todos los widgets:
  - `id: WidgetId`, `parent`, `first_child`/`last_child`, `next_sibling`/`prev_sibling`, `kind: WidgetKind`, bits de estado, layout flags, geometría calculada.
- **Slabs por tipo** (uno por clase de widget: `ButtonSlab`, `LabelSlab`, `ContainerSlab`, ...) que guardan **solo los datos específicos** de cada tipo.

```rust
pub struct WidgetNode {
    pub generation: u32,
    pub parent: Option<WidgetId>,
    pub first_child: Option<WidgetId>,
    pub last_child: Option<WidgetId>,
    pub next_sibling: Option<WidgetId>,
    pub prev_sibling: Option<WidgetId>,
    pub kind: WidgetKind,
    pub state: WidgetStateBits,
    pub rect: Rect,        // posición/tamaño resueltos por layout
}
// Un slab por tipo:
struct ButtonSlab { entries: Vec<Entry<ButtonData>> }
// ...uno por cada tipo de widget
```

- **Ventaja:** el nodo común es pequeño y homogéneo; los datos específicos viven en slabs tipados sin *padding* por el widget más grande.
- **Corrección:** se retira la cifra aislada de "24 bytes por widget" — el coste real es `WidgetNode` + entrada del slab del tipo + datos del widget. Se **mide** (ver §15).

## 3. Hijos: enlaces intrusivos (ADR-04)

- La relación padre↔hijo se modela con **enlaces intrusivos** (`first_child`/`last_child` + `next_sibling`/`prev_sibling`), **no** con `Vec<WidgetId>` por nodo.
- Ventajas: sin `Vec` por nodo (menos asignaciones, menos fragmentación), relaciones O(1), recorrido predecible.
- Insertar/quitar un hijo es O(1). El orden de hijos es el orden de inserción (o reordenable por enlace).

## 4. Identidad

- **`WidgetId` = 64 bits**: 32 bits índice + 32 bits generación (coherente con `rivet-foundation`).
- El acceso es por ID validado contra generación → detecta *use-after-free*.

## 5. Componentes

| Módulo | Responsabilidad |
|---|---|
| `widget` | `WidgetNode`, tipos concretos, árbol. |
| `registry` | `WidgetRegistry`: alta/baja, validación de IDs. |
| `event` | `EventManager` / `EventDispatcher`: enrutado y *bubbling*. |
| `hit` | `HitTester`: de coordenada → widget (recorrido del árbol de atrás hacia delante). |
| `focus` | `FocusManager`: foco por teclado, tab-order, foco por puntero. |
| `layout` | Medición + posicionamiento (box model / constraints). |
| `render` | Recorre el árbol y emite comandos a `rivet-canvas`. |
| `builder` | DSL/constructor declarativo de jerarquía. |
| `error` | `UiError`. |

## 6. Registro

```rust
pub type WidgetId = u64;
pub struct WidgetRegistry { /* nodos + slabs por tipo */ }
impl WidgetRegistry {
    pub fn create(&mut self, kind: WidgetKind) -> WidgetId;
    pub fn destroy(&mut self, id: WidgetId);
    pub fn node(&self, id: WidgetId) -> Option<&WidgetNode>;
    pub fn add_child(&mut self, parent: WidgetId, child: WidgetId);
    pub fn remove_child(&mut self, parent: WidgetId, child: WidgetId);
}
```

## 7. Eventos

- `rivet-window` entrega `InputEvent`; `rivet-ui` los convierte a eventos de widget.
- **Fases:** *capture* (raíz→hoja) → *target* → *bubble* (hoja→raíz), con posibilidad de detener propagación.
- **Hit-testing:** por geometría + z-order (hijos al frente); respeta `pointer-events`/regiones.
- **Foco:** `FocusManager` mantiene el widget enfocado, el orden `Tab`, y el foco por puntero; los eventos de teclado van al foco.

## 8. Layout

- Modelo tipo **caja** (padding/border/margin) con tamaños min/preferido/expandible.
- Contenedores: `Box` (horizontal/vertical), `Grid`, `Overlay`, `ScrollView`, alineación y espaciado.
- **Dos pasadas:** medir (tamaños deseados) → posicionar (asignar rects). v1 es *single-pass* simplificado; layout constraints avanzados en v1.x.
- Texto: consulta métricas de `rivet-typography` para medir.

## 9. Dibujo

- Recorrido en orden de árbol, saltando lo no visible/`clip`.
- Por widget: consulta estilo (`rivet-style`), estado, y emite comandos ordenados a `rivet-canvas`.
- Redibujado por **dirty regions** (invalidar solo lo que cambió) para no repintar todo — objetivo medible.

## 10. API pública (orientativa)

```rust
pub struct Widget;
pub struct Button; pub struct Label; pub struct Box_; pub struct TextInput;
pub struct Application;                 // ciclo init/run/quit
pub enum UiEvent { Pointer(..), Key(..), .. }
```

El usuario de la crate: crea widgets, los anida, conecta señales/callbacks, y ejecuta el bucle.

## 11. Widgets v1

`Container`/`Box`, `Button`, `Label`, `Image`, `TextInput`, `List`/`Scroll`, `Panel`/superficie (para apps semilla: panel y lanzador). Catálogo crece en v1.x.

## 12. Dependencias

- Internas: foundation, canvas, typography, style, image, window.
- **Rust puro** + C de sistema ya permitido por las capas inferiores.

## 13. Alcance

**v1:** almacén (nodo + slabs), hijos intrusivos, IDs 64 bits, registro, hit-test, foco, enrutado con capture/bubble, layout básico (box/list), widgets esenciales, redibujado por daño incremental.

**v1.x (por medición):** más widgets, layout tipo flex/grid completo, accesibilidad (roles, `AT-SPI`), IME, drag & drop de alto nivel, animaciones de layout.

**Fuera de v1:** motor de estilos completo (ver `rivet-style`), X11/XWayland, bindings multi-lenguaje.

## 14. Decisiones y correcciones aplicadas

- **`WidgetNode` + slabs por tipo** (ADR-03); se retira el "almacén homogéneo con `Vec<WidgetId>` de hijos".
- **Hijos por enlaces intrusivos** (ADR-04).
- **IDs de 64 bits** (ADR de §7 del paraguas).
- Se retira la cifra "24 bytes por widget" (ADR-08): depende del tipo; se mide.
- Sin afirmaciones de "0% CPU".

## 15. Métricas (objetivos, a medir — `../RIVET.md` §14)

ns de `create`/`destroy`/`add_child`, ns de hit-test sobre N widgets, µs de layout de un árbol típico, RAM por widget (por tipo), eventos/s despachados, coste del daño incremental. Método: `criterion` + `heaptrack` + `FrameStats`.

## 16. Ideas preservadas (del diseño previo)

`WidgetRegistry` como autoridad de creación · eventos con fases capture/bubble · `HitTester` geométrico · `FocusManager` separado · layout declarativo · separación estricta entre estado, estilo y dibujo.
