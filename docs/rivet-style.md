# rivet-style — Documentación definitiva

Subsistema de **estilos y temas** de Rivet.

- **Versión:** 2.0 (definitiva)
- **Fecha:** 2026-10-11
- **Capa:** 3 (presentación)
- **Depende de:** `rivet-foundation`; consumido por `rivet-ui` / `rivet-canvas`.
- **Reemplaza a:** CSS de GTK (GTK3 `GtkStyleContext` / GTK4 CSS).
- **Documento paraguas:** `../RIVET.md`

---

## 1. Propósito

Separar **apariencia** de **lógica de widget**. Define un lenguaje de estilos propio (DSL) legible y **precompilado a tablas planas** que los widgets consultan en tiempo de render. Sin parseo de CSS en runtime, sin cascada costosa, sin selectores complejos.

## 2. Modelo

- **DSL `.rivet-style`** (texto) → **compilador** → **tablas planas** (arrays de structs) indexadas por (tipo de widget, estado).
- En tiempo de ejecución, el widget solo **indexa** su estilo: **O(1)**.
- **Colores derivados por tema** (ADR-06): `lighten`/`darken`/`alpha`/`mix` se **calculan en tiempo de compilación del tema**, no por frame. El cambio de tema produce tablas precomputadas; intercambiar tema es **O(1)** (swap de tabla).

## 3. DSL (ejemplo)

```
widget button {
    background: #3a7bd5;
    color: #ffffff;
    border-radius: 6;
    padding: 8 12;
    font-size: 14;
    transition: background 150ms ease;

    state hover  { background: lighten(background, 10%); }
    state active { background: darken(background, 10%); }
    state disabled { color: alpha(color, 40%); }
}
theme dark {
    base: #1e1e1e;
    text: #e0e0e0;
}
```

## 4. Componentes

| Módulo | Responsabilidad |
|---|---|
| `lexer` / `parser` | Tokenizar y parsear el DSL. |
| `ast` | Representación intermedia. |
| `theme` | Resolución de variables de tema y **derivación de color**. |
| `compiler` | AST → tablas planas; validación. |
| `tables` | Estructuras `StyleTable`/`WidgetStyle` (indexables). |
| `runtime` | Consulta por (widget, estado); transiciones. |
| `error` | `StyleError`. |

## 5. Tablas y consulta

```rust
pub struct StyleTable { /* arrays planos por tipo */ }
pub struct WidgetStyle {
    pub background: Color, pub foreground: Color,
    pub border: Border, pub padding: Insets, pub font_size: f32, /* ... */
}
pub enum WidgetState { Normal, Hover, Active, Disabled, Focused }

impl StyleTable { pub fn get(&self, kind: WidgetKind, state: WidgetState) -> &WidgetStyle; }
```

## 6. Estados y transiciones

- Estados como **bitset**; el widget resuelve estado y consulta su fila.
- **Transiciones** declarativas (p. ej. `background 150ms ease`); animación interpolada por el motor de canvas, no por árbol de estilos.
- Prioridad de estados documentada (Disabled > Active > Focused > Hover > Normal) para evitar ambigüedad.

## 7. Temas

- Un tema define colores base, escala tipográfica y radios.
- **Cambio de tema = swap de `StyleTable`** (O(1)).
- Tema claro/oscuro y variantes; sin recalcular derivados en runtime.

## 8. Dependencias

- **Rust puro:** parser propio, utilidades de color (`palette`/propio).
- **C de sistema:** ninguno en runtime de estilos.

## 9. Alcance

**v1:** DSL, compilador a tablas, estados + transiciones, temas claro/oscuro, derivación de color en compilación.

**v1.x/v2:** herencia de estilos, selectores por clase, `@media`/`prefers-color-scheme`, carga de estilo externo en runtime (hot-reload), animaciones por keyframes.

## 10. Decisiones y correcciones aplicadas

- **Colores derivados por tema en codegen** (ADR-06): no hay coste por frame.
- Se retira cualquier afirmación de "0% de CPU" en el cambio de tema: la afirmación válida es que el cambio es **O(1)** (swap de tabla).
- Modelo **flat-table** en lugar de cascada CSS.

## 11. Métricas (objetivos, a medir — `../RIVET.md` §14)

Tamaño de las tablas en RAM, ns por consulta de estilo, µs de compilación del DSL, tiempo de swap de tema. Método: `criterion` + `heaptrack`.

## 12. Ideas preservadas (del diseño previo)

Separación estricta apariencia/lógica · estilos como datos indexables · O(1) en el camino caliente del render · compilación en lugar de interpretación en runtime.
